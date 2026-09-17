//! Shared render math: the animated zoom view at a timeline instant, the
//! follow-cursor anchor, and click ripples from the input-event log. The
//! egui preview and the export compositor both call these, so the two can
//! never drift apart.

use crate::events::InputEvent;
use crate::project::{Easing, MusicTrack, Timeline, ZoomSegment};

/// Zoom level ramps in/out over this long at segment edges.
const RAMP_NS: u64 = 500_000_000;
/// Snap easing uses a much shorter ramp.
const SNAP_RAMP_NS: u64 = 150_000_000;
/// A click ripple expands and fades over this long.
pub const RIPPLE_NS: u64 = 500_000_000;
/// The follow-cursor anchor glides to a new click over this long.
const GLIDE_NS: u64 = 250_000_000;

/// Resolved zoom transform at one instant: magnification and the normalized
/// 0..1 focus point within the source frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomView {
    pub level: f32,
    pub anchor: [f32; 2],
}

impl ZoomView {
    pub const NEUTRAL: Self = Self {
        level: 1.0,
        anchor: [0.5, 0.5],
    };
}

/// Zoom view at timeline time `t_ns`. `cursor` is the follow-cursor anchor
/// (see [`cursor_anchor`]); it applies only when the active segment asks to
/// follow the cursor. Segments are assumed sorted and non-overlapping
/// (`Project::sanitize` and the editor maintain this).
pub fn zoom_at(timeline: &Timeline, t_ns: u64, cursor: Option<[f32; 2]>) -> ZoomView {
    let Some(segment) = timeline
        .zoom
        .iter()
        .find(|s| s.in_ns <= t_ns && t_ns < s.out_ns)
    else {
        return ZoomView::NEUTRAL;
    };
    let anchor = match (segment.follow_cursor, cursor) {
        (true, Some(anchor)) => anchor,
        _ => segment.anchor,
    };
    ZoomView {
        level: 1.0 + (segment.level - 1.0) * edge_ramp(segment, t_ns),
        anchor,
    }
}

/// Eased 0..1 progress: ramps up after `in_ns`, back down before `out_ns`,
/// holds 1.0 in between.
fn edge_ramp(segment: &ZoomSegment, t_ns: u64) -> f32 {
    let ramp = match segment.easing {
        Easing::Snap => SNAP_RAMP_NS,
        _ => RAMP_NS,
    };
    let len = segment.out_ns.saturating_sub(segment.in_ns);
    let ramp = ramp.min(len / 2).max(1);
    let edge = (t_ns - segment.in_ns).min(segment.out_ns - t_ns);
    let p = (edge as f32 / ramp as f32).min(1.0);
    match segment.easing {
        Easing::Linear => p,
        Easing::Smooth => smoothstep(p),
        Easing::Snap => 1.0 - (1.0 - p).powi(3),
    }
}

fn smoothstep(p: f32) -> f32 {
    p * p * (3.0 - 2.0 * p)
}

/// Follow-cursor anchor at source time `src_ns`: the latest click at or
/// before it, gliding from the previous click position over [`GLIDE_NS`] so
/// a zoomed view pans instead of jump-cutting. `clicks` must be the take's
/// button-down events sorted by time.
pub fn cursor_anchor(clicks: &[InputEvent], src_ns: u64) -> Option<[f32; 2]> {
    let upto = clicks.partition_point(|e| e.t_ns <= src_ns);
    let current = clicks.get(upto.checked_sub(1)?)?;
    let Some(previous) = upto.checked_sub(2).and_then(|i| clicks.get(i)) else {
        return Some([current.x, current.y]);
    };
    let dt = src_ns - current.t_ns;
    if dt >= GLIDE_NS {
        return Some([current.x, current.y]);
    }
    let p = smoothstep(dt as f32 / GLIDE_NS as f32);
    Some([
        previous.x + (current.x - previous.x) * p,
        previous.y + (current.y - previous.y) * p,
    ])
}

/// Volume of a music track at timeline time `t_ns`: the track gain shaped
/// by linear fade-in/out, 0.0 when inaudible. The audible span starts at
/// `offset_ns` and ends at the media end (when known) or the timeline end,
/// whichever comes first; the fade-out finishes at that span end. Preview
/// and export both apply this, so the mix cannot differ between them.
pub fn music_gain_at(track: &MusicTrack, t_ns: u64, timeline_duration_ns: u64) -> f32 {
    let start = track.offset_ns;
    let span = audible_span_ns(track, timeline_duration_ns);
    let end = start.saturating_add(span);
    if span == 0 || t_ns < start || t_ns >= end {
        return 0.0;
    }
    let mut gain = track.gain;
    let fade_in = track.fade_in_ns.min(span);
    if fade_in > 0 {
        gain *= ((t_ns - start) as f32 / fade_in as f32).min(1.0);
    }
    let fade_out = track.fade_out_ns.min(span);
    if fade_out > 0 {
        gain *= ((end - t_ns) as f32 / fade_out as f32).min(1.0);
    }
    gain
}

/// Length of a track's audible span, measured from its offset: it ends at
/// the media end when that is known, or at the timeline end, whichever comes
/// first. The export builds its fade anchors from this too, so the preview
/// mix and the exported mix cannot disagree about where the fades sit.
pub fn audible_span_ns(track: &MusicTrack, timeline_duration_ns: u64) -> u64 {
    let mut end = timeline_duration_ns;
    if track.duration_ns > 0 {
        end = end.min(track.offset_ns.saturating_add(track.duration_ns));
    }
    end.saturating_sub(track.offset_ns)
}

/// One expanding click ripple: normalized position and 0..1 age.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ripple {
    pub x: f32,
    pub y: f32,
    pub progress: f32,
}

/// Ripples alive at source time `src_ns`: clicks from the last
/// [`RIPPLE_NS`]. `clicks` must be sorted by time.
pub fn ripples_at(clicks: &[InputEvent], src_ns: u64) -> Vec<Ripple> {
    let upto = clicks.partition_point(|e| e.t_ns <= src_ns);
    clicks[..upto]
        .iter()
        .rev()
        .take_while(|e| src_ns - e.t_ns < RIPPLE_NS)
        .map(|e| Ripple {
            x: e.x,
            y: e.y,
            progress: (src_ns - e.t_ns) as f32 / RIPPLE_NS as f32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{InputKind, MouseButton};

    fn segment(in_ns: u64, out_ns: u64, level: f32, easing: Easing) -> ZoomSegment {
        ZoomSegment {
            in_ns,
            out_ns,
            level,
            anchor: [0.25, 0.75],
            follow_cursor: false,
            easing,
        }
    }

    fn timeline_with(segment: ZoomSegment) -> Timeline {
        Timeline {
            zoom: vec![segment],
            ..Timeline::default()
        }
    }

    fn click(t_ns: u64, x: f32, y: f32) -> InputEvent {
        InputEvent {
            t_ns,
            kind: InputKind::Down,
            button: Some(MouseButton::Left),
            x,
            y,
        }
    }

    #[test]
    fn zoom_at_should_be_neutral_outside_segments() {
        let timeline = timeline_with(segment(1_000_000_000, 3_000_000_000, 2.0, Easing::Linear));
        assert_eq!(zoom_at(&timeline, 0, None), ZoomView::NEUTRAL);
        assert_eq!(zoom_at(&timeline, 3_000_000_000, None), ZoomView::NEUTRAL);
    }

    #[test]
    fn zoom_at_should_ramp_up_hold_and_ramp_down() {
        let timeline = timeline_with(segment(1_000_000_000, 4_000_000_000, 2.0, Easing::Linear));
        assert_eq!(zoom_at(&timeline, 1_000_000_000, None).level, 1.0);
        // Mid-ramp (250ms of the 500ms ramp) is halfway up.
        assert!((zoom_at(&timeline, 1_250_000_000, None).level - 1.5).abs() < 1e-3);
        assert_eq!(zoom_at(&timeline, 2_500_000_000, None).level, 2.0);
        assert!((zoom_at(&timeline, 3_750_000_000, None).level - 1.5).abs() < 1e-3);
    }

    #[test]
    fn zoom_at_should_use_cursor_only_when_following() {
        let mut seg = segment(0, 2_000_000_000, 2.0, Easing::Smooth);
        assert_eq!(
            zoom_at(&timeline_with(seg.clone()), 1_000_000_000, Some([0.9, 0.1])).anchor,
            [0.25, 0.75]
        );
        seg.follow_cursor = true;
        assert_eq!(
            zoom_at(&timeline_with(seg), 1_000_000_000, Some([0.9, 0.1])).anchor,
            [0.9, 0.1]
        );
    }

    #[test]
    fn cursor_anchor_should_glide_between_clicks() {
        let clicks = vec![click(0, 0.0, 0.0), click(1_000_000_000, 1.0, 1.0)];
        assert_eq!(cursor_anchor(&clicks, 500_000_000), Some([0.0, 0.0]));
        // Long after the second click the anchor has settled on it.
        assert_eq!(cursor_anchor(&clicks, 2_000_000_000), Some([1.0, 1.0]));
        // Mid-glide sits strictly between the two.
        let mid = cursor_anchor(&clicks, 1_100_000_000).unwrap();
        assert!(mid[0] > 0.0 && mid[0] < 1.0);
    }

    fn music(offset_ns: u64, duration_ns: u64) -> MusicTrack {
        MusicTrack {
            file: "media/music/track.mp3".to_owned(),
            offset_ns,
            gain: 0.8,
            fade_in_ns: 1_000_000_000,
            fade_out_ns: 2_000_000_000,
            duration_ns,
        }
    }

    #[test]
    fn music_gain_should_be_zero_outside_the_audible_span() {
        let track = music(1_000_000_000, 4_000_000_000);
        assert_eq!(music_gain_at(&track, 0, 20_000_000_000), 0.0);
        assert_eq!(music_gain_at(&track, 5_000_000_000, 20_000_000_000), 0.0);
    }

    #[test]
    fn music_gain_should_ramp_through_fades_and_hold_the_gain_between() {
        let track = music(1_000_000_000, 10_000_000_000);
        // Halfway through the 1s fade-in.
        let v = music_gain_at(&track, 1_500_000_000, 20_000_000_000);
        assert!((v - 0.4).abs() < 1e-3);
        assert_eq!(music_gain_at(&track, 5_000_000_000, 20_000_000_000), 0.8);
        // Halfway through the 2s fade-out that ends at the media end (11s).
        let v = music_gain_at(&track, 10_000_000_000, 20_000_000_000);
        assert!((v - 0.4).abs() < 1e-3);
    }

    #[test]
    fn music_gain_fade_out_should_anchor_to_the_timeline_end_when_it_cuts_the_track() {
        let track = music(0, 60_000_000_000);
        // Timeline ends at 4s: fade-out covers 2..4s, so 3s is halfway down.
        let v = music_gain_at(&track, 3_000_000_000, 4_000_000_000);
        assert!((v - 0.4).abs() < 1e-3);
        assert_eq!(music_gain_at(&track, 4_000_000_000, 4_000_000_000), 0.0);
    }

    #[test]
    fn ripples_at_should_include_only_recent_clicks() {
        let clicks = vec![click(0, 0.1, 0.1), click(1_000_000_000, 0.5, 0.5)];
        let ripples = ripples_at(&clicks, 1_200_000_000);
        assert_eq!(ripples.len(), 1);
        assert_eq!(ripples[0].x, 0.5);
        assert!((ripples[0].progress - 0.4).abs() < 1e-3);
        assert!(ripples_at(&clicks, 2_000_000_000).is_empty());
    }
}
