//! Audio section: system audio gain plus a volume row per music track.

use eframe::egui::{FontFamily, FontId, RichText, Ui};

use super::slider_row;
use crate::app::Session;
use crate::theme;

/// Returns true when a gain changed this frame (system gain applies live to
/// preview playback; music gains feed the per-tick music mix).
pub fn show(ui: &mut Ui, session: &mut Session) -> bool {
    let mut changed = slider_row(
        ui,
        "System audio",
        &mut session.project.style.system_audio_gain,
        breez_core::project::GAIN_RANGE,
        |v| format!("{}%", (v * 100.0).round() as u32),
    );
    if session.project.timeline.music.is_empty() {
        ui.label(
            RichText::new("Import music from the Music tool.")
                .font(FontId::new(11.0, FontFamily::Proportional))
                .color(theme::TEXT_FAINT),
        );
        return changed;
    }
    for track in &mut session.project.timeline.music {
        let name = std::path::Path::new(&track.file)
            .file_stem()
            .map_or_else(|| track.file.clone(), |s| s.to_string_lossy().into_owned());
        changed |= slider_row(
            ui,
            &name,
            &mut track.gain,
            breez_core::project::GAIN_RANGE,
            |v| format!("{}%", (v * 100.0).round() as u32),
        );
    }
    changed
}
