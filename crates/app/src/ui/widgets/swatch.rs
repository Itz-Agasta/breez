//! Wallpaper swatch: a small gradient tile with a selection ring.

use eframe::egui::{Color32, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme;
use crate::ui::widgets::vertical_gradient;

pub fn swatch(ui: &mut Ui, top: Color32, bottom: Color32, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    vertical_gradient(ui.painter(), rect, 6, top, bottom);
    if selected {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Outside,
        );
    } else if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.0, theme::BORDER_STRONG),
            StrokeKind::Outside,
        );
    }
    response
}
