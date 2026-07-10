//! Toggle switch (36x20) with an animated knob.

use eframe::egui::{Response, Sense, Ui, epaint::CircleShape, lerp, pos2, vec2};

use crate::theme;

pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool(response.id, *on);
    let track = if *on {
        theme::ACCENT
    } else {
        theme::BG_CONTROL_ACTIVE
    };
    ui.painter().rect_filled(rect, 10, track);
    let knob_x = lerp(rect.min.x + 10.0..=rect.max.x - 10.0, t);
    let knob = if *on {
        theme::BG_WINDOW
    } else {
        theme::TEXT_MUTED
    };
    ui.painter().add(CircleShape::filled(
        pos2(knob_x, rect.center().y),
        7.0,
        knob,
    ));
    response
}
