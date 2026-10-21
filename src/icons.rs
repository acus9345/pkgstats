//! Small line icons painted with egui shapes (no emoji / icon fonts),
//! plus the buttons and badges built from them.

use crate::api::Source;
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui,
    pos2, vec2,
};
use std::f32::consts::{PI, TAU};

pub const GREEN: Color32 = Color32::from_rgb(34, 197, 94);
pub const RED: Color32 = Color32::from_rgb(239, 68, 68);
pub const GOLD: Color32 = Color32::from_rgb(234, 179, 8);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Menu,
    WinMinimize,
    WinMaximize,
    WinRestore,
    Link,
    Clock,
    Trash,
    Back,
    Search,
    Settings,
    User,
    Package,
    External,
    Star,
    StarFilled,
    Refresh,
    Close,
    Check,
    Cross,
    Dots,
    Info,
    Up,
    Down,
}

/// Paint `icon` centred in `rect` with a single colour.
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    // Coordinates in icon space: (-0.5, -0.5) is the top-left corner.
    let p = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    let stroke = Stroke::new((s / 10.0).clamp(1.3, 2.0), color);
    let arc = |center: Pos2, r: f32, from: f32, to: f32| -> Vec<Pos2> {
        let steps = 24;
        (0..=steps)
            .map(|i| {
                let a = from + (to - from) * i as f32 / steps as f32;
                pos2(center.x + r * a.cos(), center.y + r * a.sin())
            })
            .collect()
    };

    match icon {
        Icon::Menu => {
            for y in [-0.25, 0.0, 0.25] {
                painter.line_segment([p(-0.32, y), p(0.32, y)], stroke);
            }
        }
        // Window controls use thin 1px lines, like Windows' own caption buttons.
        Icon::WinMinimize => {
            painter.line_segment([p(-0.32, 0.0), p(0.32, 0.0)], Stroke::new(1.0, color));
        }
        Icon::WinMaximize => {
            let r = Rect::from_min_max(p(-0.3, -0.3), p(0.3, 0.3));
            painter.rect_stroke(r, CornerRadius::same(1), Stroke::new(1.0, color), StrokeKind::Middle);
        }
        Icon::WinRestore => {
            let front = Rect::from_min_max(p(-0.32, -0.18), p(0.18, 0.32));
            painter.rect_stroke(front, CornerRadius::same(1), Stroke::new(1.0, color), StrokeKind::Middle);
            painter.line(vec![p(-0.18, -0.18), p(-0.18, -0.32), p(0.32, -0.32), p(0.32, 0.18), p(0.18, 0.18)], Stroke::new(1.0, color));
        }
        Icon::Link => {
            // Two chain links: rotated ovals overlapping along the diagonal.
            let (cos, sin) = ((PI / 4.0).cos(), -(PI / 4.0).sin());
            for offset in [-0.14, 0.14] {
                let center = p(offset, -offset);
                let pts: Vec<Pos2> = (0..32)
                    .map(|i| {
                        let t = i as f32 * TAU / 32.0;
                        let (x, y) = (0.24 * t.cos() * s, 0.12 * t.sin() * s);
                        pos2(center.x + x * cos - y * sin, center.y + x * sin + y * cos)
                    })
                    .collect();
                painter.add(Shape::closed_line(pts, stroke));
            }
        }
        Icon::Clock => {
            painter.circle_stroke(c, 0.4 * s, stroke);
            painter.line(vec![p(0.0, -0.24), p(0.0, 0.0), p(0.17, 0.12)], stroke);
        }
        Icon::Trash => {
            painter.line_segment([p(-0.36, -0.24), p(0.36, -0.24)], stroke);
            painter.line(vec![p(-0.12, -0.24), p(-0.12, -0.36), p(0.12, -0.36), p(0.12, -0.24)], stroke);
            painter.line(vec![p(-0.27, -0.24), p(-0.22, 0.38), p(0.22, 0.38), p(0.27, -0.24)], stroke);
            painter.line_segment([p(0.0, -0.08), p(0.0, 0.24)], stroke);
        }
        Icon::Back => {
            painter.line(vec![p(0.12, -0.3), p(-0.18, 0.0), p(0.12, 0.3)], stroke);
        }
        Icon::Search => {
            painter.circle_stroke(p(-0.07, -0.07), 0.25 * s, stroke);
            painter.line_segment([p(0.12, 0.12), p(0.36, 0.36)], stroke);
        }
        Icon::Settings => {
            painter.circle_stroke(c, 0.24 * s, stroke);
            painter.circle_stroke(c, 0.08 * s, stroke);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let (dx, dy) = (a.cos(), a.sin());
                painter.line_segment([p(dx * 0.24, dy * 0.24), p(dx * 0.4, dy * 0.4)], stroke);
            }
        }
        Icon::User => {
            painter.circle_stroke(p(0.0, -0.17), 0.15 * s, stroke);
            painter.line(arc(p(0.0, 0.4), 0.3 * s, PI, TAU), stroke);
        }
        Icon::Package => {
            let body = Rect::from_min_max(p(-0.33, -0.3), p(0.33, 0.33));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Middle);
            painter.line_segment([p(-0.33, -0.08), p(0.33, -0.08)], stroke);
            painter.line_segment([p(-0.08, 0.08), p(0.08, 0.08)], stroke);
        }
        Icon::External => {
            painter.line(vec![p(0.0, -0.3), p(-0.3, -0.3), p(-0.3, 0.3), p(0.3, 0.3), p(0.3, 0.0)], stroke);
            painter.line_segment([p(-0.04, 0.04), p(0.34, -0.34)], stroke);
            painter.line(vec![p(0.08, -0.34), p(0.34, -0.34), p(0.34, -0.08)], stroke);
        }
        Icon::Star | Icon::StarFilled => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let a = -PI / 2.0 + i as f32 * PI / 5.0;
                    let r = if i % 2 == 0 { 0.42 } else { 0.18 };
                    p(r * a.cos(), r * a.sin() + 0.03)
                })
                .collect();
            if icon == Icon::StarFilled {
                // Non-convex: fill the inner pentagon, then each point as a triangle.
                let inner: Vec<Pos2> = pts.iter().skip(1).step_by(2).copied().collect();
                painter.add(Shape::convex_polygon(inner, color, Stroke::NONE));
                for i in (0..10).step_by(2) {
                    let tri = vec![pts[(i + 9) % 10], pts[i], pts[i + 1]];
                    painter.add(Shape::convex_polygon(tri, color, Stroke::NONE));
                }
            }
            painter.add(Shape::closed_line(pts, stroke));
        }
        Icon::Refresh => {
            let (from, to) = (-PI / 2.0 + 0.5, -PI / 2.0 + 0.5 + 1.65 * PI);
            let r = 0.3 * s;
            painter.line(arc(c, r, from, to), stroke);
            // Arrow head at the end of the arc, pointing along the tangent.
            let end = pos2(c.x + r * to.cos(), c.y + r * to.sin());
            let t = vec2(-to.sin(), to.cos());
            let nrm = vec2(to.cos(), to.sin());
            let h = 0.16 * s;
            painter.line(vec![end - t * h + nrm * h * 0.8, end, end - t * h - nrm * h * 0.8], stroke);
        }
        Icon::Close | Icon::Cross => {
            let k = if icon == Icon::Close { 0.25 } else { 0.28 };
            painter.line_segment([p(-k, -k), p(k, k)], stroke);
            painter.line_segment([p(-k, k), p(k, -k)], stroke);
        }
        Icon::Check => {
            painter.line(vec![p(-0.3, 0.02), p(-0.08, 0.25), p(0.32, -0.25)], stroke);
        }
        Icon::Dots => {
            for x in [-0.25, 0.0, 0.25] {
                painter.circle_filled(p(x, 0.0), 0.06 * s, color);
            }
        }
        Icon::Info => {
            painter.circle_stroke(c, 0.4 * s, stroke);
            painter.circle_filled(p(0.0, -0.18), 0.05 * s, color);
            painter.line_segment([p(0.0, -0.04), p(0.0, 0.22)], stroke);
        }
        Icon::Up => {
            painter.add(Shape::convex_polygon(vec![p(0.0, -0.25), p(0.28, 0.2), p(-0.28, 0.2)], color, Stroke::NONE));
        }
        Icon::Down => {
            painter.add(Shape::convex_polygon(vec![p(-0.28, -0.2), p(0.28, -0.2), p(0.0, 0.25)], color, Stroke::NONE));
        }
    }
}

/// Square icon-only button.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button_colored(ui, icon, None, tooltip)
}

pub fn icon_button_colored(ui: &mut Ui, icon: Icon, color: Option<Color32>, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact(&resp);
        if resp.hovered() || resp.has_focus() {
            ui.painter().rect_filled(rect, v.corner_radius, v.weak_bg_fill);
        }
        let color = color.unwrap_or(v.fg_stroke.color);
        paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(15.0, 15.0)), icon, color);
    }
    resp.on_hover_text(tooltip)
}

/// Small "i" button: hover shows `text` as a tooltip, click pins it open as a popup.
pub fn info(ui: &mut Ui, text: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    let popup_id = egui::Popup::default_response_id(&resp);
    let pinned = egui::Popup::is_id_open(ui.ctx(), popup_id);
    if ui.is_rect_visible(rect) {
        let color = if resp.hovered() || pinned { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
        paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), Icon::Info, color);
    }
    egui::Popup::from_toggle_button_response(&resp).width(300.0).show(|ui| {
        ui.label(text);
    });
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if pinned { resp } else { resp.on_hover_ui(|ui| {
        ui.set_max_width(300.0);
        ui.label(text);
    }) }
}

/// Button with an optional leading icon. `selected` draws it as an active toggle.
pub fn button(ui: &mut Ui, icon: Option<Icon>, text: &str, selected: bool, icon_color: Option<Color32>) -> Response {
    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let pad = 8.0;
    let icon_w = if icon.is_some() { 14.0 + if text.is_empty() { 0.0 } else { 6.0 } } else { 0.0 };
    let size = vec2(pad * 2.0 + icon_w + galley.size().x, ui.spacing().interact_size.y.max(24.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let v = ui.style().interact_selectable(&resp, selected);
        let painter = ui.painter();
        painter.rect_filled(rect, v.corner_radius, v.weak_bg_fill);
        painter.rect_stroke(rect, v.corner_radius, v.bg_stroke, StrokeKind::Inside);
        let text_color = v.fg_stroke.color;
        let mut x = rect.left() + pad;
        if let Some(icon) = icon {
            let r = Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0));
            paint(painter, r, icon, icon_color.unwrap_or(text_color));
            x += icon_w;
        }
        let pos = pos2(x, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, text_color);
    }
    resp
}

/// Full-width sidebar row: no border, background only when hovered or selected.
/// Leads with either an icon or a registry badge.
pub fn nav_row(ui: &mut Ui, icon: Option<Icon>, badge: Option<Source>, text: &str, selected: bool) -> Response {
    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let v = ui.style().interact_selectable(&resp, selected);
        let painter = ui.painter();
        if selected || resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(6), v.weak_bg_fill);
        }
        let lead = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(16.0, 16.0));
        if let Some(source) = badge {
            painter.rect_filled(lead, 4.0, source_color(source));
            painter.text(lead.center(), egui::Align2::CENTER_CENTER, initial(source), FontId::proportional(10.5), Color32::WHITE);
        } else if let Some(icon) = icon {
            paint(painter, lead.shrink(1.0), icon, v.fg_stroke.color);
        }
        // Clip long names (package titles) to the row.
        let text_pos = pos2(rect.left() + 36.0, rect.center().y - galley.size().y / 2.0);
        painter.with_clip_rect(rect.shrink2(vec2(4.0, 0.0))).galley(text_pos, galley, v.fg_stroke.color);
    }
    resp
}

fn initial(source: Source) -> &'static str {
    match source {
        Source::GitHub => "G",
        Source::Npm => "n",
        Source::PyPI => "P",
        Source::Crates => "C",
    }
}

pub fn source_color(source: Source) -> Color32 {
    match source {
        Source::GitHub => Color32::from_rgb(110, 118, 129),
        Source::Npm => Color32::from_rgb(203, 56, 55),
        Source::PyPI => Color32::from_rgb(55, 117, 169),
        Source::Crates => Color32::from_rgb(222, 109, 42),
    }
}

/// Minimize / maximize / close button for the custom title bar: full bar
/// height, no border, background only on hover (red for close, like Windows).
pub fn window_button(ui: &mut Ui, icon: Icon, height: f32, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(46.0, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let is_close = icon == Icon::Close;
        let mut fg = ui.visuals().text_color();
        if resp.hovered() {
            let fill = if is_close { Color32::from_rgb(196, 43, 28) } else { ui.visuals().widgets.hovered.weak_bg_fill };
            ui.painter().rect_filled(rect, 0.0, fill);
            if is_close {
                fg = Color32::WHITE;
            }
        }
        // Same 10px glyph box for all three, like Windows' own caption buttons.
        let icon_rect = Rect::from_center_size(rect.center(), vec2(10.0, 10.0));
        if is_close {
            // Thinner cross than the in-app Close icon.
            let s = Stroke::new(1.0, fg);
            ui.painter().line_segment([icon_rect.left_top(), icon_rect.right_bottom()], s);
            ui.painter().line_segment([icon_rect.left_bottom(), icon_rect.right_top()], s);
        } else {
            paint(ui.painter(), icon_rect, icon, fg);
        }
    }
    resp.on_hover_text(tooltip)
}

/// Round colour swatch; a ring marks the selected one.
pub fn swatch(ui: &mut Ui, color: Color32, selected: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        if selected {
            ui.painter().circle_stroke(c, 14.0, Stroke::new(2.0, ui.visuals().strong_text_color()));
        } else if resp.hovered() {
            ui.painter().circle_stroke(c, 14.0, Stroke::new(1.0, ui.visuals().weak_text_color()));
        }
        ui.painter().circle_filled(c, 10.5, color);
    }
    resp.on_hover_text(tooltip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Small rounded square in the registry's colour with its initial.
pub fn source_badge(ui: &mut Ui, source: Source) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, source_color(source));
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial(source), FontId::proportional(11.5), Color32::WHITE);
    resp.on_hover_text(source.label())
}

/// Inline icon (non-interactive) at text height.
pub fn inline(ui: &mut Ui, icon: Icon, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    resp
}
