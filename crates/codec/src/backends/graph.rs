//! ffmpeg audio filter graph for export.
//!
//! Reproduces `breez_core::render::music_gain_at` (a linear fade in from the
//! track's offset and a linear fade out anchored to the end of its audible
//! span, each clamped to that span independently so they multiply where they
//! overlap) plus the system-audio gain and the timeline's trims. The exported
//! mix therefore matches what the preview played.
//!
//! Built as a pure string so it can be tested without spawning ffmpeg.
//!
//! Input order is fixed by `Exporter`: `0:v` is our rawvideo pipe, `1:a` is
//! the system audio when present, and the music tracks follow in order.

use crate::export::{AudioClip, ExportConfig, MusicSource, SystemAudio};

/// Seconds at fixed precision. Never format these with `{}`: ffmpeg parses
/// the scientific notation Rust emits for small floats inconsistently across
/// filters.
fn secs(ns: u64) -> String {
    format!("{:.6}", ns as f64 / 1e9)
}

/// The system audio input this export actually has, if any.
///
/// A `SystemAudio` with no clips selects nothing, so it contributes no input
/// at all. `spawn_export` and [`filter_graph`] must agree on that or the
/// music chains would be numbered against an input that was never opened and
/// would read the wrong file.
pub(crate) fn effective_system(config: &ExportConfig) -> Option<&SystemAudio> {
    config
        .system_audio
        .as_ref()
        .filter(|system| !system.clips.is_empty())
}

/// Build the `-filter_complex` value, or `None` when the export has no audio
/// at all. The mixed result is always labelled `[aout]`.
pub(crate) fn filter_graph(config: &ExportConfig) -> Option<String> {
    let system = effective_system(config);
    if system.is_none() && config.music.is_empty() {
        return None;
    }

    let mut chains: Vec<String> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut input = 1u32;

    if let Some(system) = system {
        chains.extend(system_chains(system, input));
        labels.push("[sys]".to_owned());
        input += 1;
    }
    for (index, track) in config.music.iter().enumerate() {
        chains.push(music_chain(track, input, index));
        labels.push(format!("[m{index}]"));
        input += 1;
    }

    let mut graph = chains.join(";");
    graph.push(';');
    if labels.len() == 1 {
        // A lone source still needs the `[aout]` label the caller maps.
        graph.push_str(&format!("{}anull[aout]", labels[0]));
    } else {
        graph.push_str(&format!(
            "{}amix=inputs={}:normalize=0:dropout_transition=0[aout]",
            labels.concat(),
            labels.len()
        ));
    }
    Some(graph)
}

/// Trim each clip out of the take's audio, concat them in timeline order,
/// then apply the system gain.
fn system_chains(system: &SystemAudio, input: u32) -> Vec<String> {
    let mut chains: Vec<String> = system
        .clips
        .iter()
        .enumerate()
        .map(|(i, clip)| format!("[{input}:a]{}[s{i}]", clip_filters(clip)))
        .collect();
    let joined: String = (0..system.clips.len()).map(|i| format!("[s{i}]")).collect();
    chains.push(format!(
        "{joined}concat=n={}:v=0:a=1[sysraw]",
        system.clips.len()
    ));
    chains.push(format!("[sysraw]volume={:.6}[sys]", system.gain));
    chains
}

fn clip_filters(clip: &AudioClip) -> String {
    let mut filters = format!(
        "atrim=start={}:end={},asetpts=PTS-STARTPTS",
        secs(clip.src_in_ns),
        secs(clip.src_out_ns)
    );
    // atempo only accepts 0.5..=2.0 per instance, so wider ratios chain.
    let mut remaining = clip.speed.clamp(0.25, 4.0);
    while (remaining - 1.0).abs() > 1e-4 {
        let step = remaining.clamp(0.5, 2.0);
        filters.push_str(&format!(",atempo={step:.6}"));
        remaining /= step;
    }
    filters
}

/// Delay the track to its offset, fade it the way `music_gain_at` does, then
/// scale by its gain.
fn music_chain(track: &MusicSource, input: u32, index: usize) -> String {
    let audible = track.audible_ns;
    // Each fade is clamped to the span on its own, exactly as
    // `music_gain_at` does; where they overlap, the two afade filters
    // multiply just as the two factors do there.
    let fade_in = track.fade_in_ns.min(audible);
    let fade_out = track.fade_out_ns.min(audible);
    let delay_ms = track.offset_ns / 1_000_000;

    let mut chain = format!(
        "[{input}:a]atrim=end={},asetpts=PTS-STARTPTS",
        secs(audible)
    );
    if delay_ms > 0 {
        chain.push_str(&format!(",adelay={delay_ms}|{delay_ms}:all=1"));
    }
    if fade_in > 0 {
        chain.push_str(&format!(
            ",afade=t=in:st={}:d={}",
            secs(track.offset_ns),
            secs(fade_in)
        ));
    }
    if fade_out > 0 {
        chain.push_str(&format!(
            ",afade=t=out:st={}:d={}",
            secs(track.offset_ns + audible - fade_out),
            secs(fade_out)
        ));
    }
    chain.push_str(&format!(",volume={:.6}[m{index}]", track.gain));
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(system: Option<SystemAudio>, music: Vec<MusicSource>) -> ExportConfig {
        ExportConfig {
            width: 1920,
            height: 1080,
            fps: 30,
            crf: 23,
            system_audio: system,
            music,
        }
    }

    fn music(offset_ns: u64, audible_ns: u64) -> MusicSource {
        MusicSource {
            path: "m.mp3".into(),
            gain: 0.5,
            offset_ns,
            fade_in_ns: 500_000_000,
            fade_out_ns: 1_000_000_000,
            audible_ns,
        }
    }

    fn clip(src_in_ns: u64, src_out_ns: u64) -> AudioClip {
        AudioClip {
            src_in_ns,
            src_out_ns,
            speed: 1.0,
        }
    }

    fn system(clips: Vec<AudioClip>) -> SystemAudio {
        SystemAudio {
            path: "a.m4a".into(),
            gain: 1.0,
            clips,
        }
    }

    #[test]
    fn graph_should_be_empty_without_any_audio() {
        assert_eq!(filter_graph(&config(None, Vec::new())), None);
    }

    #[test]
    fn effective_system_should_drop_a_source_that_selects_no_clips() {
        assert!(effective_system(&config(Some(system(Vec::new())), Vec::new())).is_none());
        assert!(effective_system(&config(Some(system(vec![clip(0, 1)])), Vec::new())).is_some());
    }

    #[test]
    fn music_should_be_numbered_against_the_inputs_that_are_actually_opened() {
        // No usable system audio means the lone music track is input 1.
        let g = filter_graph(&config(
            Some(system(Vec::new())),
            vec![music(0, 1_000_000_000)],
        ))
        .expect("graph");
        assert!(g.starts_with("[1:a]"));
    }

    #[test]
    fn graph_should_be_empty_when_the_timeline_has_no_clips() {
        assert_eq!(
            filter_graph(&config(Some(system(Vec::new())), Vec::new())),
            None
        );
    }

    #[test]
    fn graph_should_trim_and_concat_the_system_audio_clips() {
        let g = filter_graph(&config(
            Some(system(vec![
                clip(0, 2_000_000_000),
                clip(5_000_000_000, 6_000_000_000),
            ])),
            Vec::new(),
        ))
        .expect("graph");
        assert!(g.contains("[1:a]atrim=start=0.000000:end=2.000000,asetpts=PTS-STARTPTS[s0]"));
        assert!(g.contains("[1:a]atrim=start=5.000000:end=6.000000,asetpts=PTS-STARTPTS[s1]"));
        assert!(g.contains("[s0][s1]concat=n=2:v=0:a=1[sysraw]"));
    }

    #[test]
    fn graph_should_apply_the_system_gain() {
        let g = filter_graph(&config(
            Some(system(vec![clip(0, 1_000_000_000)])),
            Vec::new(),
        ))
        .expect("graph");
        assert!(g.contains("[sysraw]volume=1.000000[sys]"));
    }

    #[test]
    fn graph_should_delay_fade_and_scale_each_music_track() {
        let g =
            filter_graph(&config(None, vec![music(2_000_000_000, 10_000_000_000)])).expect("graph");
        assert!(g.contains("adelay=2000|2000:all=1"));
        assert!(g.contains("afade=t=in:st=2.000000:d=0.500000"));
        // The audible span ends at 12s, so the 1s fade-out starts at 11s.
        assert!(g.contains("afade=t=out:st=11.000000:d=1.000000"));
        assert!(g.contains("volume=0.500000"));
    }

    #[test]
    fn graph_should_clamp_each_fade_to_the_audible_span_independently() {
        // 0.4s audible holds neither the 0.5s fade-in nor the 1s fade-out, so
        // both clamp to 0.4s and overlap, matching `music_gain_at`.
        let g = filter_graph(&config(None, vec![music(0, 400_000_000)])).expect("graph");
        assert!(g.contains("afade=t=in:st=0.000000:d=0.400000"));
        assert!(g.contains("afade=t=out:st=0.000000:d=0.400000"));
    }

    #[test]
    fn graph_should_not_delay_a_track_at_offset_zero() {
        let g = filter_graph(&config(None, vec![music(0, 1_000_000_000)])).expect("graph");
        assert!(!g.contains("adelay"));
    }

    #[test]
    fn graph_should_mix_every_input_without_normalizing() {
        let g = filter_graph(&config(
            Some(system(vec![clip(0, 1_000_000_000)])),
            vec![music(0, 1_000_000_000)],
        ))
        .expect("graph");
        assert!(g.ends_with("[sys][m0]amix=inputs=2:normalize=0:dropout_transition=0[aout]"));
    }

    #[test]
    fn graph_should_skip_the_mixer_for_a_single_input() {
        let g = filter_graph(&config(None, vec![music(0, 1_000_000_000)])).expect("graph");
        assert!(g.ends_with("[m0]anull[aout]"));
        assert!(!g.contains("amix"));
    }

    #[test]
    fn graph_should_retime_a_sped_up_clip() {
        let g = filter_graph(&config(
            Some(system(vec![AudioClip {
                src_in_ns: 0,
                src_out_ns: 2_000_000_000,
                speed: 2.0,
            }])),
            Vec::new(),
        ))
        .expect("graph");
        assert!(g.contains("atempo=2.000000"));
    }

    #[test]
    fn graph_should_chain_atempo_beyond_what_one_instance_accepts() {
        let g = filter_graph(&config(
            Some(system(vec![AudioClip {
                src_in_ns: 0,
                src_out_ns: 2_000_000_000,
                speed: 4.0,
            }])),
            Vec::new(),
        ))
        .expect("graph");
        assert_eq!(g.matches("atempo=2.000000").count(), 2);
    }
}
