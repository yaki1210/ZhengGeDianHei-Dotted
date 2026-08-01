#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextEdit, TextStyle, Vec2, Visuals};

mod integrations;
mod windows_backend;

use integrations::{apply_browser_fonts, apply_terminal_font, browser_is_running, restore_browser, restore_terminal, Browser};
use windows_backend::{FontBackend, InstallState, Variant as BackendVariant, WidthMode};

const FONT_FAMILY: &str = "zhengge-preview";
const FONT_DATA_KEY: &str = "zhengge-selected";
const SYSTEM_FAMILY: &str = "正格点黑 16";
const DEFAULT_PREVIEW: &str = "正格点黑 16\nAa 0123456789\n像素字体之美 · 中英数字 ↔ →";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Dots,
    Squares,
}

impl Shape {
    const ALL: [Shape; 2] = [Shape::Dots, Shape::Squares];

    fn label(self) -> &'static str {
        match self {
            Self::Dots => "圆点",
            Self::Squares => "方块",
        }
    }

    fn values(self) -> &'static [u32] {
        match self {
            Self::Dots => &[70, 80, 90],
            Self::Squares => &[70, 80, 100],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Variant {
    shape: Shape,
    density: u32,
}

impl Variant {
    fn new(shape: Shape, density_index: usize) -> Self {
        let values = shape.values();
        let index = density_index.min(values.len().saturating_sub(1));
        Self { shape, density: values[index] }
    }

    fn file_name(self, halfwidth: bool) -> String {
        if self.shape == Shape::Squares && self.density == 100 {
            if halfwidth { "ZhengGeDianHei16-Original-HW.ttf" } else { "ZhengGeDianHei16-Original.ttf" }.to_owned()
        } else {
            let suffix = if halfwidth { "-HW" } else { "" };
            format!("ZhengGeDianHei16-{}{}{}.ttf", if self.shape == Shape::Dots { "Dots" } else { "Squares" }, self.density, suffix)
        }
    }

    fn description(self) -> String {
        if self.shape == Shape::Squares && self.density == 100 {
            "方块 · 100（原版）".to_owned()
        } else {
            format!("{} · {}", self.shape.label(), self.density)
        }
    }
}

struct FontSwitcherApp {
    shape: Shape,
    density_index: usize,
    preview_text: String,
    preview_expanded: bool,
    halfwidth: bool,
    selected_font: Option<Variant>,
    selected_path: Option<PathBuf>,
    backend: FontBackend,
    status: String,
    status_is_error: bool,
    status_started: Instant,
}

impl Default for FontSwitcherApp {
    fn default() -> Self {
        Self {
            shape: Shape::Dots,
            density_index: 1,
            preview_text: DEFAULT_PREVIEW.to_owned(),
            preview_expanded: true,
            halfwidth: false,
            selected_font: None,
            selected_path: None,
            backend: FontBackend::new(locate_fonts_dir()),
            status: "尚未应用到系统字体".to_owned(),
            status_is_error: false,
            status_started: Instant::now(),
        }
    }
}

impl FontSwitcherApp {
    fn variant(&self) -> Variant {
        Variant::new(self.shape, self.density_index)
    }

    fn backend_variant(&self) -> BackendVariant {
        match self.shape {
            Shape::Dots => BackendVariant::dots(self.variant().density as u8),
            Shape::Squares => BackendVariant::squares(self.variant().density as u8),
        }
    }

    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = text.into();
        self.status_is_error = is_error;
        self.status_started = Instant::now();
    }

    fn load_selected_font(&mut self, ctx: &egui::Context) {
        let variant = self.variant();
        if self.selected_font == Some(variant) {
            return;
        }

        self.selected_font = Some(variant);
        let Some(path) = locate_font(&variant.file_name(self.halfwidth)) else {
            self.selected_path = None;
            self.set_status(format!("找不到字体文件：{}（已使用系统回退字体）", variant.file_name(self.halfwidth)), true);
            return;
        };

        match fs::read(&path) {
            Ok(bytes) => {
                let mut definitions = FontDefinitions::default();
                definitions.font_data.insert(
                    FONT_DATA_KEY.to_owned(),
                    FontData::from_owned(bytes),
                );
                let selected_name = FONT_DATA_KEY.to_owned();
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    let fonts = definitions.families.entry(family).or_default();
                    fonts.retain(|name| name != FONT_DATA_KEY);
                    fonts.insert(0, selected_name.clone());
                }
                definitions.families.insert(
                    FontFamily::Name(FONT_FAMILY.into()),
                    vec![selected_name, "Proportional".to_owned()],
                );
                ctx.set_fonts(definitions);
                apply_ui_font(ctx);
                self.selected_path = Some(path.clone());
                self.set_status(format!("预览已切换：{}", variant.description()), false);
            }
            Err(error) => {
                self.selected_path = None;
                self.set_status(format!("无法读取字体：{} ({error})", path.display()), true);
            }
        }
    }

    fn render_variant_controls(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("字体样式").strong());
        ui.horizontal(|ui| {
            for shape in Shape::ALL {
                let selected = self.shape == shape;
                if ui.selectable_label(selected, shape.label()).clicked() && !selected {
                    self.shape = shape;
                    self.density_index = self.density_index.min(shape.values().len() - 1);
                    self.selected_font = None;
                }
            }
        });

        let values = self.shape.values();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("密度");
            let mut index = self.density_index as u32;
            let response = ui.add(
                egui::Slider::new(&mut index, 0..=(values.len() as u32 - 1))
                    .step_by(1.0)
                    .show_value(false),
            );
            if response.changed() {
                self.density_index = index as usize;
                self.selected_font = None;
            }
            for (i, value) in values.iter().enumerate() {
                let label = if self.shape == Shape::Squares && *value == 100 {
                    "100 原版"
                } else {
                    // Keep labels compact in the narrow layout.
                    match value {
                        70 => "70",
                        80 => "80",
                        90 => "90",
                        _ => "100",
                    }
                };
                let text = if i == self.density_index {
                    RichText::new(label).strong().color(Color32::WHITE)
                } else {
                    RichText::new(label).weak()
                };
                ui.label(text);
            }
        });
        ui.label(RichText::new(self.variant().description()).small().weak());
        let mut halfwidth = self.halfwidth;
        if ui.checkbox(&mut halfwidth, "终端兼容：半宽符号").changed() {
            self.halfwidth = halfwidth;
            self.selected_font = None;
        }
    }

    fn render_actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("应用切换").clicked() {
                let width = if self.halfwidth { WidthMode::Half } else { WidthMode::Full };
                let variant = self.backend_variant();
                match self.backend.install(variant, width) {
                    Ok(report) => self.set_status(format!("已应用 {}：{}", self.variant().description(), report.active.path.display()), false),
                    Err(error) => self.set_status(format!("应用失败：{error}"), true),
                }
            }
            if ui.button("停用").clicked() {
                match self.backend.disable() {
                    Ok(report) => self.set_status(format!("已停用字体，移除 {} 个文件", report.removed_files.len()), false),
                    Err(error) => self.set_status(format!("停用失败：{error}"), true),
                }
            }
            if ui.button("检查状态").clicked() {
                match self.backend.check() {
                    Ok(status) => match status.state {
                        InstallState::Installed(active) => self.set_status(format!("已安装：{}", active.variant.key()), false),
                        InstallState::NotInstalled => self.set_status("尚未安装正格点黑 16", false),
                        InstallState::Mismatch { reason, .. } => self.set_status(format!("安装状态异常：{reason}"), true),
                    },
                    Err(error) => self.set_status(format!("检查失败：{error}"), true),
                }
            }
        });
    }

    fn render_integrations(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("浏览器与终端设置").default_open(false).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("浏览器");
                if ui.button("应用 Chrome").clicked() { self.apply_browser(Browser::Chrome); }
                if ui.button("恢复 Chrome").clicked() { self.restore_browser(Browser::Chrome); }
                if ui.button("应用 Edge").clicked() { self.apply_browser(Browser::Edge); }
                if ui.button("恢复 Edge").clicked() { self.restore_browser(Browser::Edge); }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Windows Terminal");
                if ui.button("应用终端字体").clicked() {
                    match apply_terminal_font(SYSTEM_FAMILY, 16) {
                        Ok(path) => self.set_status(format!("已更新终端设置：{}", path.display()), false),
                        Err(error) => self.set_status(format!("终端设置失败：{error}"), true),
                    }
                }
                if ui.button("恢复终端设置").clicked() {
                    match restore_terminal() {
                        Ok(path) => self.set_status(format!("已恢复终端设置：{}", path.display()), false),
                        Err(error) => self.set_status(format!("恢复终端失败：{error}"), true),
                    }
                }
            });
            ui.label(RichText::new("外部程序运行时请先关闭；配置修改前会创建同目录备份。").small().weak());
        });
    }

    fn apply_browser(&mut self, browser: Browser) {
        if browser_is_running(browser) {
            self.set_status(format!("{} 正在运行，请关闭后再应用", browser.display_name()), true);
            return;
        }
        match apply_browser_fonts(browser, SYSTEM_FAMILY) {
            Ok(report) => self.set_status(format!("已更新 {} 个 {} 配置文件", report.changed_files.len(), browser.display_name()), false),
            Err(error) => self.set_status(format!("浏览器设置失败：{error}"), true),
        }
    }

    fn restore_browser(&mut self, browser: Browser) {
        match restore_browser(browser) {
            Ok(report) => self.set_status(format!("已恢复 {} 个 {} 配置文件", report.changed_files.len(), browser.display_name()), false),
            Err(error) => self.set_status(format!("浏览器恢复失败：{error}"), true),
        }
    }
}

impl eframe::App for FontSwitcherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.load_selected_font(ctx);
        if self.status_started.elapsed() < Duration::from_secs(2) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("正格点黑 16");
                ui.label(RichText::new("字体切换器").weak());
            });
            ui.label(RichText::new("简约、清晰、可编辑的像素字体预览").weak());
            ui.add_space(14.0);

            egui::Frame::group(ui.style())
                .fill(Color32::from_gray(24))
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(58)))
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| {
                    self.render_variant_controls(ui);
                });

            ui.add_space(12.0);
            egui::CollapsingHeader::new("可编辑展示区")
                .default_open(self.preview_expanded)
                .show(ui, |ui| {
                    self.preview_expanded = true;
                    // This is the only preview/display surface. Text is transient and editable.
                    let preview_font = FontId::new(18.0, FontFamily::Name(FONT_FAMILY.into()));
                    ui.add_sized(
                        Vec2::new(ui.available_width(), 220.0),
                        TextEdit::multiline(&mut self.preview_text)
                            .font(preview_font)
                            .desired_rows(9)
                            .hint_text("输入要预览的文字…"),
                    );
            });

            ui.add_space(12.0);
            self.render_integrations(ui);
            ui.add_space(8.0);
            self.render_actions(ui);
            ui.add_space(8.0);
            let status_color = if self.status_is_error { Color32::from_rgb(235, 130, 130) } else { Color32::from_gray(170) };
            ui.label(RichText::new(&self.status).color(status_color));
            if let Some(path) = &self.selected_path {
                ui.label(RichText::new(format!("资源：{}", path.display())).small().weak());
            }
            ui.add_space(8.0);
            ui.label(RichText::new("预览文本仅保存在内存中；应用切换不会自动执行。").small().weak());
        });
    }
}

fn apply_ui_font(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let family = FontFamily::Name(FONT_FAMILY.into());
    for text_style in [
        TextStyle::Body,
        TextStyle::Button,
        TextStyle::Heading,
        TextStyle::Monospace,
        TextStyle::Small,
    ] {
        let size = style
            .text_styles
            .get(&text_style)
            .map(|font| font.size)
            .unwrap_or(16.0);
        style.text_styles.insert(text_style, FontId::new(size, family.clone()));
    }
    ctx.set_style(style);
}

fn locate_fonts_dir() -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join("dist").join("fonts");
        if candidate.is_dir() { return candidate; }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.join("fonts")))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("fonts"))
}

fn locate_font(file_name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("fonts").join(file_name));
            candidates.push(parent.join("dist").join("fonts").join(file_name));
            if let Some(grandparent) = parent.parent() {
                candidates.push(grandparent.join("dist").join("fonts").join(file_name));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("dist").join("fonts").join(file_name));
        candidates.push(cwd.join("preview").join("fonts").join(file_name));
    }
    candidates.into_iter().find(|path| Path::new(path).is_file())
}

/// Install a panic hook that shows a MessageBox on Windows so that
/// panics are not completely silent in a `windows_subsystem = "windows"` app.
fn install_panic_hook() {
    #[cfg(windows)]
    {
        std::panic::set_hook(Box::new(|info| {
            let msg = format!("程序发生错误：\n\n{info}");
            use std::os::windows::ffi::OsStrExt;
            let text: Vec<u16> = std::ffi::OsStr::new(&msg)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let title: Vec<u16> = std::ffi::OsStr::new("正格点黑 16 字体切换器")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            unsafe {
                MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
            }
        }));
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, utype: u32) -> i32;
}

fn main() -> eframe::Result {
    install_panic_hook();

    let options = eframe::NativeOptions {
        // Use the Glow (OpenGL) renderer instead of the default wgpu backend.
        // wgpu can fail to initialize on systems with unusual GPU configurations
        // (e.g. virtual display adapters), causing a silent panic→abort crash.
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("正格点黑 16 字体切换器")
            .with_inner_size([640.0, 680.0])
            .with_min_inner_size([440.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "正格点黑 16 字体切换器",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(dark_visuals());
            apply_ui_font(&cc.egui_ctx);
            Ok(Box::new(FontSwitcherApp::default()))
        }),
    )
}

fn dark_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(Color32::from_gray(225));
    visuals.panel_fill = Color32::from_gray(15);
    visuals.window_fill = Color32::from_gray(15);
    visuals.extreme_bg_color = Color32::from_gray(8);
    visuals.faint_bg_color = Color32::from_gray(24);
    visuals.widgets.noninteractive.bg_fill = Color32::from_gray(28);
    visuals.widgets.inactive.bg_fill = Color32::from_gray(34);
    visuals.widgets.hovered.bg_fill = Color32::from_gray(50);
    visuals.widgets.active.bg_fill = Color32::from_gray(66);
    visuals
}
