//! Music lane: one blue waveform segment per music track, painted from the
//! peaks cache a worker thread fills. Dragging the body moves the track's
//! timeline offset, the two top handles drag the fade-in/out lengths,
//! click selects (binding the inspector row), Delete removes. The worker
//! also measures each file's duration, backfilled into the project.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Id, Key, Rect, Sense, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use super::{EditorState, Track};
use crate::app::Session;
use crate::theme;
use breez_codec::AudioPeaks;
use breez_core::package::RecPackage;
use breez_core::project::Project;

/// Keep at least this much of a track on the timeline when dragging it out
/// toward the end.
const MIN_VISIBLE_NS: u64 = 100_000_000;
const HANDLE_RADIUS: f32 = 4.0;

/// Waveform peaks per music file, decoded on a worker thread and cached in
/// the package (`cache/peaks/`). Absent entry = still decoding.
pub struct Waveforms {
    tx: mpsc::Sender<(String, PathBuf, PathBuf)>,
    rx: mpsc::Receiver<(String, Option<AudioPeaks>)>,
    peaks: HashMap<String, Option<AudioPeaks>>,
}

impl Waveforms {
    /// Start the worker and queue every music track already in the session.
    pub fn spawn(ctx: &eframe::egui::Context, session: &Session) -> Self {
        let (tx, job_rx) = mpsc::channel::<(String, PathBuf, PathBuf)>();
        let (result_tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("breez-peaks".to_owned())
            .spawn(move || {
                while let Ok((rel, audio, cache)) = job_rx.recv() {
                    let result = match breez_codec::generate_peaks(&audio, &cache) {
                        Ok(peaks) => Some(peaks),
                        Err(e) => {
                            log::warn!("peaks for {rel}: {e}");
                            None
                        }
                    };
                    if result_tx.send((rel, result)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn peaks thread");
        let this = Self {
            tx,
            rx,
            peaks: HashMap::new(),
        };
        for track in &session.project.timeline.music {
            this.request(&session.package, &track.file);
        }
        this
    }

    /// Queue peak generation for one music file (call once per import).
    pub fn request(&self, package: &RecPackage, rel: &str) {
        let Ok(audio) = package.resolve(rel) else {
            return;
        };
        let _ = self
            .tx
            .send((rel.to_owned(), audio, package.peaks_path(rel)));
    }

    /// Drain worker results. Returns true when a measured duration was
    /// backfilled into the project (caller marks the editor dirty).
    pub fn poll(&mut self, project: &mut Project) -> bool {
        let mut dirty = false;
        while let Ok((rel, result)) = self.rx.try_recv() {
            if let Some(peaks) = &result
                && let Some(track) = project.timeline.music.iter_mut().find(|t| t.file == rel)
                && track.duration_ns == 0
            {
                track.duration_ns = peaks.duration_ns;
                dirty = true;
            }
            self.peaks.insert(rel, result);
        }
        dirty
    }

    fn get(&self, rel: &str) -> Option<&AudioPeaks> {
        self.peaks.get(rel)?.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragPart {
    Body,
    FadeIn,
    FadeOut,
}

/// An in-flight music drag: deltas apply to the values at drag start so the
/// mapping stays stable while the track moves under the pointer.
pub(crate) struct MusicDrag {
    index: usize,
    part: DragPart,
    start_x: f32,
    start_ns: u64,
}

pub(super) fn show(
    ui: &mut Ui,
    state: &mut EditorState,
    session: &mut Session,
    track: &Track,
    lane: Rect,
) {
    if track.duration_ns == 0 {
        return;
    }
    if let Some(index) = state.selected_music
        && index < session.project.timeline.music.len()
        && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
    {
        session.project.timeline.music.remove(index);
        state.selected_music = None;
        state.dirty = true;
    }
    for index in 0..session.project.timeline.music.len() {
        let music = &session.project.timeline.music[index];
        let (start_ns, end_ns) = span(music, track.duration_ns);
        if start_ns >= end_ns {
            continue;
        }
        let body = Rect::from_min_max(
            pos2(track.x_at(start_ns), lane.min.y + 4.0),
            pos2(track.x_at(end_ns), lane.max.y - 4.0),
        );
        if body.width() < 2.0 {
            continue;
        }
        paint_track(ui, state, session, track, index, body);
        interact(ui, state, session, track, index, body);
    }
}

/// Audible span on the timeline: from the offset to the media end (when
/// known) or the timeline end.
fn span(music: &breez_core::project::MusicTrack, timeline_ns: u64) -> (u64, u64) {
    let mut end = timeline_ns;
    if music.duration_ns > 0 {
        end = end.min(music.offset_ns.saturating_add(music.duration_ns));
    }
    (music.offset_ns.min(timeline_ns), end)
}

fn paint_track(
    ui: &Ui,
    state: &EditorState,
    session: &Session,
    track: &Track,
    index: usize,
    body: Rect,
) {
    let music = &session.project.timeline.music[index];
    let radius = CornerRadius::same(5);
    ui.painter()
        .rect_filled(body, radius, theme::MUSIC_BLUE.linear_multiply(0.10));
    ui.painter().rect_stroke(
        body,
        radius,
        if state.selected_music == Some(index) {
            Stroke::new(1.5, theme::ACCENT)
        } else {
            Stroke::new(1.0, theme::MUSIC_BLUE.linear_multiply(0.35))
        },
        StrokeKind::Inside,
    );
    let inner = body.shrink2(vec2(2.0, 2.0));
    let painter = ui.painter_at(inner);
    match state.waveforms.get(&music.file) {
        Some(peaks) if !peaks.peaks.is_empty() => {
            // One bar every 2px, indexed by source time under that pixel.
            let ns_per_px = track.duration_ns as f64 / f64::from(track.width.max(1.0));
            let mut x = inner.min.x;
            while x < inner.max.x {
                let src_ns = (f64::from(x - body.min.x) * ns_per_px) as u64;
                let i = (src_ns as u128 * u128::from(peaks.peaks_per_sec) / 1_000_000_000) as usize;
                let amp = peaks.peaks.get(i).copied().unwrap_or(0.0).clamp(0.04, 1.0);
                let half = amp * inner.height() * 0.5;
                painter.vline(
                    x,
                    (inner.center().y - half)..=(inner.center().y + half),
                    Stroke::new(1.2, theme::MUSIC_BLUE.linear_multiply(0.85)),
                );
                x += 2.0;
            }
        }
        Some(_) | None => {
            painter.text(
                pos2(body.min.x + 8.0, body.center().y),
                Align2::LEFT_CENTER,
                "Analyzing\u{2026}",
                FontId::new(10.0, FontFamily::Proportional),
                theme::TEXT_MUTED,
            );
        }
    }
    // Fade wedges: a line from each bottom corner up to its handle.
    let (fade_in_x, fade_out_x) = fade_points(music, track, body);
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(120));
    painter.line_segment(
        [pos2(body.min.x, body.max.y), pos2(fade_in_x, body.min.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(body.max.x, body.max.y), pos2(fade_out_x, body.min.y)],
        stroke,
    );
    for x in [fade_in_x, fade_out_x] {
        ui.painter().circle(
            pos2(x, body.min.y),
            HANDLE_RADIUS - 1.0,
            theme::TEXT,
            Stroke::new(1.0, theme::BG_PANEL),
        );
    }
}

/// X positions of the fade-in and fade-out handles on `body`.
fn fade_points(music: &breez_core::project::MusicTrack, track: &Track, body: Rect) -> (f32, f32) {
    let (start_ns, end_ns) = span(music, track.duration_ns);
    let span_ns = (end_ns.saturating_sub(start_ns)).max(1);
    let px = |ns: u64| (ns.min(span_ns) as f32 / span_ns as f32) * body.width();
    (
        body.min.x + px(music.fade_in_ns),
        body.max.x - px(music.fade_out_ns),
    )
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
    let (fade_in_x, fade_out_x) = fade_points(&session.project.timeline.music[index], track, body);
    let handle = |x: f32| {
        Rect::from_center_size(
            pos2(x, body.min.y),
            vec2(HANDLE_RADIUS * 2.5, HANDLE_RADIUS * 2.5),
        )
    };
    let parts = [
        (DragPart::FadeIn, handle(fade_in_x)),
        (DragPart::FadeOut, handle(fade_out_x)),
        (DragPart::Body, body),
    ];
    for (part, zone) in parts {
        let sense = if part == DragPart::Body {
            Sense::click_and_drag()
        } else {
            Sense::drag()
        };
        let response = ui.interact(zone, Id::new(("music-track", index, part as u8)), sense);
        response.clone().on_hover_cursor(if part == DragPart::Body {
            CursorIcon::Grab
        } else {
            CursorIcon::ResizeHorizontal
        });
        if response.clicked() || response.drag_started() {
            state.selected_music = Some(index);
            state.selected_zoom = None;
        }
        if response.drag_started() {
            let music = &session.project.timeline.music[index];
            state.music_drag = Some(MusicDrag {
                index,
                part,
                start_x: response
                    .interact_pointer_pos()
                    .map_or(zone.center().x, |p| p.x),
                start_ns: match part {
                    DragPart::Body => music.offset_ns,
                    DragPart::FadeIn => music.fade_in_ns,
                    DragPart::FadeOut => music.fade_out_ns,
                },
            });
        }
        if response.dragged()
            && let (Some(drag), Some(pos)) = (&state.music_drag, response.interact_pointer_pos())
            && drag.index == index
            && drag.part == part
        {
            apply_drag(session, drag, pos.x, ns_per_px, track.duration_ns);
            state.dirty = true;
        }
        if response.drag_stopped() {
            state.music_drag = None;
        }
    }
}

fn apply_drag(
    session: &mut Session,
    drag: &MusicDrag,
    pointer_x: f32,
    ns_per_px: f64,
    timeline_ns: u64,
) {
    let music = &mut session.project.timeline.music[drag.index];
    let delta_ns = f64::from(pointer_x - drag.start_x) * ns_per_px;
    let shifted = (drag.start_ns as f64 + delta_ns).max(0.0) as u64;
    match drag.part {
        DragPart::Body => {
            music.offset_ns = shifted.min(timeline_ns.saturating_sub(MIN_VISIBLE_NS));
        }
        part => {
            let (start_ns, end_ns) = span(music, timeline_ns);
            let span_ns = end_ns.saturating_sub(start_ns);
            // Fade-out grows leftward, so its delta runs against the drag.
            let fade = if part == DragPart::FadeOut {
                (drag.start_ns as f64 - delta_ns).max(0.0) as u64
            } else {
                shifted
            };
            let fade = fade.min(span_ns);
            if part == DragPart::FadeIn {
                music.fade_in_ns = fade;
            } else {
                music.fade_out_ns = fade;
            }
        }
    }
}
