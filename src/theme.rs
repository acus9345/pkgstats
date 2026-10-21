//! Dark dashboard look: near-black background, softly bordered cards, Segoe UI
//! for text and a bold monospace face for big numbers.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, RichText, Stroke,
    TextStyle, Ui, Visuals,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub const BG: Color32 = Color32::from_rgb(10, 10, 11);
pub const CARD: Color32 = Color32::from_rgb(20, 20, 22);
pub const CARD_HOVER: Color32 = Color32::from_rgb(30, 30, 33);
pub const BORDER: Color32 = Color32::from_rgb(38, 38, 42);
pub const TEXT: Color32 = Color32::from_rgb(236, 236, 238);
pub const MUTED: Color32 = Color32::from_rgb(140, 140, 148);
pub const GRID: Color32 = Color32::from_rgb(44, 44, 48);
pub const BLUE: Color32 = Color32::from_rgb(77, 141, 255);

/// Accent colour choices offered in Settings → App style.
pub const PRESETS: [(&str, Color32); 8] = [
    ("Blue", BLUE),
    ("Soft yellow", Color32::from_rgb(245, 214, 122)),
    ("Peach", Color32::from_rgb(255, 166, 112)),
    ("Rose", Color32::from_rgb(244, 124, 160)),
    ("Lavender", Color32::from_rgb(167, 139, 250)),
    ("Sky", Color32::from_rgb(86, 204, 232)),
    ("Mint", Color32::from_rgb(96, 212, 166)),
    ("Silver", Color32::from_rgb(196, 198, 206)),
];

/// Current accent as 0xRRGGBB; read everywhere through [`accent`].
static ACCENT: AtomicU32 = AtomicU32::new(0x4D8DFF);

/// Highlight colour for links, chart lines, bars and selections.
pub fn accent() -> Color32 {
    let v = ACCENT.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Switch the accent and restyle immediately.
pub fn set_accent(ctx: &egui::Context, color: Color32) {
    let [r, g, b, _] = color.to_array();
    ACCENT.store(u32::from_be_bytes([0, r, g, b]), Ordering::Relaxed);
    apply_visuals(ctx);
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn number() -> FontFamily {
    FontFamily::Name("number".into())
}

/// A rounded, subtly bordered card like the metric tiles.
pub fn card() -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(20))
}

/// Uppercase, letter-spaced label ("TOTAL TOKENS").
pub fn caps_label(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(12.0).extra_letter_spacing(0.8).color(MUTED)
}

/// Card heading ("Daily Spend").
pub fn card_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).family(semibold()).size(15.0).color(TEXT));
    ui.add_space(10.0);
}

/// Large monospace value.
pub fn big_number(text: &str) -> RichText {
    RichText::new(text).family(number()).size(26.0).color(TEXT)
}

pub fn apply(ctx: &egui::Context, accent: Color32) {
    install_fonts(ctx);
    // Always dark, even when Windows is in light mode.
    ctx.set_theme(egui::Theme::Dark);
    set_accent(ctx, accent);
    apply_spacing(ctx);
}

fn apply_visuals(ctx: &egui::Context) {
    let accent = accent();
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = Color32::from_rgb(14, 14, 16); // text inputs, plot background
    v.faint_bg_color = Color32::from_rgb(16, 16, 18); // sidebar, striped rows
    v.code_bg_color = CARD;
    v.hyperlink_color = accent;
    v.selection.bg_fill = accent.gamma_multiply(0.3);
    v.selection.stroke = Stroke::new(1.0, accent);
    v.override_text_color = None;

    let radius = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;
    for (state, fill, stroke) in [
        (&mut w.inactive, Color32::from_rgb(24, 24, 27), BORDER),
        (&mut w.hovered, CARD_HOVER, Color32::from_rgb(58, 58, 64)),
        (&mut w.active, Color32::from_rgb(36, 36, 40), Color32::from_rgb(70, 70, 78)),
        (&mut w.open, CARD_HOVER, BORDER),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 200, 206));
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);

    ctx.set_visuals_of(egui::Theme::Dark, v);
}

fn apply_spacing(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::new(24.0, semibold())),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.window_margin = Margin::same(18);
        style.animation_time = 0.18;
    });
}

/// Use Windows' Segoe UI and Consolas when present; otherwise keep egui's built-in fonts.
fn install_fonts(ctx: &egui::Context) {
    let dir = std::path::Path::new("C:\\Windows\\Fonts");
    let load = |file: &str| std::fs::read(dir.join(file)).ok();

    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, file: &str, family: FontFamily| {
        if let Some(bytes) = load(file) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            fonts.families.entry(family).or_default().insert(0, name.into());
        }
    };
    add("segoe", "segoeui.ttf", FontFamily::Proportional);
    add("consolas", "consola.ttf", FontFamily::Monospace);
    add("segoe-semibold", "seguisb.ttf", semibold());
    add("consolas-bold", "consolab.ttf", number());

    // Named families need a fallback chain (egui's defaults cover symbols).
    let proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let monospace = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    fonts.families.entry(semibold()).or_default().extend(proportional);
    fonts.families.entry(number()).or_default().extend(monospace);

    ctx.set_fonts(fonts);
}
