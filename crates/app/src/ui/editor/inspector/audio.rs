//! Audio section: system audio gain now, music rows arrive in Phase 5.

use eframe::egui::{FontFamily, FontId, RichText, Ui};

use super::slider_row;
use crate::app::Session;
use crate::theme;

pub fn show(ui: &mut Ui, session: &mut Session) {
    slider_row(
        ui,
        "System audio",
        &mut session.project.style.system_audio_gain,
        0.0..=2.0,
        |v| format!("{}%", (v * 100.0).round() as u32),
    );
    ui.label(
        RichText::new("Music tracks land in Phase 5.")
            .font(FontId::new(11.0, FontFamily::Proportional))
            .color(theme::TEXT_FAINT),
    );
}
