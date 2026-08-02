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
const DEFAULT_PREVIEW: &str = "正格点黑 16  Aa 0123456789\n像素字体之美 · 中英数字 ↔ →";

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
    fonts_ready: bool,
    show_browser_menu: bool,
    show_terminal_menu: bool,
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
            fonts_ready: false,
            show_browser_menu: false,
            show_terminal_menu: false,
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
            bind_fallback_font(ctx);
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
                let proportional_fonts = definitions
                    .families
                    .get(&FontFamily::Proportional)
                    .cloned()
                    .unwrap_or_default();
                definitions.families.insert(
                    FontFamily::Name(FONT_FAMILY.into()),
                    std::iter::once(selected_name)
                        .chain(proportional_fonts.into_iter())
                        .collect(),
                );
                ctx.set_fonts(definitions);
                apply_ui_font(ctx);
                self.selected_path = Some(path.clone());
            }
            Err(error) => {
                self.selected_path = None;
                bind_fallback_font(ctx);
                self.set_status(format!("无法读取字体：{} ({error})", path.display()), true);
            }
        }
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
        if !self.fonts_ready {
            self.load_selected_font(ctx);
            self.fonts_ready = true;
            ctx.request_repaint();
            return;
        }
        self.load_selected_font(ctx);
        if self.status_started.elapsed() < Duration::from_secs(2) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        let panel_frame = egui::Frame::central_panel(&ctx.style())
            .fill(Color32::from_rgb(18, 18, 18))
            .inner_margin(egui::Margin::symmetric(20.0, 16.0));

        egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            // Hide the side scrollbar (keep wheel/drag scrolling) for a cleaner look
            ui.style_mut().spacing.scroll.bar_width = 0.0;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    
                    // App Title Header
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("正格点黑 16").size(28.0).strong().color(Color32::WHITE));
                        ui.add_space(4.0);
                        ui.label(RichText::new("字体切换器").size(20.0).color(Color32::from_gray(160)));
                    });
                    ui.add_space(16.0);

                    // Full-width Segmented Control
                    ui.columns(2, |cols| {
                        let is_dots = self.shape == Shape::Dots;
                        let (bg_dots, fg_dots, stroke_dots) = if is_dots {
                            (Color32::WHITE, Color32::from_rgb(17, 17, 17), Stroke::NONE)
                        } else {
                            (Color32::from_rgb(26, 26, 26), Color32::from_rgb(210, 210, 210), Stroke::new(1.0, Color32::from_rgb(50, 50, 50)))
                        };
                        let btn_dots = egui::Button::new(
                            RichText::new("●  圆点")
                                .size(22.0)
                                .color(fg_dots)
                        )
                        .fill(bg_dots)
                        .stroke(stroke_dots)
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(Vec2::new(cols[0].available_width(), 46.0));

                        if cols[0].add(btn_dots).clicked() && !is_dots {
                            self.shape = Shape::Dots;
                            self.density_index = self.density_index.min(Shape::Dots.values().len() - 1);
                            self.selected_font = None;
                        }

                        let is_squares = self.shape == Shape::Squares;
                        let (bg_sq, fg_sq, stroke_sq) = if is_squares {
                            (Color32::WHITE, Color32::from_rgb(17, 17, 17), Stroke::NONE)
                        } else {
                            (Color32::from_rgb(26, 26, 26), Color32::from_rgb(210, 210, 210), Stroke::new(1.0, Color32::from_rgb(50, 50, 50)))
                        };
                        let btn_sq = egui::Button::new(
                            RichText::new("■  方块")
                                .size(22.0)
                                .color(fg_sq)
                        )
                        .fill(bg_sq)
                        .stroke(stroke_sq)
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(Vec2::new(cols[1].available_width(), 46.0));

                        if cols[1].add(btn_sq).clicked() && !is_squares {
                            self.shape = Shape::Squares;
                            self.density_index = self.density_index.min(Shape::Squares.values().len() - 1);
                            self.selected_font = None;
                        }
                    });

                    ui.add_space(20.0);

                    // Full-width white slider
                    let values = self.shape.values();
                    let slider_w = ui.available_width();
                    let mut index = self.density_index as u32;
                    let values_len = values.len() as u32 - 1;

                    let slider_changed = ui.scope(|ui| {
                        ui.set_min_width(slider_w);
                        ui.spacing_mut().slider_width = slider_w;
                        ui.visuals_mut().widgets.inactive.bg_fill = Color32::WHITE;
                        ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(80, 80, 80));

                        let slider = egui::Slider::new(&mut index, 0..=values_len)
                            .step_by(1.0)
                            .show_value(false);

                        ui.add(slider).changed()
                    }).inner;

                    if slider_changed {
                        self.density_index = index as usize;
                        self.selected_font = None;
                    }
                    ui.add_space(4.0);

                    // Density Ticks Alignment (Left, Center, Right)
                    ui.horizontal(|ui| {
                        let total_w = ui.available_width();
                        
                        let left_val = format!("{}", values[0]);
                        let is_left = self.density_index == 0;
                        ui.label(
                            RichText::new(left_val)
                                .size(18.0)
                                .color(if is_left { Color32::WHITE } else { Color32::from_gray(130) })
                        );

                        ui.add_space(total_w / 2.0 - 54.0);
                        let mid_val = format!("{}", values[1]);
                        let is_mid = self.density_index == 1;
                        ui.label(
                            RichText::new(mid_val)
                                .size(18.0)
                                .color(if is_mid { Color32::WHITE } else { Color32::from_gray(130) })
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let right_val = if self.shape == Shape::Squares && values[2] == 100 {
                                "100".to_string()
                            } else {
                                format!("{}", values[2])
                            };
                            let is_right = self.density_index == 2;
                            ui.label(
                                RichText::new(right_val)
                                    .size(18.0)
                                    .color(if is_right { Color32::WHITE } else { Color32::from_gray(130) })
                            );
                        });
                    });

                    ui.add_space(18.0);

                    // Requirement 2: Display Text Editor Box - Clickable Header to Expand/Collapse
                    let sign_text = if self.preview_expanded { "-" } else { "+" };
                    let header_frame = egui::Frame::none()
                        .fill(Color32::from_rgb(24, 24, 24))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(44, 44, 44)))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(14.0, 10.0));

                    let header_resp = header_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("展示文本编辑区").size(19.0).color(Color32::from_gray(200)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(sign_text).size(22.0).strong().color(Color32::from_gray(160)));
                            });
                        });
                    });

                    if header_resp.response.interact(egui::Sense::click()).clicked() {
                        self.preview_expanded = !self.preview_expanded;
                    }

                    if self.preview_expanded {
                        ui.add_space(6.0);
                        let preview_font = FontId::new(30.0, FontFamily::Name(FONT_FAMILY.into()));
                        ui.add_sized(
                            Vec2::new(ui.available_width(), 130.0),
                            TextEdit::multiline(&mut self.preview_text)
                                .font(preview_font)
                                .desired_rows(3)
                                .hint_text("输入要预览的文字…"),
                        );
                    }

                    ui.add_space(20.0);

                    // Section 4: 浏览器 Integration Card
                    egui::Frame::none()
                        .fill(Color32::from_rgb(24, 24, 24))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(44, 44, 44)))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("浏览器").size(20.0).strong().color(Color32::WHITE));
                                    ui.add_space(2.0);
                                    ui.label(RichText::new("Chrome / Edge · 需要单独确认").size(17.0).color(Color32::from_gray(140)));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Requirement 4: White background, dark text
                                    let btn = egui::Button::new(
                                        RichText::new("设置 / 恢复")
                                            .size(18.0)
                                            .strong()
                                            .color(Color32::from_rgb(17, 17, 17))
                                    )
                                    .fill(Color32::WHITE)
                                    .rounding(egui::Rounding::same(6.0))
                                    .min_size(Vec2::new(100.0, 36.0));
                                    if ui.add(btn).clicked() {
                                        self.show_browser_menu = !self.show_browser_menu;
                                    }
                                });
                            });
                            if self.show_browser_menu {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(6.0);
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("应用 Chrome").clicked() { self.apply_browser(Browser::Chrome); }
                                    if ui.button("恢复 Chrome").clicked() { self.restore_browser(Browser::Chrome); }
                                    if ui.button("应用 Edge").clicked() { self.apply_browser(Browser::Edge); }
                                    if ui.button("恢复 Edge").clicked() { self.restore_browser(Browser::Edge); }
                                });
                            }
                        });

                    ui.add_space(12.0);

                    // Section 5: 终端与控制台 Integration Card
                    egui::Frame::none()
                        .fill(Color32::from_rgb(24, 24, 24))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(44, 44, 44)))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("终端与控制台").size(20.0).strong().color(Color32::WHITE));
                                    ui.add_space(2.0);
                                    ui.label(RichText::new("Windows Terminal · 需要单独确认").size(17.0).color(Color32::from_gray(140)));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Requirement 4: White background, dark text
                                    let btn = egui::Button::new(
                                        RichText::new("设置 / 恢复")
                                            .size(18.0)
                                            .strong()
                                            .color(Color32::from_rgb(17, 17, 17))
                                    )
                                    .fill(Color32::WHITE)
                                    .rounding(egui::Rounding::same(6.0))
                                    .min_size(Vec2::new(100.0, 36.0));
                                    if ui.add(btn).clicked() {
                                        self.show_terminal_menu = !self.show_terminal_menu;
                                    }
                                });
                            });

                            if self.show_terminal_menu {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(6.0);
                                ui.horizontal_wrapped(|ui| {
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

                                // Half-width terminal compatibility mode (inside expanded panel)
                                ui.add_space(8.0);
                                let mut halfwidth = self.halfwidth;
                                if ui.checkbox(
                                    &mut halfwidth,
                                    RichText::new("窄终端兼容模式（半宽符号：防止与下一字符重叠）")
                                        .size(18.0)
                                        .color(Color32::from_gray(210))
                                ).changed() {
                                    self.halfwidth = halfwidth;
                                    self.selected_font = None;
                                }
                            }
                        });

                    ui.add_space(16.0);

                    // Section 6: Bottom Action Bar
                    ui.horizontal(|ui| {
                        let status_desc = format!("已预览：{}", self.variant().description());
                        let left_width = (ui.available_width() - 310.0).max(180.0);
                        
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width, 50.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(status_desc)
                                        .size(17.0)
                                        .color(Color32::from_gray(160))
                                );
                            }
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Button 3: 应用切换
                            let btn_apply = egui::Button::new(
                                RichText::new("应用\n切换")
                                    .size(19.0)
                                    .strong()
                                    .color(Color32::from_rgb(17, 17, 17))
                            )
                            .fill(Color32::WHITE)
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(Vec2::new(92.0, 50.0));
                            
                            if ui.add(btn_apply).clicked() {
                                let width = if self.halfwidth { WidthMode::Half } else { WidthMode::Full };
                                let variant = self.backend_variant();
                                match self.backend.install(variant, width) {
                                    Ok(report) => self.set_status(format!("已应用 {}：{}", self.variant().description(), report.active.path.display()), false),
                                    Err(error) => self.set_status(format!("应用失败：{error}"), true),
                                }
                            }

                            ui.add_space(6.0);

                            // Button 2: 停用字体
                            let btn_disable = egui::Button::new(
                                RichText::new("停用\n字体")
                                    .size(19.0)
                                    .color(Color32::from_gray(230))
                            )
                            .fill(Color32::from_rgb(34, 34, 34))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(60, 60, 60)))
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(Vec2::new(86.0, 50.0));

                            if ui.add(btn_disable).clicked() {
                                match self.backend.disable() {
                                    Ok(report) => self.set_status(format!("已停用字体，移除 {} 个文件", report.removed_files.len()), false),
                                    Err(error) => self.set_status(format!("停用失败：{error}"), true),
                                }
                            }

                            ui.add_space(6.0);

                            // Button 1: 检查状态
                            let btn_check = egui::Button::new(
                                RichText::new("检查\n状态")
                                    .size(19.0)
                                    .color(Color32::from_gray(230))
                            )
                            .fill(Color32::from_rgb(34, 34, 34))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(60, 60, 60)))
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(Vec2::new(86.0, 50.0));

                            if ui.add(btn_check).clicked() {
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
                    });

                    ui.add_space(10.0);
                    let status_color = if self.status_is_error { Color32::from_rgb(235, 130, 130) } else { Color32::from_gray(160) };
                    ui.label(RichText::new(&self.status).size(17.0).color(status_color));
                    ui.add_space(16.0);
                });
        });
    }
}

fn bind_fallback_font(ctx: &egui::Context) {
    let mut definitions = FontDefinitions::default();
    let proportional_fonts = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    definitions
        .families
        .insert(FontFamily::Name(FONT_FAMILY.into()), proportional_fonts);
    ctx.set_fonts(definitions);
    apply_ui_font(ctx);
}

fn apply_ui_font(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let family = FontFamily::Name(FONT_FAMILY.into());

    style.text_styles.insert(TextStyle::Heading, FontId::new(28.0, family.clone()));
    style.text_styles.insert(TextStyle::Body, FontId::new(22.0, family.clone()));
    style.text_styles.insert(TextStyle::Button, FontId::new(22.0, family.clone()));
    style.text_styles.insert(TextStyle::Monospace, FontId::new(22.0, family.clone()));
    style.text_styles.insert(TextStyle::Small, FontId::new(18.0, family.clone()));

    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);

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

// Requirement 5: Embed icon.png into eframe Viewport
fn load_app_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../icon.png");
    let image = image::load_from_memory(icon_bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

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

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("正格点黑 16 字体切换器")
        .with_inner_size([720.0, 850.0])
        .with_min_inner_size([580.0, 700.0]);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "正格点黑 16 字体切换器",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(dark_visuals());
            bind_fallback_font(&cc.egui_ctx);
            Ok(Box::new(FontSwitcherApp::default()))
        }),
    )
}

fn dark_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(Color32::from_gray(230));
    visuals.panel_fill = Color32::from_rgb(18, 18, 18);
    visuals.window_fill = Color32::from_rgb(18, 18, 18);
    visuals.extreme_bg_color = Color32::from_rgb(14, 14, 14);
    visuals.faint_bg_color = Color32::from_rgb(26, 26, 26);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(28, 28, 28);
    
    // White theme for sliders & active widgets
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(80, 80, 80);
    visuals.widgets.hovered.bg_fill = Color32::WHITE;
    visuals.widgets.active.bg_fill = Color32::WHITE;
    visuals.selection.bg_fill = Color32::WHITE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_gray(220));
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_gray(200));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_gray(255));
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_gray(255));
    visuals.window_rounding = egui::Rounding::same(12.0);
    visuals
}
