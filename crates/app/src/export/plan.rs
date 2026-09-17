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
use breez_core::project::{Project, Timeline};
use breez_core::render::audible_span_ns;

/// The slices of `take`'s audio that the timeline's clips select. Clips from
/// other takes are skipped: one export carries one system-audio input, so
/// slicing take A's audio with take B's trims would be worse than silence.
pub fn system_clips(timeline: &Timeline, take_id: u32) -> Vec<AudioClip> {
    timeline
        .clips
        .iter()
        .filter(|clip| clip.take == take_id)
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
    let take = project.primary_take();
    let system = take
        .and_then(|take| Some((take.id, take.audio.as_deref()?)))
        .and_then(|(id, rel)| Some((id, package.resolve(rel).ok()?)))
        .map(|(id, path)| SystemAudio {
            path,
            gain: project.style.system_audio_gain,
            clips: system_clips(&project.timeline, id),
        })
        .filter(|system| !system.clips.is_empty());

    let music = project
        .timeline
        .music
        .iter()
        .filter_map(|track| {
            let audible = audible_span_ns(track, duration);
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
    use breez_core::project::{Clip, MusicTrack, Take, Timeline};

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

    fn clip(take: u32, src_in_ns: u64, src_out_ns: u64) -> Clip {
        Clip {
            take,
            src_in_ns,
            src_out_ns,
            speed: 1.0,
        }
    }

    #[test]
    fn system_clips_should_carry_each_clip_trim_across() {
        let timeline = Timeline {
            clips: vec![clip(0, 1_000_000_000, 3_000_000_000)],
            ..Timeline::default()
        };
        let clips = system_clips(&timeline, 0);
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].src_in_ns, 1_000_000_000);
        assert_eq!(clips[0].src_out_ns, 3_000_000_000);
    }

    #[test]
    fn system_clips_should_skip_clips_from_another_take() {
        let timeline = Timeline {
            clips: vec![
                clip(0, 0, 1_000_000_000),
                clip(1, 4_000_000_000, 5_000_000_000),
            ],
            ..Timeline::default()
        };
        assert_eq!(system_clips(&timeline, 0).len(), 1);
        assert_eq!(system_clips(&timeline, 1)[0].src_in_ns, 4_000_000_000);
    }

    #[test]
    fn primary_take_should_follow_the_first_clip_not_the_first_take() {
        let mut project = Project::new("t");
        project.takes = vec![
            Take {
                id: 0,
                video: "a".to_owned(),
                audio: None,
                events: None,
                width: 1280,
                height: 720,
                fps: 30,
                duration_ns: 1_000_000_000,
            },
            Take {
                id: 1,
                video: "b".to_owned(),
                audio: None,
                events: None,
                width: 1920,
                height: 1080,
                fps: 60,
                duration_ns: 1_000_000_000,
            },
        ];
        project.timeline.clips = vec![clip(1, 0, 1_000_000_000)];
        assert_eq!(project.primary_take().map(|t| t.id), Some(1));
    }

    #[test]
    fn audible_should_stop_at_the_timeline_end() {
        assert_eq!(
            audible_span_ns(&track(1_000_000_000, 30_000_000_000), 5_000_000_000),
            4_000_000_000
        );
    }

    #[test]
    fn audible_should_stop_at_the_media_end() {
        assert_eq!(
            audible_span_ns(&track(0, 3_000_000_000), 10_000_000_000),
            3_000_000_000
        );
    }

    #[test]
    fn audible_should_span_to_the_timeline_end_when_the_media_length_is_unknown() {
        assert_eq!(
            audible_span_ns(&track(1_000_000_000, 0), 10_000_000_000),
            9_000_000_000
        );
    }

    #[test]
    fn audible_should_be_zero_for_a_track_past_the_end() {
        assert_eq!(
            audible_span_ns(&track(20_000_000_000, 3_000_000_000), 10_000_000_000),
            0
        );
    }
}
