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

pub fn source_color(source: Source) -> Color32 {
    match source {
        Source::GitHub => Color32::from_rgb(110, 118, 129),
        Source::Npm => Color32::from_rgb(203, 56, 55),
        Source::PyPI => Color32::from_rgb(55, 117, 169),
        Source::Crates => Color32::from_rgb(222, 109, 42),
    }
}

/// Small rounded square in the registry's colour with its initial.
pub fn source_badge(ui: &mut Ui, source: Source) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    let letter = match source {
        Source::GitHub => "G",
        Source::Npm => "n",
        Source::PyPI => "P",
        Source::Crates => "C",
    };
    ui.painter().rect_filled(rect, 4.0, source_color(source));
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, letter, FontId::proportional(11.5), Color32::WHITE);
    resp.on_hover_text(source.label())
}

/// Inline icon (non-interactive) at text height.
pub fn inline(ui: &mut Ui, icon: Icon, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    resp
}
