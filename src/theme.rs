//! Dark dashboard look: softly bordered cards, Segoe UI for text and a bold
//! monospace face for big numbers. Like Chrome's colour themes, every surface
//! is a tone of the chosen accent colour, so picking a colour tints the whole app.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, RichText, Stroke,
    TextStyle, Ui, Visuals,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Claude Desktop's clay orange, the default accent.
pub const CLAUDE: Color32 = Color32::from_rgb(217, 119, 87);
pub const BLUE: Color32 = Color32::from_rgb(77, 141, 255);

/// Accent colour choices offered in Settings → App style.
pub const PRESETS: [(&str, Color32); 9] = [
    ("Claude", CLAUDE),
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
static ACCENT: AtomicU32 = AtomicU32::new(0xD97757);

/// Highlight colour for links, chart lines, bars and selections.
pub fn accent() -> Color32 {
    let v = ACCENT.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Switch the accent and restyle the whole app immediately.
pub fn set_accent(ctx: &egui::Context, color: Color32) {
    let [r, g, b, _] = color.to_array();
    ACCENT.store(u32::from_be_bytes([0, r, g, b]), Ordering::Relaxed);
    apply_visuals(ctx);
}

/// Tones of the accent's hue, darkest to lightest.
pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub input: Color32,
    pub card: Color32,
    pub widget: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub grid: Color32,
    pub muted: Color32,
    pub text_soft: Color32,
    pub text: Color32,
    /// Fill and text for selected items; `None` uses the accent.
    pub selected: Option<(Color32, Color32)>,
}

/// The default look, sampled from Claude Desktop's dark theme: neutral
/// near-black surfaces, cards outlined rather than filled, grey selection.
const CLAUDE_PALETTE: Palette = Palette {
    bg: Color32::from_rgb(21, 21, 21),
    sidebar: Color32::from_rgb(17, 17, 17),
    input: Color32::from_rgb(31, 31, 31),
    card: Color32::from_rgb(21, 21, 21),
    widget: Color32::from_rgb(29, 29, 29),
    hover: Color32::from_rgb(35, 35, 35),
    active: Color32::from_rgb(51, 51, 51),
    border: Color32::from_rgb(42, 42, 42),
    border_strong: Color32::from_rgb(55, 55, 55),
    grid: Color32::from_rgb(38, 38, 38),
    muted: Color32::from_rgb(140, 140, 140),
    text_soft: Color32::from_rgb(196, 196, 194),
    text: Color32::from_rgb(242, 243, 240),
    selected: Some((Color32::from_rgb(35, 35, 35), Color32::from_rgb(242, 243, 240))),
};

/// Neutral near-black greys, used with the Blue accent.
const NEUTRAL: Palette = Palette {
    bg: Color32::from_rgb(10, 10, 11),
    sidebar: Color32::from_rgb(16, 16, 18),
    input: Color32::from_rgb(14, 14, 16),
    card: Color32::from_rgb(20, 20, 22),
    widget: Color32::from_rgb(24, 24, 27),
    hover: Color32::from_rgb(30, 30, 33),
    active: Color32::from_rgb(36, 36, 40),
    border: Color32::from_rgb(38, 38, 42),
    border_strong: Color32::from_rgb(58, 58, 64),
    grid: Color32::from_rgb(44, 44, 48),
    muted: Color32::from_rgb(140, 140, 148),
    text_soft: Color32::from_rgb(200, 200, 206),
    text: Color32::from_rgb(236, 236, 238),
    selected: None,
};

/// Claude (the default) and Blue have hand-picked palettes. Any other colour
/// tints the whole app: same hue, saturation scaled down so surfaces stay calm,
/// lightness set per role.
pub fn palette() -> Palette {
    let accent = accent();
    if accent == CLAUDE {
        return CLAUDE_PALETTE;
    }
    if accent == BLUE {
        return NEUTRAL;
    }
    let (h, s, _) = to_hsl(accent);
    let tone = |sat: f32, light: f32| from_hsl(h, s * sat, light);
    Palette {
        bg: tone(0.50, 0.06),
        sidebar: tone(0.55, 0.08),
        input: tone(0.50, 0.07),
        card: tone(0.50, 0.10),
        widget: tone(0.50, 0.13),
        hover: tone(0.52, 0.165),
        active: tone(0.55, 0.20),
        border: tone(0.45, 0.18),
        border_strong: tone(0.45, 0.28),
        grid: tone(0.40, 0.20),
        muted: tone(0.30, 0.64),
        text_soft: tone(0.22, 0.82),
        text: tone(0.20, 0.94),
        selected: None,
    }
}

pub fn bg() -> Color32 {
    palette().bg
}

pub fn text() -> Color32 {
    palette().text
}

pub fn muted() -> Color32 {
    palette().muted
}

pub fn grid() -> Color32 {
    palette().grid
}

/// sRGB colour -> (hue degrees, saturation 0..1, lightness 0..1).
fn to_hsl(c: Color32) -> (f32, f32, f32) {
    let [r, g, b, _] = c.to_array().map(|v| v as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s.min(1.0), l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b))
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn number() -> FontFamily {
    FontFamily::Name("number".into())
}

/// A rounded, subtly bordered card like the metric tiles.
pub fn card() -> Frame {
    let p = palette();
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(20))
}

/// Uppercase, letter-spaced label ("TOTAL TOKENS").
pub fn caps_label(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(12.0).extra_letter_spacing(0.8).color(muted())
}

/// Card heading ("Daily Spend").
pub fn card_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).family(semibold()).size(15.0).color(text()));
    ui.add_space(10.0);
}

/// Large monospace value.
pub fn big_number(value: &str) -> RichText {
    RichText::new(value).family(number()).size(26.0).color(text())
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
    let p = palette();
    let mut v = Visuals::dark();
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.input; // text inputs, plot background
    v.faint_bg_color = p.sidebar; // sidebar, striped rows
    v.code_bg_color = p.card;
    v.hyperlink_color = accent;
    let (sel_fill, sel_text) = p.selected.unwrap_or((accent.gamma_multiply(0.3), accent));
    v.selection.bg_fill = sel_fill;
    v.selection.stroke = Stroke::new(1.0, sel_text);
    v.override_text_color = None;

    let radius = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill, stroke) in [
        (&mut w.inactive, p.widget, p.border),
        (&mut w.hovered, p.hover, p.border_strong),
        (&mut w.active, p.active, p.border_strong),
        (&mut w.open, p.hover, p.border),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.fg_stroke = Stroke::new(1.0, p.text_soft);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.active.fg_stroke = Stroke::new(1.5, p.text);

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
        // Always-visible scroll bars; egui's default floating ones only show on hover.
        style.spacing.scroll = egui::style::ScrollStyle::solid();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_round_trips() {
        for (_, c) in PRESETS {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l).to_array();
            for (a, b) in back.iter().zip(c.to_array()) {
                assert!((*a as i32 - b as i32).abs() <= 1, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn palette_keeps_the_hue_and_stays_dark() {
        for (name, c) in PRESETS {
            ACCENT.store(u32::from_be_bytes([0, c.r(), c.g(), c.b()]), Ordering::Relaxed);
            let p = palette();
            let (_, _, bg_l) = to_hsl(p.bg);
            let (_, _, text_l) = to_hsl(p.text);
            assert!(bg_l < 0.1 && text_l > 0.9, "{name}");
            if name == "Claude" {
                assert_eq!(p.bg, CLAUDE_PALETTE.bg, "default is the Claude look");
            } else if name == "Blue" {
                assert_eq!(p.bg, NEUTRAL.bg, "blue stays neutral");
            } else if name != "Silver" {
                let (h, _) = (to_hsl(c).0, ());
                let (bh, _, _) = to_hsl(p.card);
                let diff = (h - bh).abs().min(360.0 - (h - bh).abs());
                assert!(diff < 12.0, "{name}: accent hue {h}, card hue {bh}");
            }
            println!("{name:12} bg {:?} card {:?} text {:?}", p.bg.to_array(), p.card.to_array(), p.text.to_array());
        }
    }
}
