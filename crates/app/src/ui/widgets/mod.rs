//! Reusable painter-based controls styled from theme tokens. Each widget is
//! a plain function drawing with the egui painter, so the design tokens in
//! `theme.rs` stay the single source of truth.

pub mod button;
pub mod chip;
pub mod grid9;
pub mod segmented;
pub mod slider;
pub mod swatch;
pub mod switch;

use eframe::egui::{Color32, CornerRadius, Mesh, Painter, Rect, pos2, vec2};

/// Vertical two-stop gradient with rounded corners. Rounded caps are painted
/// as flat rects (the color barely changes over the cap height) and the body
/// is a vertex-colored mesh quad, which egui interpolates smoothly.
pub fn vertical_gradient(painter: &Painter, rect: Rect, radius: u8, top: Color32, bottom: Color32) {
    let h = rect.height();
    if h <= 0.0 || rect.width() <= 0.0 {
        return;
    }
    let cap = (f32::from(radius)).max(2.0).min(h / 2.0);
    let bottom_cap_color = top.lerp_to_gamma(bottom, (h - cap) / h);
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(rect.width(), cap)),
        CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        },
        top,
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(rect.min.x, rect.max.y - cap), rect.max),
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        },
        bottom_cap_color,
    );

    let mut mesh = Mesh::default();
    let body_top = rect.min.y + cap;
    let body_bottom = rect.max.y - cap;
    let top_color = top.lerp_to_gamma(bottom, cap / h);
    mesh.colored_vertex(pos2(rect.min.x, body_top), top_color);
    mesh.colored_vertex(pos2(rect.max.x, body_top), top_color);
    mesh.colored_vertex(pos2(rect.min.x, body_bottom), bottom_cap_color);
    mesh.colored_vertex(pos2(rect.max.x, body_bottom), bottom_cap_color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    painter.add(mesh);
}
