//! Visual theme and fonts.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, Style, Visuals};
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(255, 106, 146);
pub const ACCENT2: Color32 = Color32::from_rgb(92, 196, 255);
pub const BG0: Color32 = Color32::from_rgb(17, 18, 23);
pub const BG1: Color32 = Color32::from_rgb(24, 26, 32);
pub const BG2: Color32 = Color32::from_rgb(33, 35, 43);
pub const BG3: Color32 = Color32::from_rgb(44, 47, 57);
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 145, 160);
pub const KEY: Color32 = Color32::from_rgb(255, 196, 84);
pub const AXIS_X: Color32 = Color32::from_rgb(235, 80, 90);
pub const AXIS_Y: Color32 = Color32::from_rgb(120, 210, 90);
pub const AXIS_Z: Color32 = Color32::from_rgb(80, 140, 245);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = BG1;
    v.window_fill = BG1;
    v.extreme_bg_color = BG0;
    v.faint_bg_color = BG2;
    v.code_bg_color = BG0;
    v.window_stroke = Stroke::new(1.0, BG3);
    v.selection.bg_fill = ACCENT.linear_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT2;
    v.widgets.noninteractive.bg_fill = BG1;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BG3);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 204, 214));
    v.widgets.inactive.bg_fill = BG3;
    v.widgets.inactive.weak_bg_fill = BG2;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(214, 218, 228));
    v.widgets.hovered.bg_fill = Color32::from_rgb(58, 62, 75);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 56, 68);
    v.widgets.active.bg_fill = ACCENT.linear_multiply(0.7);
    v.widgets.active.weak_bg_fill = ACCENT.linear_multiply(0.5);
    let r = CornerRadius::same(5);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(8);
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s: &mut Style| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.slider_width = 150.0;
        s.spacing.interact_size.y = 20.0;
    });
    load_fonts(ctx);
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    // Use system fonts for Japanese, then Chinese, bone/morph/file names (models come from
    // both). Fonts already on the PC are used, never shipped.
    let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let wf = |n: &str| format!("{win}\\Fonts\\{n}");
    let japanese: Vec<String> = vec![
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into(),
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc".into(),
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc".into(),
        "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf".into(),
        "/usr/share/fonts/noto-cjk/NotoSansCJKjp-Regular.otf".into(),
        "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf".into(),
        wf("YuGothM.ttc"),
        wf("meiryo.ttc"),
        wf("msgothic.ttc"),
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc".into(),
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf".into(),
    ];
    let chinese: Vec<String> = vec![
        wf("msyh.ttc"),
        wf("simsun.ttc"),
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc".into(),
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc".into(),
        "/System/Library/Fonts/Hiragino Sans GB.ttc".into(),
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf".into(),
    ];
    let mut loaded: Vec<String> = Vec::new();
    for (key, list) in [("cjk", &japanese), ("cjk-sc", &chinese)] {
        for path in list.iter() {
            if loaded.contains(path) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(path) {
                fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
                for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                    fonts.families.entry(fam).or_default().push(key.into());
                }
                loaded.push(path.clone());
                break;
            }
        }
        // Noto CJK already covers Chinese; no need for a second font.
        if loaded.iter().any(|p| p.contains("NotoSansCJK-Regular")) {
            break;
        }
    }
    ctx.set_fonts(fonts);
}

pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title.to_uppercase()).size(10.5).color(TEXT_DIM).strong());
    ui.add_space(1.0);
}

/// Click area for a card that holds its own buttons. It is registered *before* the card's
/// contents (using the card's size from the previous frame), so the buttons inside stay on top
/// and keep working. Call `remember_card` with the card's rect after drawing it.
pub fn card_behind(ui: &mut egui::Ui, id: egui::Id) -> Option<egui::Response> {
    let rect: Option<egui::Rect> = ui.ctx().data(|d| d.get_temp(id));
    rect.map(|r| ui.interact(r, id, egui::Sense::click()))
}

pub fn remember_card(ui: &egui::Ui, id: egui::Id, rect: egui::Rect) {
    ui.ctx().data_mut(|d| d.insert_temp(id, rect));
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(BG2)
        .stroke(Stroke::new(1.0, BG3))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(egui::Margin::same(8))
        .show(ui, add)
        .inner
}
