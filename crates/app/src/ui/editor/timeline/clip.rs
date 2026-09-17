//! Screen clip: filmstrip thumbnails with white trim handles. Dragging a
//! handle edits `src_in_ns`/`src_out_ns` (non-destructive trim); the clip
//! body is a scrub surface like the ruler.

use eframe::egui::{
    Color32, CornerRadius, Id, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};

use super::{EditorState, Track, seek_interaction};
use crate::app::Session;
use crate::theme;

const HANDLE_WIDTH: f32 = 10.0;
const MIN_CLIP_NS: u64 = 100_000_000;

/// Which trim handle a drag started on, plus the fixed mapping captured at
/// drag start. Pixel-to-time mapping is frozen for the whole drag because
/// the fit-width track rescales as the trim changes the duration.
pub(crate) struct TrimDrag {
    pub clip: usize,
    pub right: bool,
    pub start_x: f32,
    pub start_src_ns: u64,
    pub ns_per_px: f64,
}

pub(super) fn show(
    ui: &mut Ui,
    state: &mut EditorState,
    session: &mut Session,
    track: &Track,
    lane: Rect,
) {
    let timeline = &session.project.timeline;
    if track.duration_ns == 0 || timeline.clips.is_empty() {
        return;
    }
    let clips: Vec<(usize, Rect)> = timeline
        .clips
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let start = timeline.clip_start_ns(index);
            // Timeline length, not source length: `clip_start_ns` already
            // divides by speed, so using the raw source span would paint a
            // 2x clip at twice its real width, overlapping the next clip and
            // putting both trim handles at the wrong x.
            let len = timeline.clip_len_ns(index);
            let body = Rect::from_min_max(
                pos2(track.x_at(start), lane.min.y + 7.0),
                pos2(track.x_at(start + len), lane.max.y - 7.0),
            );
            (index, body)
        })
        .collect();

    for (index, body) in clips {
        paint_body(ui, state, session, index, body);
        trim_handles(ui, state, session, track, index, body);
    }
}

fn paint_body(ui: &mut Ui, state: &mut EditorState, session: &Session, index: usize, body: Rect) {
    let clip = &session.project.timeline.clips[index];
    let take = session.project.takes.iter().find(|t| t.id == clip.take);
    let radius = CornerRadius::same(theme::RADIUS_CLIP);
    ui.painter()
        .rect_filled(body, radius, Color32::from_rgb(0x05, 0x05, 0x05));

    if let Some(take) = take
        && take.duration_ns > 0
    {
        let aspect = take.width.max(1) as f32 / take.height.max(1) as f32;
        let slot_width = (body.height() * aspect).max(8.0);
        let painter = ui.painter_at(body);
        let mut x = body.min.x;
        while x < body.max.x {
            let slot = Rect::from_min_max(pos2(x, body.min.y), pos2(x + slot_width, body.max.y));
            // Thumbs cover the take; pick by the slot's source position.
            let body_fraction = (slot.center().x - body.min.x) / body.width();
            let src_ns = clip.src_in_ns as f64
                + f64::from(body_fraction)
                    * (clip.src_out_ns.saturating_sub(clip.src_in_ns)) as f64;
            let fraction = (src_ns / take.duration_ns as f64) as f32;
            if let Some(texture) = state.filmstrip.texture(ui.ctx(), take.id, fraction) {
                painter.add(Shape::image(
                    texture.id(),
                    slot,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                ));
            }
            x += slot_width;
        }
    }
    ui.painter().rect_stroke(
        body,
        radius,
        Stroke::new(1.0, theme::BORDER_STRONG),
        StrokeKind::Inside,
    );
}

fn trim_handles(
    ui: &mut Ui,
    state: &mut EditorState,
    session: &mut Session,
    track: &Track,
    index: usize,
    body: Rect,
) {
    let ns_per_px = track.duration_ns as f64 / f64::from(track.width);
    for right in [false, true] {
        let x = if right { body.max.x } else { body.min.x };
        let zone =
            Rect::from_center_size(pos2(x, body.center().y), vec2(HANDLE_WIDTH, body.height()));
        let id = Id::new(("trim", index, right));
        let response = ui.interact(zone, id, Sense::drag());
        if response.drag_started() {
            let clip = &session.project.timeline.clips[index];
            state.trim = Some(TrimDrag {
                clip: index,
                right,
                start_x: response.interact_pointer_pos().map_or(x, |p| p.x),
                start_src_ns: if right {
                    clip.src_out_ns
                } else {
                    clip.src_in_ns
                },
                ns_per_px,
            });
        }
        if response.dragged()
            && let (Some(drag), Some(pos)) = (&state.trim, response.interact_pointer_pos())
            && drag.clip == index
            && drag.right == right
        {
            apply_trim(session, drag, pos.x);
            state.dirty = true;
        }
        if response.drag_stopped() {
            state.trim = None;
            // Trim may have shortened the timeline under the playhead.
            let duration = session.project.timeline.duration_ns();
            if state.player.playhead_ns() > duration {
                state.player.seek(session, duration);
            }
        }
        // White capsule handle.
        let handle = Rect::from_center_size(pos2(x, body.center().y), vec2(5.0, 18.0));
        let color = if response.hovered() || response.dragged() {
            theme::ACCENT
        } else {
            Color32::from_rgb(0xc9, 0xc9, 0xc9)
        };
        ui.painter().rect_filled(handle, 3, color);
    }

    // The remaining body is a scrub surface, underneath the handles.
    let inner = body.shrink2(vec2(HANDLE_WIDTH / 2.0, 0.0));
    if inner.width() > 0.0 {
        let response = ui.interact(
            inner,
            Id::new(("clip-body", index)),
            Sense::click_and_drag(),
        );
        seek_interaction(&response, state, session, track);
    }
}

fn apply_trim(session: &mut Session, drag: &TrimDrag, pointer_x: f32) {
    let clip = &mut session.project.timeline.clips[drag.clip];
    let Some(take) = session.project.takes.iter().find(|t| t.id == clip.take) else {
        return;
    };
    let delta_ns = f64::from(pointer_x - drag.start_x) * drag.ns_per_px;
    let target = (drag.start_src_ns as f64 + delta_ns).max(0.0) as u64;
    if drag.right {
        clip.src_out_ns = super::clamp_ns(
            target,
            clip.src_in_ns.saturating_add(MIN_CLIP_NS),
            take.duration_ns,
        );
    } else {
        clip.src_in_ns = target.min(clip.src_out_ns.saturating_sub(MIN_CLIP_NS));
    }
}
