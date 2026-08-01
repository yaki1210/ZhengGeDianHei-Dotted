**Refactor Plan: ZhengGeDianHei 16 Font Switcher EXE**

**Project Context**
- Current: tools/FontSwitcher.cs (WinForms C#), produces ~36KB EXE (already tiny but needs modernization).
- Variants: original (100% solid squares), Squares70/Squares80, Dots70/Dots80/Dots90.
- Features: font install (registry + PrivateFontCollection), preview at 16/32px, apply to Chrome/Edge, Windows Terminal, console, status check, force enable.
- Preview text hardcoded; browser/terminal JSON patching in C#.

**Refactor Goals**
- Lightweight & modern: native EXE <10MB total.
- Modern UI: clean two-row layout, dark theme preferred, responsive.
- Font switching UI: two rows of variant buttons (Dots 70/80/90, Squares 70/80/100-original).
- Pull bar (Slider): for font size / density.
- All software text applies selected font (dynamic UI font loading).
- Display area: default hidden (CollapsingHeader), expandable text editor with sample text (简繁中英数字), editable but no persistence/save required (transient preview only).
- Keep core: install/apply/disable/check/force, browser/terminal support.

**Technology Decision**
- **Use Rust + eframe (egui + winit)** for modern, tiny native EXE (target ~5-8MB stripped).
- Leverage provided GitHub build repo: git@github.com:yaki1210/ZhengGeDianHei-Dotted.git (clone locally for font variants, Rust scaffolding, or build scripts).
- Font handling: embed/load TTFs from dist/fonts/ using rusttype/fontdue; preview rendering via egui.
- Alternative (if Rust complex): optimize C# to WinUI/Modern theme + smaller binary, but Rust preferred for size/modernity.
- Build: `cargo build --release --target x86_64-pc-windows-msvc`, strip EXE if needed, output to dist/.

**UI/Feature Design**
- Header: title "正格点黑 16 字体切换器" with icon.
- Two rows:
  - 圆点 (Dots): buttons/radio for 70,80,90
  - 方块 (Squares): 70,80,100 (original highlighted)
- Slider (拉条) for font size (16px default) or outline density.
- Text area: default CollapsingHeader (hidden), inside editable TextBox with sample content: "正格点黑16 像素字体之美 永国愛あのアAag。、 中英数字 0123456789 ←→∑≤≥"
  - Font applied automatically to all UI + text area.
- Bottom: status label, Apply/Force/Disable/Check buttons.
- Browser/Terminal sections: keep functional (use Python helpers or port the JSON edit logic to Rust if possible).
- No save: edits are preview-only, changes not persisted to disk.

**Implementation Steps**
1. **Setup**
   - Clone GitHub repo if needed: `git clone git@github.com:yaki1210/ZhengGeDianHei-Dotted.git` (use run_terminal_command).
   - Extract font variants, update Python build tools if needed (use `e:\miniconda3\envs\ai\python.exe` as the Python environment for any Python scripts, variant building, or JSON patching helpers).
   - Create new Cargo project in workspace root or subdir.

2. **Core Rust Code**
   - Define enum Variants (original, sq70, dot70 etc.) with file paths.
   - Load fonts on app start, store as Vec<FontData>.
   - UI state: current_variant, font_size, text_content (default sample), is_text_expanded.
   - Font application: egui's FontFamily or custom glyph mapping; all labels use selected font.
   - Preview: render TextEdit with selected font/size.

3. **Modern UI Implementation**
   - Use egui: Grid or two Rows for variant buttons (RadioButton or Button with selection).
   - Slider: egui::Slider for size/density.
   - CollapsingHeader for text area.
   - Dark theme via egui::Theme.
   - Responsive, fixed size ~600x700 for compatibility.

4. **Font & Preview Logic**
   - On variant change: load TTF, set egui font.
   - Text area: always show with current font (egui TextEdit supports font).
   - All labels: set font via egui's custom fonts or style.

5. **Application Logic**
   - Install/Apply: simulate with AddFontResourceW (use winapi crate) + registry update (or focus on preview since EXE is portable).
   - Browser/Terminal: keep C# Python scripts or add Rust JSON parsing (serde).
   - Status: simple label.

6. **Build & Packaging**
   - Cargo.toml with dependencies: eframe, egui, rusttype, serde_json, winapi.
   - Build EXE to dist/正格点黑16字体切换器.exe
   - Ensure total size <10MB (Rust binaries are tiny).
   - Include all fonts in dist/fonts/.

7. **Testing & Verification**
   - Test font switching, preview update, UI text font application.
   - Verify <10MB EXE.
   - No data persistence in text editor.
   - Run on Windows 10/11, test variants (dots/squares), original.

**Risks & Notes**
- Rust learning curve: use eframe template as base.
- Font application in apps: keep C# helpers for registry/browser/terminal.
- Since plan mode, no other edits allowed until plan approved.
- After this, user may run commands to implement the Rust project.

**Next**
- Present this plan; user may ask questions or approve for execution.
