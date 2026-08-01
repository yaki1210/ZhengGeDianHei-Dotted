// 正格点黑 16 字体切换器 - portable font hot-swapper
// v2: force-enable (sweep conflicting registrations + restart FontCache)
//     and status inspection (registry scan + SHA256 fingerprint match).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.Drawing.Text;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.Principal;
using System.ServiceProcess;
using System.Text;
using System.Windows.Forms;
using Microsoft.Win32;

public class FontSwitcher : Form
{
    // ------------------------------------------------------------ win32
    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
    static extern int AddFontResourceW(string lpFileName);
    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
    static extern bool RemoveFontResourceW(string lpFileName);
    [DllImport("user32.dll")]
    static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam,
        IntPtr lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
    static readonly IntPtr HWND_BROADCAST = new IntPtr(0xffff);
    const uint WM_FONTCHANGE = 0x001D;
    const uint SMTO_ABORTIFHUNG = 0x0002;

    // ------------------------------------------------------------ config
    const string REG_KEY = @"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";
    const string REG_VALUE = "ZhengGeDianHei 16 (TrueType)";
    const string NAME_PREFIX = "ZhengGeDianHei";
    const string MARKER = "ZGDH16.active";
    const string CONSOLE_BACKUP = "console-font-backup.json";
    const string FAMILY = "正格点黑 16";
    const string HW_SUFFIX = "-HW";

    static string VariantFile(Variant v, bool hw)
    {
        return hw ? v.File.Replace(".ttf", HW_SUFFIX + ".ttf") : v.File;
    }

    class Variant
    {
        public string Key, Label, File;
        public Variant(string k, string l, string f) { Key = k; Label = l; File = f; }
    }
    static readonly Variant[] Variants =
    {
        new Variant("original", "原版（实心方块）",   "ZhengGeDianHei16-Original.ttf"),
        new Variant("sq70",     "方块 70%（大间隙）", "ZhengGeDianHei16-Squares70.ttf"),
        new Variant("sq80",     "方块 80%（小间隙）", "ZhengGeDianHei16-Squares80.ttf"),
        new Variant("dot70",    "圆点 70%（轻盈）",   "ZhengGeDianHei16-Dots70.ttf"),
        new Variant("dot80",    "圆点 80%（均衡）",   "ZhengGeDianHei16-Dots80.ttf"),
        new Variant("dot90",    "圆点 90%（饱满）",   "ZhengGeDianHei16-Dots90.ttf"),
    };

    static string FontsSrcDir =
        Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "fonts");
    static string InstallDir = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        @"Microsoft\Windows\Fonts");
    static string SystemFontsDir =
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows), "Fonts");

    class Browser
    {
        public string Name, Exe, Dir;
        public Browser(string n, string e, string d) { Name = n; Exe = e; Dir = d; }
    }
    static string LocalAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
    static readonly Browser[] Browsers =
    {
        new Browser("Google Chrome", "chrome", Path.Combine(LocalAppData, @"Google\Chrome\User Data\Default")),
        new Browser("Microsoft Edge", "msedge", Path.Combine(LocalAppData, @"Microsoft\Edge\User Data\Default")),
    };

    // ------------------------------------------------------------ ui
    readonly Dictionary<Variant, RadioButton> radios = new Dictionary<Variant, RadioButton>();
    CheckBox chkCompat, chkChrome, chkEdge;
    Label preview16, preview32, status;
    PrivateFontCollection pfc;
    Font f16, f32;

    public FontSwitcher()
    {
        Text = "正格点黑 16 字体切换器";
        ClientSize = new Size(586, 718);
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        try { Icon = new Icon(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "icon.ico")); }
        catch { }

        var grp = new GroupBox { Text = "选择像素样式", Left = 12, Top = 10, Width = 562, Height = 146 };
        for (int i = 0; i < Variants.Length; i++)
        {
            var v = Variants[i];
            var rb = new RadioButton
            {
                Text = v.Label,
                Left = 14 + (i % 2) * 280,
                Top = 26 + (i / 2) * 28,
                Width = 268,
                Tag = v,
            };
            rb.CheckedChanged += (s, e) => { if (((RadioButton)s).Checked) UpdatePreview(v); };
            radios[v] = rb;
            grp.Controls.Add(rb);
        }
        chkCompat = new CheckBox
        {
            Text = "窄终端兼容模式（半宽符号，防止与下一字符重叠）",
            Left = 14, Top = 116, Width = 534,
            ForeColor = Color.FromArgb(60, 60, 60),
        };
        chkCompat.CheckedChanged += (s, e) => UpdatePreview(Selected());
        grp.Controls.Add(chkCompat);
        Controls.Add(grp);

        var grpP = new GroupBox { Text = "预览（应用程序中的真实效果）", Left = 12, Top = 162, Width = 562, Height = 190 };
        preview16 = new Label { Left = 14, Top = 24, Width = 534, Height = 48 };
        preview32 = new Label { Left = 14, Top = 78, Width = 534, Height = 100 };
        grpP.Controls.Add(preview16);
        grpP.Controls.Add(preview32);
        Controls.Add(grpP);

        var grpB = new GroupBox { Text = "应用到系统程序", Left = 12, Top = 360, Width = 562, Height = 190 };
        chkChrome = new CheckBox { Text = "Google Chrome", Left = 14, Top = 26, Width = 160, Checked = true };
        chkEdge = new CheckBox { Text = "Microsoft Edge", Left = 186, Top = 26, Width = 160, Checked = false };
        var btnApplyWeb = new Button { Text = "应用字体到浏览器", Left = 14, Top = 56, Width = 156, Height = 32 };
        btnApplyWeb.Click += (s, e) => ApplyBrowserFont();
        var btnRestoreWeb = new Button { Text = "恢复浏览器字体", Left = 178, Top = 56, Width = 156, Height = 32 };
        btnRestoreWeb.Click += (s, e) => RestoreBrowserFont();
        var btnNotepad = new Button { Text = "记事本设置指引", Left = 342, Top = 56, Width = 156, Height = 32 };
        btnNotepad.Click += (s, e) => ShowNotepadGuide();
        var hintB = new Label
        {
            Text = "修改前自动备份浏览器原配置，可随时一键恢复。请先用上方「应用切换」安装字体；浏览器需已关闭（可自动关闭），重新启动浏览器后生效。",
            Left = 14, Top = 100, Width = 534, Height = 38,
            ForeColor = Color.FromArgb(90, 90, 90),
        };
        var sepT = new Label { Text = "终端", Left = 14, Top = 144, Width = 534, Height = 16, ForeColor = Color.FromArgb(90, 90, 90) };
        var btnTermApply = new Button { Text = "应用终端字体", Left = 14, Top = 158, Width = 156, Height = 28 };
        btnTermApply.Click += (s, e) => ApplyTerminalFont();
        var btnTermRestore = new Button { Text = "恢复终端字体", Left = 178, Top = 158, Width = 156, Height = 28 };
        btnTermRestore.Click += (s, e) => RestoreTerminalFont();
        var btnSysGuide = new Button { Text = "系统字体指引", Left = 342, Top = 158, Width = 156, Height = 28 };
        btnSysGuide.Click += (s, e) => ShowSystemFontGuide();
        grpB.Controls.Add(chkChrome);
        grpB.Controls.Add(chkEdge);
        grpB.Controls.Add(btnApplyWeb);
        grpB.Controls.Add(btnRestoreWeb);
        grpB.Controls.Add(btnNotepad);
        grpB.Controls.Add(hintB);
        grpB.Controls.Add(sepT);
        grpB.Controls.Add(btnTermApply);
        grpB.Controls.Add(btnTermRestore);
        grpB.Controls.Add(btnSysGuide);
        Controls.Add(grpB);

        var btnApply = new Button { Text = "应用切换", Left = 12, Top = 562, Width = 133, Height = 40 };
        btnApply.Click += (s, e) => Apply(false);
        var btnForce = new Button { Text = "强制启用", Left = 155, Top = 562, Width = 133, Height = 40 };
        btnForce.Click += (s, e) => Apply(true);
        var btnCheck = new Button { Text = "检查状态", Left = 298, Top = 562, Width = 133, Height = 40 };
        btnCheck.Click += (s, e) => MessageBox.Show(Diagnose(), "字体状态检查",
            MessageBoxButtons.OK, MessageBoxIcon.Information);
        var btnOff = new Button { Text = "停用字体", Left = 441, Top = 562, Width = 133, Height = 40 };
        btnOff.Click += (s, e) => Disable();
        Controls.Add(btnApply);
        Controls.Add(btnForce);
        Controls.Add(btnCheck);
        Controls.Add(btnOff);

        var note = new Label
        {
            Text = "切换后请重启相关程序。若样式没变化：先点「检查状态」看是否有同名冲突，再点「强制启用」彻底清扫并重启系统字体缓存。\r\n滚动/缩放卡顿：记事本等实时渲染程序建议选「原版（实心方块）」，圆点/方块变体轮廓数量多、渲染开销大。\r\n窄终端兼容模式：符号半宽化（等比 8px），供 Windows Terminal 窄模式等只给 1 格的终端使用；CJK 终端/记事本/浏览器请保持不勾选，符号为全宽 16px。",
            Left = 12, Top = 612, Width = 562, Height = 58,
            ForeColor = Color.FromArgb(90, 90, 90),
        };
        Controls.Add(note);

        status = new Label { Left = 12, Top = 676, Width = 562, Height = 40, ForeColor = Color.FromArgb(0, 110, 40) };
        Controls.Add(status);

        if (!Directory.Exists(FontsSrcDir))
            MessageBox.Show("未找到 fonts 文件夹，请保持切换器与 fonts 文件夹在同一目录。",
                "提示", MessageBoxButtons.OK, MessageBoxIcon.Warning);

        var active = DetectActive();
        radios[active].Checked = true;
        RefreshStatus();
    }

    // ------------------------------------------------------------ helpers
    static bool IsAdmin()
    {
        var id = WindowsIdentity.GetCurrent();
        return new WindowsPrincipal(id).IsInRole(WindowsBuiltInRole.Administrator);
    }

    static void Broadcast()
    {
        UIntPtr r;
        SendMessageTimeout(HWND_BROADCAST, WM_FONTCHANGE, UIntPtr.Zero, IntPtr.Zero,
            SMTO_ABORTIFHUNG, 2000, out r);
    }

    static string Sha256(string path)
    {
        using (var sha = SHA256.Create())
        using (var fs = File.OpenRead(path))
            return BitConverter.ToString(sha.ComputeHash(fs));
    }

    /// <summary>Identify a font file by comparing its hash with the 12 sources.</summary>
    string IdentifyByHash(string path)
    {
        if (!File.Exists(path)) return "（文件不存在）";
        string h;
        try { h = Sha256(path); }
        catch { return "（无法读取）"; }
        foreach (var v in Variants)
        {
            foreach (bool hw in new[] { false, true })
            {
                string src = Path.Combine(FontsSrcDir, VariantFile(v, hw));
                if (File.Exists(src) && Sha256(src) == h)
                    return (hw ? "[窄终端兼容] " : "") + v.Label;
            }
        }
        return "未知文件（不是切换器自带的 12 个文件）";
    }

    /// <summary>All registry font values whose name mentions ZhengGeDianHei.</summary>
    static List<Tuple<string, string, RegistryKey>> FindRegEntries()
    {
        var list = new List<Tuple<string, string, RegistryKey>>();
        foreach (var root in new[] { Registry.CurrentUser, Registry.LocalMachine })
        {
            RegistryKey k = null;
            try
            {
                k = root.OpenSubKey(REG_KEY, false);
                if (k == null) continue;
                foreach (var name in k.GetValueNames())
                    if (name.IndexOf(NAME_PREFIX, StringComparison.OrdinalIgnoreCase) >= 0)
                        list.Add(Tuple.Create(name, k.GetValue(name) as string ?? "", root));
            }
            catch { }
            finally { if (k != null) k.Close(); }
        }
        return list;
    }

    Variant DetectActive()
    {
        try
        {
            string mark = Path.Combine(InstallDir, MARKER);
            if (File.Exists(mark))
            {
                string key = File.ReadAllText(mark).Trim();
                foreach (var v in Variants)
                    if (v.Key == key) return v;
            }
        }
        catch { }
        return Variants[0];
    }

    void RefreshStatus()
    {
        var entries = FindRegEntries();
        if (entries.Count == 0)
        {
            status.Text = "当前状态：未安装 ZhengGeDianHei 16";
            return;
        }
        var sb = new StringBuilder("当前状态：");
        foreach (var e in entries)
        {
            string where = e.Item3 == Registry.CurrentUser ? "用户" : "系统";
            sb.Append(string.Format("[{0}] {1}  ", where, IdentifyByHash(e.Item2)));
        }
        status.Text = sb.ToString();
    }

    // ------------------------------------------------------------ sweep
    /// <summary>Remove every same-named registration & leftover file we can reach.</summary>
    static void SweepConflicts(bool includeSystem, StringBuilder log)
    {
        foreach (var e in FindRegEntries())
        {
            bool isSystem = e.Item3 == Registry.LocalMachine;
            if (isSystem && !includeSystem) continue;
            try { if (File.Exists(e.Item2)) RemoveFontResourceW(e.Item2); } catch { }
            try
            {
                using (var k = e.Item3.OpenSubKey(REG_KEY, true))
                    if (k != null) k.DeleteValue(e.Item1, false);
                log.AppendLine("已移除注册项 [" + (isSystem ? "系统" : "用户") + "] " + e.Item1);
            }
            catch (Exception ex) { log.AppendLine("移除注册项失败：" + ex.Message); }
        }
        // leftover files in the per-user fonts dir
        try
        {
            foreach (var f in Directory.GetFiles(InstallDir, NAME_PREFIX + "*.ttf"))
            {
                try { RemoveFontResourceW(f); } catch { }
                try { File.Delete(f); log.AppendLine("已删除残留文件 " + Path.GetFileName(f)); }
                catch { }
            }
        }
        catch { }
        // system-wide conflicting files (needs admin)
        if (includeSystem)
        {
            try
            {
                foreach (var f in Directory.GetFiles(SystemFontsDir, NAME_PREFIX + "*.ttf"))
                {
                    try { File.Delete(f); log.AppendLine("已删除系统字体文件 " + Path.GetFileName(f)); }
                    catch (Exception ex) { log.AppendLine("系统字体文件删除失败（可忽略，重启后失效）：" + ex.Message); }
                }
            }
            catch { }
        }
        string mark = Path.Combine(InstallDir, MARKER);
        try { if (File.Exists(mark)) File.Delete(mark); } catch { }
    }

    static void RestartFontCache(StringBuilder log)
    {
        try
        {
            var sc = new ServiceController("FontCache");
            if (sc.Status != ServiceControllerStatus.Stopped)
            {
                sc.Stop();
                sc.WaitForStatus(ServiceControllerStatus.Stopped, TimeSpan.FromSeconds(15));
            }
            sc.Start();
            sc.WaitForStatus(ServiceControllerStatus.Running, TimeSpan.FromSeconds(15));
            log.AppendLine("已重启系统字体缓存服务（FontCache）");
        }
        catch (Exception ex) { log.AppendLine("字体缓存服务重启失败：" + ex.Message + "（建议重启电脑）"); }
    }

    static string InstallFresh(Variant v, string file, StringBuilder log)
    {
        Directory.CreateDirectory(InstallDir);
        // unique file name every time -> defeats per-file font caching
        string dest = Path.Combine(InstallDir,
            string.Format("ZhengGeDianHei16-{0}-{1}-{2:MMddHHmmss}.ttf",
                v.Key, file.EndsWith(HW_SUFFIX + ".ttf") ? "hw" : "fw", DateTime.Now));
        File.Copy(Path.Combine(FontsSrcDir, file), dest, true);
        using (var k = Registry.CurrentUser.CreateSubKey(REG_KEY))
            k.SetValue(REG_VALUE, dest, RegistryValueKind.String);
        AddFontResourceW(dest);
        File.WriteAllText(Path.Combine(InstallDir, MARKER), v.Key);
        log.AppendLine("已安装 " + v.Label + " -> " + Path.GetFileName(dest));
        return dest;
    }

    // ------------------------------------------------------------ actions
    Variant Selected()
    {
        foreach (var kv in radios) if (kv.Value.Checked) return kv.Key;
        return null;
    }

    void Apply(bool force)
    {
        var sel = Selected();
        if (sel == null) return;
        bool hw = chkCompat.Checked;
        string file = VariantFile(sel, hw);
        if (!File.Exists(Path.Combine(FontsSrcDir, file)))
        {
            MessageBox.Show("缺少字体文件：" + file, "错误",
                MessageBoxButtons.OK, MessageBoxIcon.Error);
            return;
        }

        bool systemConflict = false;
        foreach (var e in FindRegEntries())
            if (e.Item3 == Registry.LocalMachine) systemConflict = true;

        if (force && !IsAdmin())
        {
            var r = MessageBox.Show(
                "强制启用需要管理员权限，用于：\n" +
                "  1. 清除系统级同名安装（如果有）\n" +
                "  2. 重启系统字体缓存服务，彻底踢掉旧字体\n\n" +
                "将以管理员身份重新打开本切换器并自动完成启用，是否继续？",
                "强制启用", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (r != DialogResult.Yes) return;
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = Application.ExecutablePath,
                    Arguments = "forceenable " + sel.Key +
                                (chkCompat.Checked ? " hw" : ""),
                    Verb = "runas",
                });
                Application.Exit();
            }
            catch { MessageBox.Show("已取消管理员授权。"); }
            return;
        }

        var log = new StringBuilder();
        try
        {
            if (force) RestartFontCache(log);          // release file locks first
            SweepConflicts(force, log);
            InstallFresh(sel, file, log);
            Broadcast();
            RefreshStatus();
            string warn = (!force && systemConflict)
                ? "\n\n⚠ 检测到系统级同名安装，可能仍会压住新字体，建议再点一次「强制启用」。"
                : "";
            MessageBox.Show("已启用「" + sel.Label + "」。\n部分程序需要重启后才会显示新样式。" +
                warn + "\n\n" + log, "完成", MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
        catch (Exception ex)
        {
            MessageBox.Show("操作失败：\n" + ex.Message + "\n\n" + log, "错误",
                MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    void Disable()
    {
        var log = new StringBuilder();
        try
        {
            SweepConflicts(false, log);
            Broadcast();
            RefreshStatus();
            MessageBox.Show("已停用并卸载 ZhengGeDianHei 16。\n\n" + log,
                "完成", MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
        catch (Exception ex)
        {
            MessageBox.Show("停用失败：\n" + ex.Message, "错误",
                MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    string Diagnose()
    {
        var sb = new StringBuilder();
        sb.AppendLine("【注册表中的同名条目】");
        var entries = FindRegEntries();
        if (entries.Count == 0) sb.AppendLine("  （无：系统未安装该字体）");
        foreach (var e in entries)
        {
            string where = e.Item3 == Registry.CurrentUser ? "当前用户" : "系统级";
            string exists = File.Exists(e.Item2) ? "存在" : "缺失！";
            sb.AppendLine(string.Format("  [{0}] {1}\n      -> {2}（{3}）",
                where, e.Item1, e.Item2, exists));
            sb.AppendLine("      指纹识别 = " + IdentifyByHash(e.Item2));
        }
        sb.AppendLine();
        sb.AppendLine("【切换器标记】");
        string mark = Path.Combine(InstallDir, MARKER);
        if (File.Exists(mark))
        {
            string key = File.ReadAllText(mark).Trim();
            string label = key;
            foreach (var v in Variants) if (v.Key == key) label = v.Label;
            sb.AppendLine("  " + label);
        }
        else sb.AppendLine("  （无）");
        sb.AppendLine();
        if (entries.Count > 1)
            sb.AppendLine("⚠ 结论：存在多个同名条目互相冲突！请点「强制启用」清扫。");
        else if (entries.Count == 1)
            sb.AppendLine("✓ 结论：注册状态正常。若程序里仍无变化，请重启该程序；还不行就点「强制启用」刷新系统字体缓存。");
        return sb.ToString();
    }

    // ------------------------------------------------------------ browser / notepad
    static string BrowserPrefs(Browser b) { return Path.Combine(b.Dir, "Preferences"); }
    static string BrowserBackup(Browser b) { return Path.Combine(b.Dir, "Preferences.zgdh-backup"); }

    static bool BrowserRunning(Browser b)
    {
        return Process.GetProcessesByName(b.Exe).Length > 0;
    }

    static void CloseBrowser(Browser b)
    {
        foreach (var p in Process.GetProcessesByName(b.Exe))
        {
            try { if (!p.HasExited) p.CloseMainWindow(); } catch { }
        }
        for (int i = 0; i < 30 && BrowserRunning(b); i++)
            System.Threading.Thread.Sleep(200);
        foreach (var p in Process.GetProcessesByName(b.Exe))
        {
            try { if (!p.HasExited) p.Kill(); } catch { }
        }
    }

    void ApplyBrowserFont()
    {
        if (!chkChrome.Checked && !chkEdge.Checked)
        {
            MessageBox.Show("请至少勾选一个浏览器。", "提示",
                MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return;
        }
        var log = new StringBuilder();
        int done = 0;
        foreach (var b in Browsers)
        {
            bool sel = b.Name == "Google Chrome" ? chkChrome.Checked : chkEdge.Checked;
            if (!sel) continue;
            string prefs = BrowserPrefs(b);
            if (!File.Exists(prefs))
            {
                log.AppendLine(b.Name + "：未找到配置文件（未安装？）");
                continue;
            }
            if (BrowserRunning(b))
            {
                var r = MessageBox.Show(b.Name + " 正在运行，需要先关闭才能修改字体。\n" +
                    "是否自动关闭？（未保存的网页内容可能丢失）", "需要关闭浏览器",
                    MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (r != DialogResult.Yes) { log.AppendLine(b.Name + "：已跳过（浏览器未关闭）"); continue; }
                CloseBrowser(b);
                if (BrowserRunning(b)) { log.AppendLine(b.Name + "：关闭失败，已跳过"); continue; }
            }
            string bak = BrowserBackup(b);
            if (!File.Exists(bak))
            {
                try { File.Copy(prefs, bak); log.AppendLine(b.Name + "：已备份原配置"); }
                catch (Exception ex) { log.AppendLine(b.Name + "：备份失败：" + ex.Message); continue; }
            }
            try
            {
                string text = File.ReadAllText(prefs, Encoding.UTF8);
                var root = JParse(text) as Dictionary<string, object>;
                if (root == null) throw new Exception("配置文件不是有效的 JSON");
                if (!JSetFonts(root, FAMILY))
                {
                    log.AppendLine(b.Name + "：标准字体已是「" + FAMILY + "」，无需修改");
                    continue;
                }
                File.WriteAllText(prefs, JSerialize(root), new UTF8Encoding(false));
                log.AppendLine(b.Name + "：已应用「" + FAMILY + "」");
                done++;
            }
            catch (Exception ex)
            {
                log.AppendLine(b.Name + "：写入失败（原配置已备份，未损坏）：" + ex.Message);
            }
        }
        string tail = done == 0 ? "" : "\n\n重新启动浏览器后生效。";
        MessageBox.Show(log.ToString() + tail, "浏览器字体",
            MessageBoxButtons.OK, MessageBoxIcon.Information);
    }

    void RestoreBrowserFont()
    {
        if (!chkChrome.Checked && !chkEdge.Checked)
        {
            MessageBox.Show("请至少勾选一个浏览器。", "提示",
                MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return;
        }
        var log = new StringBuilder();
        foreach (var b in Browsers)
        {
            bool sel = b.Name == "Google Chrome" ? chkChrome.Checked : chkEdge.Checked;
            if (!sel) continue;
            string prefs = BrowserPrefs(b);
            string bak = BrowserBackup(b);
            if (!File.Exists(bak))
            {
                log.AppendLine(b.Name + "：没有找到备份（从未用本工具修改过？）");
                continue;
            }
            if (BrowserRunning(b))
            {
                var r = MessageBox.Show(b.Name + " 正在运行，需要先关闭才能恢复。\n是否自动关闭？",
                    "需要关闭浏览器", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (r != DialogResult.Yes) { log.AppendLine(b.Name + "：已跳过（浏览器未关闭）"); continue; }
                CloseBrowser(b);
                if (BrowserRunning(b)) { log.AppendLine(b.Name + "：关闭失败，已跳过"); continue; }
            }
            try { File.Copy(bak, prefs, true); log.AppendLine(b.Name + "：已恢复原始配置"); }
            catch (Exception ex) { log.AppendLine(b.Name + "：恢复失败：" + ex.Message); }
        }
        MessageBox.Show(log.ToString() + "\n\n重新启动浏览器后生效。", "恢复浏览器字体",
            MessageBoxButtons.OK, MessageBoxIcon.Information);
    }

    void ShowNotepadGuide()
    {
        var r = MessageBox.Show(
            "Windows 11 记事本的字体设置存放在应用数据中，没有公开接口，脚本无法安全修改，请手动设置一次：\n\n" +
            "1. 打开记事本\n" +
            "2. 点右上角「设置」图标（齿轮）\n" +
            "3. 选「字体」：字体列表里选择「正格点黑 16」\n" +
            "4. 按需调整字号（点阵字建议 12~14）\n\n" +
            "设置立即生效，所有新记事本窗口都会保持该字体。\n\n是否现在打开记事本？",
            "记事本设置指引", MessageBoxButtons.YesNo, MessageBoxIcon.Information);
        if (r == DialogResult.Yes) Process.Start("notepad.exe");
    }

    // ------------------------------------------------------------ terminal
    static string WtSettingsPath()
    {
        string pkg = Path.Combine(LocalAppData,
            @"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json");
        if (File.Exists(pkg)) return pkg;
        return Path.Combine(LocalAppData, @"Microsoft\Windows Terminal\settings.json");
    }

    static bool IsAppRunning(string exe) { return Process.GetProcessesByName(exe).Length > 0; }

    static void CloseApp(string exe)
    {
        foreach (var p in Process.GetProcessesByName(exe))
        {
            try { if (!p.HasExited) p.CloseMainWindow(); } catch { }
        }
        for (int i = 0; i < 30 && IsAppRunning(exe); i++)
            System.Threading.Thread.Sleep(200);
        foreach (var p in Process.GetProcessesByName(exe))
        {
            try { if (!p.HasExited) p.Kill(); } catch { }
        }
    }

    void ApplyWt(string path, StringBuilder log)
    {
        try
        {
            string text = File.ReadAllText(path, Encoding.UTF8);
            var root = JParse(text) as Dictionary<string, object>;
            if (root == null) throw new Exception("设置文件不是有效的 JSON");
            if (!JSetTerminalFont(root, FAMILY, 16))
            {
                log.AppendLine("Windows Terminal：字体已是「" + FAMILY + "」，无需修改");
                return;
            }
            File.WriteAllText(path, JSerialize(root), new UTF8Encoding(false));
            log.AppendLine("Windows Terminal：已设置「" + FAMILY + "」16px");
        }
        catch (Exception ex) { log.AppendLine("Windows Terminal：写入失败：" + ex.Message); }
    }

    void ApplyTerminalFont()
    {
        var log = new StringBuilder();
        string wt = WtSettingsPath();
        if (File.Exists(wt))
        {
            bool busy = IsAppRunning("WindowsTerminal");
            if (busy)
            {
                var r = MessageBox.Show("Windows Terminal 正在运行，修改设置文件前需要先关闭（退出时会覆盖修改）。\n是否自动关闭？",
                    "需要关闭 Windows Terminal", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (r != DialogResult.Yes)
                    log.AppendLine("Windows Terminal：已跳过（窗口未关闭）");
                else
                {
                    CloseApp("WindowsTerminal");
                    busy = IsAppRunning("WindowsTerminal");
                }
            }
            if (!busy)
            {
                string bak = wt + ".zgdh-backup";
                if (!File.Exists(bak)) File.Copy(wt, bak);
                ApplyWt(wt, log);
            }
        }
        else log.AppendLine("Windows Terminal：未找到设置文件（未安装？）");

        ApplyConsoleFont(log);
        MessageBox.Show(log.ToString() + "\n\n打开新的终端窗口生效。", "终端字体",
            MessageBoxButtons.OK, MessageBoxIcon.Information);
    }

    void ApplyConsoleFont(StringBuilder log)
    {
        var snap = new Dictionary<string, object>();
        string[] keys = { "Console", @"Console\cmd.exe", @"Console\powershell.exe" };
        foreach (string key in keys)
        {
            bool existed = true;
            var entry = new Dictionary<string, object>();
            try
            {
                using (var k = Registry.CurrentUser.OpenSubKey(key, false))
                {
                    if (k == null) existed = false;
                    else
                    {
                        object fam = k.GetValue("FontFamily");
                        object sz = k.GetValue("FontSize");
                        entry["FontFace"] = k.GetValue("FontFace") as string;
                        entry["FontFamily"] = fam == null ? null : (double?)(int)fam;
                        entry["FontSize"] = sz == null ? null : (double?)(int)sz;
                    }
                }
            }
            catch { }
            entry["existed"] = existed;
            snap[key] = entry;
            try
            {
                using (var w = Registry.CurrentUser.CreateSubKey(key))
                {
                    w.SetValue("FontFace", FAMILY, RegistryValueKind.String);
                    w.SetValue("FontFamily", (int)0x36, RegistryValueKind.DWord);
                    w.SetValue("FontSize", 0x00100000, RegistryValueKind.DWord);
                }
                log.AppendLine("控制台（" + key + "）：已设置「" + FAMILY + "」");
            }
            catch (Exception ex) { log.AppendLine("控制台（" + key + "）设置失败：" + ex.Message); }
        }
        // 注册 TrueTypeFont 列表（让字体出现在控制台属性对话框的列表里）
        try
        {
            string ttfName = null;
            using (var k = Registry.LocalMachine.OpenSubKey(
                @"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Console\TrueTypeFont", false))
            {
                if (k != null)
                    for (int n = 0; n < 20; n++)
                        if (k.GetValue(n.ToString()) == null) { ttfName = n.ToString(); break; }
            }
            object ttfOld = null;
            using (var k = Registry.LocalMachine.CreateSubKey(
                @"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Console\TrueTypeFont"))
            {
                if (ttfName == null) ttfName = "0";
                ttfOld = k.GetValue(ttfName);
                k.SetValue(ttfName, FAMILY, RegistryValueKind.String);
            }
            var ttfEntry = new Dictionary<string, object>();
            ttfEntry["name"] = ttfName;
            ttfEntry["old"] = ttfOld;
            snap["__TrueTypeFont"] = ttfEntry;
            log.AppendLine("已注册控制台字体列表（TrueTypeFont）");
        }
        catch (Exception ex) { log.AppendLine("TrueTypeFont 注册失败（需要管理员权限？）：" + ex.Message); }

        try
        {
            File.WriteAllText(Path.Combine(InstallDir, CONSOLE_BACKUP),
                JSerialize(snap), new UTF8Encoding(false));
            log.AppendLine("原配置已备份（可一键恢复）");
        }
        catch (Exception ex) { log.AppendLine("备份文件写入失败：" + ex.Message); }
    }

    void RestoreTerminalFont()
    {
        var log = new StringBuilder();
        string wt = WtSettingsPath();
        string bak = wt + ".zgdh-backup";
        if (File.Exists(bak))
        {
            bool busy = IsAppRunning("WindowsTerminal");
            if (busy)
            {
                var r = MessageBox.Show("Windows Terminal 正在运行，需要先关闭才能恢复。\n是否自动关闭？",
                    "需要关闭 Windows Terminal", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (r != DialogResult.Yes)
                    log.AppendLine("Windows Terminal：已跳过（窗口未关闭）");
                else
                {
                    CloseApp("WindowsTerminal");
                    busy = IsAppRunning("WindowsTerminal");
                }
            }
            if (!busy)
            {
                try { File.Copy(bak, wt, true); log.AppendLine("Windows Terminal：已恢复原始设置"); }
                catch (Exception ex) { log.AppendLine("Windows Terminal：恢复失败：" + ex.Message); }
            }
        }
        else log.AppendLine("Windows Terminal：没有找到备份（从未用本工具修改过？）");

        string snapPath = Path.Combine(InstallDir, CONSOLE_BACKUP);
        if (File.Exists(snapPath))
        {
            try
            {
                var root = JParse(File.ReadAllText(snapPath, Encoding.UTF8)) as Dictionary<string, object>;
                if (root != null)
                {
                    foreach (var kv in root)
                    {
                        if (kv.Key == "__TrueTypeFont") continue;
                        var entry = kv.Value as Dictionary<string, object>;
                        if (entry == null) continue;
                        bool existed = entry.ContainsKey("existed") &&
                            entry["existed"] is bool && (bool)entry["existed"];
                        if (!existed)
                        {
                            try
                            {
                                Registry.CurrentUser.DeleteSubKeyTree(kv.Key, false);
                                log.AppendLine("控制台（" + kv.Key + "）：已删除（原无此键）");
                            }
                            catch (Exception ex) { log.AppendLine("控制台（" + kv.Key + "）删除失败：" + ex.Message); }
                            continue;
                        }
                        try
                        {
                            using (var k = Registry.CurrentUser.OpenSubKey(kv.Key, true))
                            {
                                if (k == null) continue;
                                foreach (string vn in new[] { "FontFace", "FontFamily", "FontSize" })
                                {
                                    object v = entry[vn];
                                    if (v == null) { try { k.DeleteValue(vn, false); } catch { } }
                                    else if (vn == "FontFace")
                                        k.SetValue(vn, (string)v, RegistryValueKind.String);
                                    else k.SetValue(vn, (int)(double)v, RegistryValueKind.DWord);
                                }
                            }
                            log.AppendLine("控制台（" + kv.Key + "）：已恢复原值");
                        }
                        catch (Exception ex) { log.AppendLine("控制台（" + kv.Key + "）恢复失败：" + ex.Message); }
                    }
                    object ttfO = root.ContainsKey("__TrueTypeFont") ? root["__TrueTypeFont"] : null;
                    var ttfE = ttfO as Dictionary<string, object>;
                    if (ttfE != null)
                    {
                        string ttfName = ttfE.ContainsKey("name") ? ttfE["name"] as string : null;
                        object ttfOld = ttfE.ContainsKey("old") ? ttfE["old"] : null;
                        if (ttfName != null)
                            try
                            {
                                using (var k = Registry.LocalMachine.OpenSubKey(
                                    @"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Console\TrueTypeFont", true))
                                {
                                    if (k != null)
                                    {
                                        if (ttfOld == null) { try { k.DeleteValue(ttfName, false); } catch { } }
                                        else k.SetValue(ttfName, (string)ttfOld, RegistryValueKind.String);
                                    }
                                }
                                log.AppendLine("已还原控制台字体列表");
                            }
                            catch (Exception ex) { log.AppendLine("TrueTypeFont 还原失败：" + ex.Message); }
                    }
                }
                File.Delete(snapPath);
            }
            catch (Exception ex) { log.AppendLine("控制台备份解析失败：" + ex.Message); }
        }
        else log.AppendLine("控制台：没有找到备份（从未用本工具修改过？）");

        MessageBox.Show(log.ToString() + "\n\n打开新的终端窗口生效。", "恢复终端字体",
            MessageBoxButtons.OK, MessageBoxIcon.Information);
    }

    void ShowSystemFontGuide()
    {
        MessageBox.Show(
            "系统字体（FontSubstitutes）手动设置步骤：\n\n" +
            "1. Win+R 输入 regedit 回车\n" +
            "2. 定位到：\n   HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\FontSubstitutes\n" +
            "3. 新建字符串值（REG_SZ），全部设为：正格点黑 16\n" +
            "   Segoe UI\n   Segoe UI Semibold\n   Segoe UI Light\n   Segoe UI Black\n" +
            "   Microsoft YaHei UI\n" +
            "4. 注销或重启系统生效\n\n" +
            "还原：删除上述新增的键值即可（系统默认本来没有这些条目）。\n\n" +
            "风险提示：\n" +
            "- 需要管理员权限 + 重启，建议先创建系统还原点\n" +
            "- Win11 设置页、开始菜单等 UWP 界面可能不跟随\n" +
            "- 9~12px 小字号下点阵字会被压缩，部分界面文字难读\n" +
            "- 标题栏、菜单等局部不生效属正常现象",
            "系统字体（手动指引）", MessageBoxButtons.OK, MessageBoxIcon.Information);
    }

    // ------------------------------------------------------------ minimal JSON (no dependencies)
    static object JParse(string s) { int p = 0; return JParseValue(s, ref p); }

    static void JSkipWs(string s, ref int p)
    {
        while (p < s.Length && (s[p] == ' ' || s[p] == '\t' || s[p] == '\r' || s[p] == '\n')) p++;
    }

    static object JParseValue(string s, ref int p)
    {
        JSkipWs(s, ref p);
        if (p >= s.Length) throw new Exception("JSON 意外结尾");
        char c = s[p];
        if (c == '{')
        {
            p++;
            var d = new Dictionary<string, object>();
            JSkipWs(s, ref p);
            if (p < s.Length && s[p] == '}') { p++; return d; }
            while (true)
            {
                JSkipWs(s, ref p);
                if (p >= s.Length || s[p] != '"') throw new Exception("JSON 键解析失败");
                string key = JParseString(s, ref p);
                JSkipWs(s, ref p);
                if (p >= s.Length || s[p] != ':') throw new Exception("JSON 缺少冒号");
                p++;
                d[key] = JParseValue(s, ref p);
                JSkipWs(s, ref p);
                if (p >= s.Length) throw new Exception("JSON 意外结尾");
                if (s[p] == ',') { p++; continue; }
                if (s[p] == '}') { p++; break; }
                throw new Exception("JSON 语法错误");
            }
            return d;
        }
        if (c == '[')
        {
            p++;
            var list = new List<object>();
            JSkipWs(s, ref p);
            if (p < s.Length && s[p] == ']') { p++; return list; }
            while (true)
            {
                list.Add(JParseValue(s, ref p));
                JSkipWs(s, ref p);
                if (p >= s.Length) throw new Exception("JSON 意外结尾");
                if (s[p] == ',') { p++; continue; }
                if (s[p] == ']') { p++; break; }
                throw new Exception("JSON 数组语法错误");
            }
            return list;
        }
        if (c == '"') return JParseString(s, ref p);
        if (c == 't') { JExpect(s, ref p, "true"); return true; }
        if (c == 'f') { JExpect(s, ref p, "false"); return false; }
        if (c == 'n') { JExpect(s, ref p, "null"); return null; }
        int start = p;
        while (p < s.Length && "0123456789+-.eE".IndexOf(s[p]) >= 0) p++;
        string num = s.Substring(start, p - start);
        double dbl;
        if (double.TryParse(num, NumberStyles.Float, CultureInfo.InvariantCulture, out dbl)) return dbl;
        return num;
    }

    static string JParseString(string s, ref int p)
    {
        p++;
        var sb = new StringBuilder();
        while (p < s.Length)
        {
            char c = s[p];
            if (c == '"') { p++; return sb.ToString(); }
            if (c == '\\')
            {
                p++;
                if (p >= s.Length) break;
                char e = s[p];
                switch (e)
                {
                    case '"': sb.Append('"'); break;
                    case '\\': sb.Append('\\'); break;
                    case '/': sb.Append('/'); break;
                    case 'b': sb.Append('\b'); break;
                    case 'f': sb.Append('\f'); break;
                    case 'n': sb.Append('\n'); break;
                    case 'r': sb.Append('\r'); break;
                    case 't': sb.Append('\t'); break;
                    case 'u':
                        if (p + 4 < s.Length)
                        {
                            string hex = s.Substring(p + 1, 4);
                            sb.Append((char)int.Parse(hex, NumberStyles.HexNumber,
                                CultureInfo.InvariantCulture));
                            p += 4;
                        }
                        break;
                    default: sb.Append(e); break;
                }
                p++;
            }
            else { sb.Append(c); p++; }
        }
        throw new Exception("JSON 字符串未闭合");
    }

    static void JExpect(string s, ref int p, string word)
    {
        if (p + word.Length <= s.Length && s.Substring(p, word.Length) == word) p += word.Length;
        else throw new Exception("JSON 关键字解析失败");
    }

    static string JSerialize(object v)
    {
        var sb = new StringBuilder();
        JSerializeTo(v, sb);
        return sb.ToString();
    }

    static void JSerializeTo(object v, StringBuilder sb)
    {
        if (v == null) { sb.Append("null"); return; }
        if (v is bool) { sb.Append((bool)v ? "true" : "false"); return; }
        if (v is double)
        {
            double d = (double)v;
            if (d == Math.Floor(d) && Math.Abs(d) < 1e15)
                sb.Append(((long)d).ToString(CultureInfo.InvariantCulture));
            else sb.Append(d.ToString("R", CultureInfo.InvariantCulture));
            return;
        }
        if (v is string) { sb.Append(JSerializeString((string)v)); return; }
        var list = v as List<object>;
        if (list != null)
        {
            sb.Append('[');
            for (int i = 0; i < list.Count; i++)
            {
                if (i > 0) sb.Append(',');
                JSerializeTo(list[i], sb);
            }
            sb.Append(']');
            return;
        }
        var dict = v as Dictionary<string, object>;
        if (dict != null)
        {
            sb.Append('{');
            bool first = true;
            foreach (var kv in dict)
            {
                if (!first) sb.Append(',');
                first = false;
                sb.Append(JSerializeString(kv.Key));
                sb.Append(':');
                JSerializeTo(kv.Value, sb);
            }
            sb.Append('}');
            return;
        }
        sb.Append("null");
    }

    static string JSerializeString(string s)
    {
        var sb = new StringBuilder();
        sb.Append('"');
        foreach (char c in s)
        {
            switch (c)
            {
                case '"': sb.Append("\\\""); break;
                case '\\': sb.Append("\\\\"); break;
                case '\b': sb.Append("\\b"); break;
                case '\f': sb.Append("\\f"); break;
                case '\n': sb.Append("\\n"); break;
                case '\r': sb.Append("\\r"); break;
                case '\t': sb.Append("\\t"); break;
                default:
                    if (c < 0x20) sb.AppendFormat("\\u{0:x4}", (int)c);
                    else sb.Append(c);
                    break;
            }
        }
        sb.Append('"');
        return sb.ToString();
    }

    static Dictionary<string, object> GetOrAdd(Dictionary<string, object> parent, string key)
    {
        object v;
        if (parent.TryGetValue(key, out v))
        {
            var d = v as Dictionary<string, object>;
            if (d != null) return d;
        }
        var nd = new Dictionary<string, object>();
        parent[key] = nd;
        return nd;
    }

    static bool JSetFonts(Dictionary<string, object> root, string family)
    {
        var webkit = GetOrAdd(root, "webkit");
        var wp = GetOrAdd(webkit, "webprefs");
        var fonts = GetOrAdd(wp, "fonts");
        string[] keys = { "standard", "serif", "sansserif", "fixed" };
        bool changed = false;
        foreach (string k in keys)
        {
            object o;
            string cur = fonts.TryGetValue(k, out o) ? o as string : null;
            if (cur != family) { fonts[k] = family; changed = true; }
        }
        return changed;
    }

    static bool SetKV(Dictionary<string, object> d, string k, object v)
    {
        object o;
        if (d.TryGetValue(k, out o) && o != null)
        {
            if (o is string && v is string && (string)o == (string)v) return false;
            if (o is double && v is double && (double)o == (double)v) return false;
        }
        d[k] = v;
        return true;
    }

    static bool JSetTerminalFont(Dictionary<string, object> root, string family, int size)
    {
        bool changed = false;
        var profiles = GetOrAdd(root, "profiles");
        var defaults = GetOrAdd(profiles, "defaults");
        var font = GetOrAdd(defaults, "font");
        changed |= SetKV(font, "face", family);
        changed |= SetKV(font, "size", (double)size);
        changed |= SetKV(defaults, "fontSize", (double)size);   // 旧版字段
        object listO;
        if (profiles.TryGetValue("list", out listO))
        {
            var list = listO as List<object>;
            if (list != null)
                foreach (object item in list)
                {
                    var d = item as Dictionary<string, object>;
                    if (d == null) continue;
                    var f = GetOrAdd(d, "font");
                    changed |= SetKV(f, "face", family);
                    changed |= SetKV(f, "size", (double)size);
                    changed |= SetKV(d, "fontSize", (double)size); // 旧版字段
                    object legacy;
                    if (d.TryGetValue("fontFace", out legacy) && legacy is string)
                    {
                        if ((string)legacy != family) { d["fontFace"] = family; changed = true; }
                    }
                }
        }
        return changed;
    }

    // ------------------------------------------------------------ preview
    void UpdatePreview(Variant v)
    {
        if (v == null) return;
        string src = Path.Combine(FontsSrcDir, VariantFile(v, chkCompat.Checked));
        if (!File.Exists(src)) return;
        var oldPfc = pfc; var old16 = f16; var old32 = f32;
        try
        {
            pfc = new PrivateFontCollection();
            pfc.AddFontFile(src);
            var fam = pfc.Families[0];
            f16 = new Font(fam, 16, GraphicsUnit.Pixel);
            f32 = new Font(fam, 32, GraphicsUnit.Pixel);
            preview16.Font = f16;
            preview32.Font = f32;
            preview16.Text = "正格点黑16 像素字体之美 永国愛あのアAag。、" +
                             "\r\n←→↔⇒∞√∑≤≥∈ ■◆♥●① αβγЖ 0123456789";
            preview32.Text = "正格点黑 永国愛あのアAag。、" +
                             "\r\n←→↔⇒∞√∑≤≥∈ ■◆♥●① αβγЖ 0123456789";
        }
        catch (Exception ex)
        {
            preview16.Text = "预览加载失败：" + ex.Message;
        }
        if (old16 != null) old16.Dispose();
        if (old32 != null) old32.Dispose();
        if (oldPfc != null) oldPfc.Dispose();
    }

    // ------------------------------------------------------------ entry
    [STAThread]
    static void Main(string[] args)
    {
        Application.EnableVisualStyles();
        if (args.Length >= 2 && args[0] == "forceenable")
        {
            // elevated worker: sweep everything, restart cache, install, report
            Variant sel = Variants[0];
            foreach (var v in Variants) if (v.Key == args[1]) sel = v;
            bool hw = args.Length >= 3 && args[2] == "hw";
            var log = new StringBuilder();
            try
            {
                RestartFontCache(log);
                SweepConflicts(true, log);
                InstallFresh(sel, VariantFile(sel, hw), log);
                Broadcast();
                MessageBox.Show("强制启用完成：「" + sel.Label + "」。\n" +
                    "部分程序需要重启后才会显示新样式。\n\n" + log,
                    "完成", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            catch (Exception ex)
            {
                MessageBox.Show("强制启用失败：\n" + ex.Message + "\n\n" + log,
                    "错误", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
            return;
        }
        Application.Run(new FontSwitcher());
    }
}
