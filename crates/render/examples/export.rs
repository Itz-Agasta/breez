//! Export a `.rec` package to MP4 without the GUI, and report throughput.
//!
//! Lives here rather than in `breez-codec` because it needs the compositor
//! too, and `breez-render` already depends on neither.
//!
//! cargo run --release -p breez-render --example export -- <pkg.rec> <out.mp4>

use std::path::{Path, PathBuf};

use breez_codec::{AudioClip, ExportConfig, Exporter, MusicSource, SequentialReader, SystemAudio};
use breez_core::package::RecPackage;
use breez_core::project::{Project, Timeline};
use breez_core::render::{self, ZoomView, audible_span_ns};
use breez_core::timeline::{frame_count, frame_time_ns};
use breez_core::{events, layout};
use breez_render::{Compositor, FrameParams, SourceFrame};

type Error = Box<dyn std::error::Error>;

fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let (Some(pkg), Some(dest)) = (args.next(), args.next()) else {
        return Err("usage: export <pkg.rec> <out.mp4>".into());
    };
    let short_side: u32 = args.next().map_or(Ok(1080), |s| s.parse())?;

    let package = RecPackage::open(&pkg)?;
    let mut project = Project::load(&package)?;
    project.sanitize();
    let take = project.takes.first().ok_or("package has no takes")?.clone();

    let fps = take.fps.max(1);
    let duration = project.timeline.duration_ns();
    let total = frame_count(duration, fps);
    let (width, height) = layout::output_size(&project.style.ratio, short_side);
    println!(
        "{} -> {}: {width}x{height} @ {fps}fps, {total} frames ({:.2}s)",
        pkg,
        dest,
        duration as f64 / 1e9
    );

    let mut exporter = Exporter::create(
        Path::new(&dest),
        &ExportConfig {
            width,
            height,
            fps,
            crf: 23,
            system_audio: system_audio(&package, &project),
            music: music(&package, &project),
        },
    )?;
    let clicks = click_events(&package, &project);
    let mut compositor = Compositor::new(width, height);
    let video = package.resolve(&take.video)?;
    let mut reader = SequentialReader::open(&video, 0)?;

    let started = std::time::Instant::now();
    for index in 0..total {
        let t_ns = frame_time_ns(index, fps).min(duration.saturating_sub(1));
        let Some(clip_time) = project.timeline.resolve(t_ns) else {
            break;
        };
        let frame = reader.frame_at(clip_time.src_ns)?;
        let zoom: ZoomView = render::zoom_at(
            &project.timeline,
            t_ns,
            render::cursor_anchor(&clicks, clip_time.src_ns),
        );
        let ripples = render::ripples_at(&clicks, clip_time.src_ns);
        let composed = compositor.compose(
            SourceFrame {
                data: &frame.data,
                width: frame.width,
                height: frame.height,
            },
            &FrameParams {
                style: &project.style,
                zoom,
                ripples: &ripples,
                click_highlight: project.style.cursor.click_highlight,
            },
        );
        exporter.push_frame(composed)?;
    }
    exporter.finish()?;

    let elapsed = started.elapsed();
    println!(
        "done in {elapsed:.2?} ({:.2}x realtime, {:.1} fps)",
        elapsed.as_secs_f64() / (duration as f64 / 1e9),
        total as f64 / elapsed.as_secs_f64()
    );
    Ok(())
}

fn system_audio(package: &RecPackage, project: &Project) -> Option<SystemAudio> {
    let path = package
        .resolve(project.takes.first()?.audio.as_deref()?)
        .ok()?;
    let clips = clip_slices(&project.timeline, project.takes.first()?.id);
    (!clips.is_empty()).then_some(SystemAudio {
        path,
        gain: project.style.system_audio_gain,
        clips,
    })
}

fn clip_slices(timeline: &Timeline, take_id: u32) -> Vec<AudioClip> {
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

fn music(package: &RecPackage, project: &Project) -> Vec<MusicSource> {
    let duration = project.timeline.duration_ns();
    project
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
        .collect()
}

fn click_events(package: &RecPackage, project: &Project) -> Vec<events::InputEvent> {
    let Some(path) = project
        .takes
        .first()
        .and_then(|take| take.events.as_deref())
        .and_then(|rel| package.resolve(rel).ok())
        .filter(|path: &PathBuf| path.exists())
    else {
        return Vec::new();
    };
    events::read_log(&path)
        .map(|events| {
            events
                .into_iter()
                .filter(|e| e.kind == events::InputKind::Down)
                .collect()
        })
        .unwrap_or_default()
}
