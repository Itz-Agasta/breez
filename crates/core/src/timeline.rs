//! Timeline playback math: mapping a timeline position to a clip's source
//! time. Clips play back to back in `timeline.clips` order; a clip covers
//! `(src_out_ns - src_in_ns) / speed` of timeline time.

use crate::project::Timeline;

/// A timeline position resolved into a source position within one clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipTime {
    /// Index into `timeline.clips`.
    pub clip: usize,
    /// Take id the clip references.
    pub take: u32,
    /// Position within the take's media, in source nanoseconds.
    pub src_ns: u64,
}

impl Timeline {
    /// Total timeline duration in nanoseconds.
    pub fn duration_ns(&self) -> u64 {
        self.clips.iter().map(clip_len_ns).sum()
    }

    /// Timeline time at which clip `index` starts.
    pub fn clip_start_ns(&self, index: usize) -> u64 {
        self.clips.iter().take(index).map(clip_len_ns).sum()
    }

    /// Timeline time clip `index` occupies, which is its source span scaled
    /// by its speed. Drawing or hit-testing a clip against its raw source
    /// span misplaces every clip after it.
    pub fn clip_len_ns(&self, index: usize) -> u64 {
        self.clips.get(index).map_or(0, clip_len_ns)
    }

    /// Inverse of [`Timeline::resolve`] for one take: the timeline time at
    /// which source position `src_ns` of `take` is shown, if any clip covers
    /// it (the first one wins when several do).
    pub fn timeline_ns_for(&self, take: u32, src_ns: u64) -> Option<u64> {
        let mut start = 0u64;
        for clip in &self.clips {
            if clip.take == take && (clip.src_in_ns..clip.src_out_ns).contains(&src_ns) {
                let offset = ((src_ns - clip.src_in_ns) as f64
                    / f64::from(clip.speed.max(f32::EPSILON))) as u64;
                return Some(start + offset);
            }
            start += clip_len_ns(clip);
        }
        None
    }

    /// Resolve timeline time `t_ns` to a clip and source position. Returns
    /// `None` for `t_ns >= duration` (callers clamp before resolving the
    /// exact end).
    pub fn resolve(&self, t_ns: u64) -> Option<ClipTime> {
        let mut start = 0u64;
        for (index, clip) in self.clips.iter().enumerate() {
            let len = clip_len_ns(clip);
            if t_ns < start + len {
                let src_offset = ((t_ns - start) as f64 * f64::from(clip.speed)) as u64;
                return Some(ClipTime {
                    clip: index,
                    take: clip.take,
                    src_ns: (clip.src_in_ns + src_offset).min(clip.src_out_ns),
                });
            }
            start += len;
        }
        None
    }
}

/// Output frames needed to cover `duration_ns` at `fps`, rounding a partial
/// final frame up so the last moment is not cut off.
///
/// Multiplying before dividing matters: a nanoseconds-per-frame value
/// truncates (1e9/30 is 33_333_333, not 33_333_333.33), and dividing a whole
/// duration by the short value rounds up into a frame the source does not
/// have.
pub fn frame_count(duration_ns: u64, fps: u32) -> u64 {
    if duration_ns == 0 || fps == 0 {
        return 0;
    }
    (duration_ns.saturating_mul(u64::from(fps))).div_ceil(1_000_000_000)
}

/// Timeline time sampled by output frame `index`.
pub fn frame_time_ns(index: u64, fps: u32) -> u64 {
    index.saturating_mul(1_000_000_000) / u64::from(fps.max(1))
}

fn clip_len_ns(clip: &crate::project::Clip) -> u64 {
    let src = clip.src_out_ns.saturating_sub(clip.src_in_ns);
    (src as f64 / f64::from(clip.speed.max(f32::EPSILON))) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Clip;

    fn clip(take: u32, src_in_ns: u64, src_out_ns: u64) -> Clip {
        Clip {
            take,
            src_in_ns,
            src_out_ns,
            speed: 1.0,
        }
    }

    #[test]
    fn resolve_should_map_across_clip_boundaries() {
        let timeline = Timeline {
            clips: vec![clip(0, 1_000, 5_000), clip(1, 0, 2_000)],
            ..Timeline::default()
        };
        assert_eq!(timeline.duration_ns(), 6_000);
        assert_eq!(
            timeline.resolve(0),
            Some(ClipTime {
                clip: 0,
                take: 0,
                src_ns: 1_000
            })
        );
        assert_eq!(
            timeline.resolve(4_500),
            Some(ClipTime {
                clip: 1,
                take: 1,
                src_ns: 500
            })
        );
        assert_eq!(timeline.resolve(6_000), None);
    }

    #[test]
    fn timeline_ns_for_should_invert_resolve_within_a_clip() {
        let timeline = Timeline {
            clips: vec![clip(0, 1_000, 5_000), clip(1, 0, 2_000)],
            ..Timeline::default()
        };
        assert_eq!(timeline.timeline_ns_for(0, 1_500), Some(500));
        assert_eq!(timeline.timeline_ns_for(1, 500), Some(4_500));
        // Trimmed away or unknown source positions map to nothing.
        assert_eq!(timeline.timeline_ns_for(0, 500), None);
        assert_eq!(timeline.timeline_ns_for(7, 0), None);
    }

    #[test]
    fn frame_count_should_not_exceed_the_frames_the_source_has() {
        // A 10s 30fps take holds frames 0..=299. Asking for 301 walks the
        // decoder off the end and fails the whole export.
        assert_eq!(frame_count(10_000_000_000, 30), 300);
        assert_eq!(frame_count(10_000_000_000, 60), 600);
    }

    #[test]
    fn frame_count_should_round_up_a_partial_final_frame() {
        // 2.02s at 30fps is 60.6 frames.
        assert_eq!(frame_count(2_020_000_000, 30), 61);
    }

    #[test]
    fn frame_count_should_be_zero_for_an_empty_timeline() {
        assert_eq!(frame_count(0, 30), 0);
    }

    #[test]
    fn every_frame_time_should_land_inside_the_duration() {
        for fps in [24, 25, 30, 50, 60] {
            for duration_ns in [1_000_000_000, 10_000_000_000, 8_333_333_333] {
                let count = frame_count(duration_ns, fps);
                assert!(
                    frame_time_ns(count - 1, fps) < duration_ns,
                    "last frame of {duration_ns}ns at {fps}fps falls outside it"
                );
            }
        }
    }

    #[test]
    fn frame_time_should_land_on_exact_frame_boundaries() {
        assert_eq!(frame_time_ns(0, 30), 0);
        assert_eq!(frame_time_ns(30, 30), 1_000_000_000);
    }

    #[test]
    fn clip_len_should_scale_with_speed() {
        let timeline = Timeline {
            clips: vec![Clip {
                take: 0,
                src_in_ns: 0,
                src_out_ns: 4_000,
                speed: 2.0,
            }],
            ..Timeline::default()
        };
        // 4us of source at 2x occupies 2us of timeline.
        assert_eq!(timeline.clip_len_ns(0), 2_000);
        assert_eq!(timeline.clip_len_ns(9), 0);
    }

    #[test]
    fn clip_start_should_accumulate_previous_lengths() {
        let timeline = Timeline {
            clips: vec![clip(0, 0, 3_000), clip(0, 1_000, 2_000)],
            ..Timeline::default()
        };
        assert_eq!(timeline.clip_start_ns(0), 0);
        assert_eq!(timeline.clip_start_ns(1), 3_000);
    }
}
