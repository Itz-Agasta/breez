//! Lane rows with gutter labels. The screen lane carries the clip painter,
//! the zoom lane its keyframe segments, the cursor lane click diamonds
//! from the event log, and the music lane waveform tracks. Empty lane space
//! doubles as a scrub surface.

use eframe::egui::{Align2, FontFamily, FontId, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::{EditorState, Track, clip, keyframes, seek_interaction, waveform};
use crate::app::Session;
use crate::theme;

const LANES: &[(&str, f32)] = &[
    ("Screen", 54.0),
    ("Zoom", 30.0),
    ("Cursor", 22.0),
    ("Music", 34.0),
];

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session, track: &Track) {
    // Rows must touch so gutter and separator lines stay continuous.
    ui.spacing_mut().item_spacing.y = 0.0;
    for (label, height) in LANES {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), *height), Sense::click_and_drag());
        seek_interaction(&response, state, session, track);
        ui.painter().text(
            pos2(rect.min.x + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            *label,
            FontId::new(11.0, FontFamily::Proportional),
            theme::TEXT_LABEL,
        );
        ui.painter().vline(
            rect.min.x + theme::GUTTER_WIDTH - 8.0,
            rect.y_range(),
            Stroke::new(1.0, theme::BORDER),
        );
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 0.5,
            Stroke::new(1.0, theme::BORDER),
        );
        match *label {
            "Screen" => clip::show(ui, state, session, track, rect),
            "Zoom" => keyframes::show(ui, state, session, track, rect, &response),
            "Cursor" => click_diamonds(ui, state, session, track, rect),
            "Music" => waveform::show(ui, state, session, track, rect),
            _ => {}
        }
    }
}

/// One small diamond per recorded click, mapped from take source time to
/// timeline time (trimmed-away clicks disappear with their clip).
fn click_diamonds(ui: &Ui, state: &EditorState, session: &Session, track: &Track, lane: Rect) {
    if track.duration_ns == 0 {
        return;
    }
    let timeline = &session.project.timeline;
    for (take, clicks) in &state.clicks {
        for click in clicks {
            let Some(t_ns) = timeline.timeline_ns_for(*take, click.t_ns) else {
                continue;
            };
            let center = pos2(track.x_at(t_ns), lane.center().y);
            let r = 3.5;
            ui.painter().add(Shape::convex_polygon(
                vec![
                    center - vec2(0.0, r),
                    center + vec2(r, 0.0),
                    center + vec2(0.0, r),
                    center - vec2(r, 0.0),
                ],
                theme::TEXT_MUTED,
                Stroke::NONE,
            ));
        }
    }
}
