//! Timeline: transport bar, ruler, lanes with the screen clip, and the
//! playhead. The whole timeline fits the track width (no zoom yet); all
//! rows share one `Track` mapping between x pixels and timeline time.

pub(crate) mod clip;
pub mod filmstrip;
pub(crate) mod keyframes;
mod lanes;
mod playhead;
mod ruler;
mod transport;
pub(crate) mod waveform;

use eframe::egui::{Frame, Panel, Rect, Response, Stroke, Ui};

use super::EditorState;
use crate::app::Session;
use crate::theme;

/// Clamp into `lo..=hi`, tolerating an inverted range.
///
/// `u64::clamp` panics when `lo > hi`, and the drag handlers build their
/// bounds from timeline state that a trim can invalidate: shortening the
/// clip can leave a zoom segment ending past the timeline, or a clip shorter
/// than the minimum length. `sanitize` only restores those invariants at
/// load, so the bound has to survive the inverted case rather than assume it
/// away. Collapsing to `lo` keeps the dragged edge against its neighbour.
pub(super) fn clamp_ns(value: u64, lo: u64, hi: u64) -> u64 {
    value.clamp(lo, hi.max(lo))
}

/// Pixel <-> timeline-time mapping for the lane/ruler track area.
#[derive(Clone, Copy)]
pub(super) struct Track {
    left: f32,
    width: f32,
    duration_ns: u64,
}

impl Track {
    fn new(panel: Rect, duration_ns: u64) -> Self {
        Self {
            left: panel.min.x + theme::GUTTER_WIDTH,
            width: (panel.max.x - 14.0 - panel.min.x - theme::GUTTER_WIDTH).max(1.0),
            duration_ns,
        }
    }

    fn x_at(&self, t_ns: u64) -> f32 {
        if self.duration_ns == 0 {
            return self.left;
        }
        self.left + self.width * (t_ns as f32 / self.duration_ns as f32)
    }

    fn ns_at(&self, x: f32) -> u64 {
        let fraction = ((x - self.left) / self.width).clamp(0.0, 1.0);
        (fraction as f64 * self.duration_ns as f64) as u64
    }
}

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) {
    state.filmstrip.poll();
    Panel::bottom("timeline")
        .exact_size(theme::TIMELINE_HEIGHT)
        .frame(Frame::new().fill(theme::BG_PANEL))
        .show_separator_line(false)
        .show(ui, |ui| {
            let panel = ui.max_rect();
            ui.painter().hline(
                panel.x_range(),
                panel.min.y + 0.5,
                Stroke::new(1.0, theme::BORDER),
            );
            let track = Track::new(panel, session.project.timeline.duration_ns());
            transport::show(ui, state, session);
            let ruler_top = ui.cursor().min.y;
            ruler::show(ui, state, session, &track);
            lanes::show(ui, state, session, &track);
            let region = Rect::from_min_max(
                eframe::egui::pos2(track.left, ruler_top),
                eframe::egui::pos2(track.left + track.width, ui.cursor().min.y),
            );
            playhead::show(ui, region, &track, state.player.playhead_ns());
        });
}

/// Click/drag-to-seek shared by the ruler and empty lane space. Playback
/// pauses while scrubbing and resumes on release.
pub(super) fn seek_interaction(
    response: &Response,
    state: &mut EditorState,
    session: &Session,
    track: &Track,
) {
    if (response.drag_started() || response.clicked()) && state.player.is_playing() {
        state.player.pause();
        state.resume_after_scrub = true;
    }
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        state.player.seek(session, track.ns_at(pos.x));
    }
    if (response.drag_stopped() || response.clicked()) && state.resume_after_scrub {
        state.resume_after_scrub = false;
        state.player.play(session);
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_ns;

    #[test]
    fn clamp_ns_should_behave_like_clamp_for_a_valid_range() {
        assert_eq!(clamp_ns(5, 0, 10), 5);
        assert_eq!(clamp_ns(20, 0, 10), 10);
        assert_eq!(clamp_ns(0, 3, 10), 3);
    }

    #[test]
    fn clamp_ns_should_collapse_to_the_low_bound_when_the_range_is_inverted() {
        // A zoom segment left ending past a trimmed timeline produces this.
        assert_eq!(clamp_ns(7, 5_200_000_000, 5_100_000_000), 5_200_000_000);
    }
}
