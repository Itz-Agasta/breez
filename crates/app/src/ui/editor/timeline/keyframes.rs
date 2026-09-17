//! Zoom lane: translucent keyframe segments with a painted easing ramp and
//! level text. Click selects (binding the Zoom & pan inspector), dragging
//! the body moves, the edges resize, double-click or the transport diamond
//! adds, Delete removes. Edits keep `timeline.zoom` sorted and
//! non-overlapping by clamping against neighbor segments.

use eframe::egui::{
    Color32, CornerRadius, CursorIcon, Id, Key, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use super::{EditorState, Track};
use crate::app::Session;
use crate::theme;
use breez_core::project::{Easing, ZoomSegment};

const HANDLE_WIDTH: f32 = 8.0;
const MIN_SEGMENT_NS: u64 = 200_000_000;
const DEFAULT_SEGMENT_NS: u64 = 2_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragPart {
    Body,
    Left,
    Right,
}

/// An in-flight segment drag: which part grabbed where, and the segment
/// bounds at drag start (deltas apply to these, so the drag is stable even
/// if intermediate clamping bites).
pub(crate) struct ZoomDrag {
    index: usize,
    part: DragPart,
    start_x: f32,
    start_in_ns: u64,
    start_out_ns: u64,
}

pub(super) fn show(
    ui: &mut Ui,
    state: &mut EditorState,
    session: &mut Session,
    track: &Track,
    lane: Rect,
    lane_response: &Response,
) {
    if track.duration_ns == 0 {
        return;
    }
    if lane_response.double_clicked()
        && let Some(pos) = lane_response.interact_pointer_pos()
    {
        add_at(state, session, track.ns_at(pos.x));
    }
    if let Some(index) = state.selected_zoom
        && index < session.project.timeline.zoom.len()
        && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
    {
        session.project.timeline.zoom.remove(index);
        state.selected_zoom = None;
        state.dirty = true;
    }
    for index in 0..session.project.timeline.zoom.len() {
        let segment = &session.project.timeline.zoom[index];
        let min_x = track.x_at(segment.in_ns).min(track.left + track.width);
        let max_x = track.x_at(segment.out_ns).min(track.left + track.width);
        let body = Rect::from_min_max(pos2(min_x, lane.min.y + 5.0), pos2(max_x, lane.max.y - 5.0));
        if body.width() < 2.0 {
            continue;
        }
        paint_segment(ui, session, index, body, state.selected_zoom == Some(index));
        interact(ui, state, session, track, index, body);
    }
}

/// Insert a segment starting at `t_ns` (transport diamond and lane
/// double-click). Inside an existing segment it selects that one instead;
/// a gap too small for a minimum-length segment is left alone.
pub(super) fn add_at(state: &mut EditorState, session: &mut Session, t_ns: u64) {
    let duration = session.project.timeline.duration_ns();
    if duration == 0 {
        return;
    }
    let t = t_ns.min(duration.saturating_sub(1));
    let zoom = &mut session.project.timeline.zoom;
    if let Some(index) = zoom.iter().position(|s| s.in_ns <= t && t < s.out_ns) {
        state.selected_zoom = Some(index);
        state.selected_music = None;
        return;
    }
    let index = zoom.partition_point(|s| s.in_ns <= t);
    let gap_end = zoom.get(index).map_or(duration, |s| s.in_ns);
    let out_ns = t.saturating_add(DEFAULT_SEGMENT_NS).min(gap_end);
    if out_ns.saturating_sub(t) < MIN_SEGMENT_NS {
        return;
    }
    zoom.insert(
        index,
        ZoomSegment {
            in_ns: t,
            out_ns,
            level: 1.8,
            anchor: [0.5, 0.5],
            follow_cursor: true,
            easing: Easing::Smooth,
        },
    );
    state.selected_zoom = Some(index);
    state.selected_music = None;
    state.dirty = true;
}

fn paint_segment(ui: &Ui, session: &Session, index: usize, body: Rect, selected: bool) {
    let segment = &session.project.timeline.zoom[index];
    let radius = CornerRadius::same(5);
    ui.painter()
        .rect_filled(body, radius, Color32::from_white_alpha(14));
    ui.painter().rect_stroke(
        body,
        radius,
        if selected {
            Stroke::new(1.5, theme::ACCENT)
        } else {
            Stroke::new(1.0, theme::BORDER_STRONG)
        },
        StrokeKind::Inside,
    );
    let painter = ui.painter_at(body.shrink2(vec2(6.0, 0.0)));
    let glyph = Rect::from_min_size(
        pos2(body.min.x + 8.0, body.center().y - 4.0),
        vec2(10.0, 8.0),
    );
    easing_glyph(&painter, glyph, segment.easing);
    painter.text(
        pos2(glyph.max.x + 6.0, body.center().y),
        eframe::egui::Align2::LEFT_CENTER,
        format!("{:.1}x", segment.level),
        eframe::egui::FontId::new(10.0, eframe::egui::FontFamily::Monospace),
        theme::TEXT_MUTED,
    );
}

/// Tiny painted ramp icon (font-independent): the segment's easing curve
/// drawn bottom-left to top-right.
fn easing_glyph(painter: &eframe::egui::Painter, rect: Rect, easing: Easing) {
    let at = |x: f32, y: f32| {
        pos2(
            rect.min.x + x * rect.width(),
            rect.max.y - y * rect.height(),
        )
    };
    let points: Vec<Pos2> = match easing {
        Easing::Linear => vec![at(0.0, 0.0), at(1.0, 1.0)],
        Easing::Smooth => (0..=4)
            .map(|i| {
                let p = i as f32 / 4.0;
                at(p, p * p * (3.0 - 2.0 * p))
            })
            .collect(),
        Easing::Snap => vec![at(0.0, 0.0), at(0.3, 0.0), at(0.3, 1.0), at(1.0, 1.0)],
    };
    painter.add(Shape::line(points, Stroke::new(1.2, theme::TEXT_MUTED)));
}

fn interact(
    ui: &mut Ui,
    state: &mut EditorState,
    session: &mut Session,
    track: &Track,
    index: usize,
    body: Rect,
) {
    let ns_per_px = track.duration_ns as f64 / f64::from(track.width);
    let parts = [
        (DragPart::Left, edge_zone(body, false)),
        (DragPart::Right, edge_zone(body, true)),
        (DragPart::Body, body.shrink2(vec2(HANDLE_WIDTH / 2.0, 0.0))),
    ];
    for (part, zone) in parts {
        if zone.width() <= 0.0 {
            continue;
        }
        let sense = if part == DragPart::Body {
            Sense::click_and_drag()
        } else {
            Sense::drag()
        };
        let response = ui.interact(zone, Id::new(("zoom-seg", index, part as u8)), sense);
        response.clone().on_hover_cursor(if part == DragPart::Body {
            CursorIcon::Grab
        } else {
            CursorIcon::ResizeHorizontal
        });
        if response.clicked() {
            state.selected_zoom = Some(index);
            state.selected_music = None;
        }
        if response.drag_started() {
            state.selected_zoom = Some(index);
            state.selected_music = None;
            let segment = &session.project.timeline.zoom[index];
            state.zoom_drag = Some(ZoomDrag {
                index,
                part,
                start_x: response
                    .interact_pointer_pos()
                    .map_or(zone.center().x, |p| p.x),
                start_in_ns: segment.in_ns,
                start_out_ns: segment.out_ns,
            });
        }
        if response.dragged()
            && let (Some(drag), Some(pos)) = (&state.zoom_drag, response.interact_pointer_pos())
            && drag.index == index
            && drag.part == part
        {
            apply_drag(session, drag, pos.x, ns_per_px);
            state.dirty = true;
        }
        if response.drag_stopped() {
            state.zoom_drag = None;
        }
    }
}

fn edge_zone(body: Rect, right: bool) -> Rect {
    let x = if right { body.max.x } else { body.min.x };
    Rect::from_center_size(pos2(x, body.center().y), vec2(HANDLE_WIDTH, body.height()))
}

/// Move/resize against the drag-start bounds, clamped to the neighboring
/// segments (and the timeline ends) so the sorted non-overlap invariant
/// holds without ever reordering the vec.
fn apply_drag(session: &mut Session, drag: &ZoomDrag, pointer_x: f32, ns_per_px: f64) {
    let duration = session.project.timeline.duration_ns();
    let zoom = &mut session.project.timeline.zoom;
    let prev_end = if drag.index > 0 {
        zoom[drag.index - 1].out_ns
    } else {
        0
    };
    let next_start = zoom.get(drag.index + 1).map_or(duration, |s| s.in_ns);
    let delta_ns = f64::from(pointer_x - drag.start_x) * ns_per_px;
    let shifted = |base: u64| (base as f64 + delta_ns).max(0.0) as u64;
    let segment = &mut zoom[drag.index];
    match drag.part {
        DragPart::Body => {
            let len = drag.start_out_ns - drag.start_in_ns;
            segment.in_ns = super::clamp_ns(
                shifted(drag.start_in_ns),
                prev_end,
                next_start.saturating_sub(len),
            );
            segment.out_ns = segment.in_ns + len;
        }
        DragPart::Left => {
            segment.in_ns = super::clamp_ns(
                shifted(drag.start_in_ns),
                prev_end,
                segment.out_ns.saturating_sub(MIN_SEGMENT_NS),
            );
        }
        DragPart::Right => {
            segment.out_ns = super::clamp_ns(
                shifted(drag.start_out_ns),
                segment.in_ns.saturating_add(MIN_SEGMENT_NS),
                next_start,
            );
        }
    }
}
