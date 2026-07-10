//! Cursor section: click highlight only in the MVP; size/smoothing arrive
//! with the pinray cursor re-render phase.

use eframe::egui::{FontFamily, FontId, RichText, Ui};

use super::switch_row;
use crate::app::Session;
use crate::theme;

pub fn show(ui: &mut Ui, session: &mut Session) {
    switch_row(
        ui,
        "Click highlight",
        &mut session.project.style.cursor.click_highlight,
    );
    ui.label(
        RichText::new("Size and smoothing arrive with cursor re-render.")
            .font(FontId::new(11.0, FontFamily::Proportional))
            .color(theme::TEXT_FAINT),
    );
}
