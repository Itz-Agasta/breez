//! Horizontal slider: 4px track, accent fill, round knob. Click or drag to
//! set. Takes the full available width by default.

use eframe::egui::{Rangef, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use std::ops::RangeInclusive;

use crate::theme;

pub fn slider(ui: &mut Ui, value: &mut f32, range: RangeInclusive<f32>) -> Response {
    let width = ui.available_width().max(60.0);
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click_and_drag());
    let (min, max) = (*range.start(), *range.end());

    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let t = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
        let new = min + t * (max - min);
        if new != *value {
            *value = new;
            response.mark_changed();
        }
    }

    let t = if max > min {
        ((*value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let track = Rect::from_x_y_ranges(
        Rangef::new(rect.min.x, rect.max.x),
        Rangef::new(rect.center().y - 2.0, rect.center().y + 2.0),
    );
    ui.painter().rect_filled(track, 2, theme::BG_CONTROL_ACTIVE);
    let fill_end = rect.min.x + t * rect.width();
    let fill = Rect::from_x_y_ranges(
        Rangef::new(rect.min.x, fill_end),
        Rangef::new(track.min.y, track.max.y),
    );
    ui.painter().rect_filled(fill, 2, theme::ACCENT);
    let knob_center = pos2(
        fill_end.clamp(rect.min.x + 6.0, rect.max.x - 6.0),
        rect.center().y,
    );
    ui.painter().circle_filled(knob_center, 6.0, theme::TEXT);
    ui.painter().rect_stroke(
        Rect::from_center_size(knob_center, vec2(12.0, 12.0)),
        6,
        Stroke::new(1.0, theme::BORDER_STRONG),
        StrokeKind::Outside,
    );
    response
}
