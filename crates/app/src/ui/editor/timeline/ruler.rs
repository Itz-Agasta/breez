//! Time ruler: second ticks scaled so the whole timeline fits the width.
//! The strip is also the primary scrub surface (click/drag to seek).

use eframe::egui::{Align2, FontFamily, FontId, Sense, Stroke, Ui, pos2, vec2};

use super::{EditorState, Track, seek_interaction};
use crate::app::Session;
use crate::theme;
use crate::ui::record::format_secs;

const HEIGHT: f32 = 22.0;

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &Session, track: &Track) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::click_and_drag());
    seek_interaction(&response, state, session, track);
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(1.0, theme::BORDER),
    );
    let total_secs = (track.duration_ns as f64 / 1e9).ceil().max(1.0) as u64;
    if track.width < 40.0 {
        return;
    }
    let px_per_sec = track.width / total_secs as f32;
    let step = label_step(px_per_sec);
    let mut sec = 0;
    while sec <= total_secs {
        let x = track.left + sec as f32 * px_per_sec;
        ui.painter().vline(
            x,
            eframe::egui::Rangef::new(rect.max.y - 5.0, rect.max.y - 1.0),
            Stroke::new(1.0, theme::BORDER_STRONG),
        );
        ui.painter().text(
            pos2(x + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            format_secs(sec),
            FontId::new(10.0, FontFamily::Monospace),
            theme::TEXT_LABEL,
        );
        sec += step;
    }
}

/// Pick a tick step so labels stay ~70px apart. Beyond the nice-step table
/// the step keeps growing (whole minutes), so tick count stays bounded by
/// the track width no matter how long the timeline is.
fn label_step(px_per_sec: f32) -> u64 {
    for step in [1, 2, 5, 10, 15, 30, 60, 120, 300] {
        if px_per_sec * step as f32 >= 70.0 {
            return step;
        }
    }
    ((70.0 / px_per_sec / 60.0).ceil() as u64).max(6) * 60
}
