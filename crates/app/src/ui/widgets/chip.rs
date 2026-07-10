//! Tiny rounded status chips ("Draft", "0 keyframes").

use eframe::egui::{Color32, CornerRadius, FontId, Sense, Ui, vec2};

use crate::theme;

pub fn chip(ui: &mut Ui, label: &str, fg: Color32) {
    let font = FontId::new(10.5, theme::medium());
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 14.0, 18.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_CHIP),
        theme::BG_CONTROL,
    );
    let pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(pos, galley, fg);
}
