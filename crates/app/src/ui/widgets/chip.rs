//! Tiny rounded status chips ("Draft", "0 keyframes").

use eframe::egui::{Align2, Color32, CornerRadius, FontId, Sense, Ui, vec2};

use crate::theme;

pub fn chip(ui: &mut Ui, label: &str, fg: Color32) {
    let font = FontId::new(10.5, theme::medium());
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::PLACEHOLDER)
        .size()
        .x;
    let (rect, _) = ui.allocate_exact_size(vec2(text_width + 14.0, 18.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_CHIP),
        theme::BG_CONTROL,
    );
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, font, fg);
}
