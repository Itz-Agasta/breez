//! Turning a project into an export plan: how many output frames there are,
//! which timeline instant each one samples, and the audio sources handed to
//! the codec.
//!
//! `audible_ns` here must span exactly what `render::music_gain_at` fades
//! over, which is why both read the same `min(offset + media, timeline end)`
//! rule, including its "media length unknown means span to the timeline end"
//! case.

use breez_codec::{AudioClip, MusicSource, SystemAudio};
use breez_core::package::RecPackage;
use breez_core::project::{MusicTrack, Project, Timeline};

/// Output frames needed to cover `timeline_duration_ns` at `fps`, rounding a
/// partial final frame up so the last moment is not cut off.
pub fn frame_count(timeline_duration_ns: u64, fps: u32) -> u64 {
    if timeline_duration_ns == 0 || fps == 0 {
        return 0;
    }
    let per_frame = (1_000_000_000u64 / u64::from(fps)).max(1);
    timeline_duration_ns.div_ceil(per_frame)
}

/// Timeline time sampled by output frame `index`.
pub fn frame_time_ns(index: u64, fps: u32) -> u64 {
    index * 1_000_000_000 / u64::from(fps.max(1))
}

/// Length of a track's audible span, measured from its offset. Mirrors the
/// span `music_gain_at` uses: a track whose media length is not known yet
/// (`duration_ns == 0`) runs to the timeline end.
pub fn audible_ns(track: &MusicTrack, timeline_duration_ns: u64) -> u64 {
    let mut end = timeline_duration_ns;
    if track.duration_ns > 0 {
        end = end.min(track.offset_ns.saturating_add(track.duration_ns));
    }
    end.saturating_sub(track.offset_ns)
}

/// The take-audio slices the timeline's clips select.
pub fn system_clips(timeline: &Timeline) -> Vec<AudioClip> {
    timeline
        .clips
        .iter()
        .map(|clip| AudioClip {
            src_in_ns: clip.src_in_ns,
            src_out_ns: clip.src_out_ns,
            speed: clip.speed,
        })
        .collect()
}

/// Resolve every audio input for the export. Tracks that fall past the
/// timeline end are dropped: they contribute nothing the preview played.
pub fn audio_sources(
    package: &RecPackage,
    project: &Project,
) -> (Option<SystemAudio>, Vec<MusicSource>) {
    let duration = project.timeline.duration_ns();
    let system = project
        .takes
        .first()
        .and_then(|take| take.audio.as_deref())
        .and_then(|rel| package.resolve(rel).ok())
        .map(|path| SystemAudio {
            path,
            gain: project.style.system_audio_gain,
            clips: system_clips(&project.timeline),
        })
        .filter(|system| !system.clips.is_empty());

    let music = project
        .timeline
        .music
        .iter()
        .filter_map(|track| {
            let audible = audible_ns(track, duration);
            if audible == 0 {
                return None;
            }
            Some(MusicSource {
                path: package.resolve(&track.file).ok()?,
                gain: track.gain,
                offset_ns: track.offset_ns,
                fade_in_ns: track.fade_in_ns,
                fade_out_ns: track.fade_out_ns,
                audible_ns: audible,
            })
        })
        .collect();
    (system, music)
}

#[cfg(test)]
mod tests {
    use super::*;
    use breez_core::project::{Clip, Timeline};

    fn track(offset_ns: u64, duration_ns: u64) -> MusicTrack {
        MusicTrack {
            file: "media/music/a.mp3".to_owned(),
            offset_ns,
            gain: 0.5,
            fade_in_ns: 0,
            fade_out_ns: 0,
            duration_ns,
        }
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
    fn frame_time_should_land_on_exact_frame_boundaries() {
        assert_eq!(frame_time_ns(0, 30), 0);
        assert_eq!(frame_time_ns(30, 30), 1_000_000_000);
    }

    #[test]
    fn system_clips_should_carry_each_clip_trim_across() {
        let timeline = Timeline {
            clips: vec![Clip {
                take: 0,
                src_in_ns: 1_000_000_000,
                src_out_ns: 3_000_000_000,
                speed: 1.0,
            }],
            ..Timeline::default()
        };
        let clips = system_clips(&timeline);
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].src_in_ns, 1_000_000_000);
        assert_eq!(clips[0].src_out_ns, 3_000_000_000);
    }

    #[test]
    fn audible_should_stop_at_the_timeline_end() {
        assert_eq!(
            audible_ns(&track(1_000_000_000, 30_000_000_000), 5_000_000_000),
            4_000_000_000
        );
    }

    #[test]
    fn audible_should_stop_at_the_media_end() {
        assert_eq!(
            audible_ns(&track(0, 3_000_000_000), 10_000_000_000),
            3_000_000_000
        );
    }

    #[test]
    fn audible_should_span_to_the_timeline_end_when_the_media_length_is_unknown() {
        assert_eq!(
            audible_ns(&track(1_000_000_000, 0), 10_000_000_000),
            9_000_000_000
        );
    }

    #[test]
    fn audible_should_be_zero_for_a_track_past_the_end() {
        assert_eq!(
            audible_ns(&track(20_000_000_000, 3_000_000_000), 10_000_000_000),
            0
        );
    }
}
