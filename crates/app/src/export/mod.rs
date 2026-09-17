//! Export job: one worker thread walking the timeline, decoding source
//! frames, compositing them and pushing them into the codec's exporter.
//!
//! The UI thread only ever touches the progress channel and the cancel flag,
//! so a long export never blocks a repaint. Decoding reuses
//! `breez_codec::VideoDecoder` and seeks only when the walk is not linear, so
//! a single-clip export is one straight decode.

pub mod plan;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use breez_codec::{ExportConfig, Exporter, SequentialReader};
use breez_core::events::InputEvent;
use breez_core::layout;
use breez_core::package::RecPackage;
use breez_core::project::Project;
use breez_core::render;
use breez_core::timeline::{frame_count, frame_time_ns};
use breez_render::{Compositor, FrameParams, SourceFrame};

type JobError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    High,
    Balanced,
    Small,
}

impl Quality {
    /// x264 constant rate factor; lower is better quality.
    pub fn crf(self) -> u8 {
        match self {
            Self::High => 18,
            Self::Balanced => 23,
            Self::Small => 28,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Balanced => "Balanced",
            Self::Small => "Small",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    P1080,
    P1440,
    Source,
}

impl Preset {
    /// Short side of the output in pixels; the ratio sets the other.
    pub fn short_side(self, take_h: u32) -> u32 {
        match self {
            Self::P1080 => 1080,
            Self::P1440 => 1440,
            Self::Source => take_h.max(2),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::P1080 => "1080p",
            Self::P1440 => "1440p",
            Self::Source => "Source",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub preset: Preset,
    pub quality: Quality,
    pub dest: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    /// `Some` once the job ended: the written path, or the error message.
    pub finished: Option<Result<PathBuf, String>>,
}

pub struct ExportJob {
    rx: Receiver<Progress>,
    cancel: Arc<AtomicBool>,
    latest: Progress,
}

impl ExportJob {
    pub fn spawn(
        package: &RecPackage,
        project: &Project,
        clicks: &HashMap<u32, Vec<InputEvent>>,
        settings: Settings,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let total = frame_count(
            project.timeline.duration_ns(),
            project.primary_take().map_or(30, |take| take.fps),
        );

        let package = package.clone();
        let project = project.clone();
        let clicks = clicks.clone();
        let worker_cancel = Arc::clone(&cancel);
        let worker_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("breez-export".to_owned())
            .spawn(move || {
                let result = run(
                    &package,
                    &project,
                    &clicks,
                    &settings,
                    &worker_tx,
                    &worker_cancel,
                );
                let _ = worker_tx.send(Progress {
                    done: total,
                    total,
                    finished: Some(result.map_err(|e| e.to_string())),
                });
            });
        if spawned.is_err() {
            let _ = tx.send(Progress {
                done: 0,
                total,
                finished: Some(Err("could not start the export thread".to_owned())),
            });
        }

        Self {
            rx,
            cancel,
            latest: Progress {
                done: 0,
                total,
                finished: None,
            },
        }
    }

    /// Latest progress. Drains everything queued so a slow UI frame does not
    /// fall behind the worker.
    pub fn poll(&mut self) -> Progress {
        loop {
            match self.rx.try_recv() {
                Ok(progress) => {
                    let total = self.latest.total.max(progress.total);
                    self.latest = Progress { total, ..progress };
                    if self.latest.finished.is_some() {
                        return self.latest.clone();
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                    return self.latest.clone();
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn run(
    package: &RecPackage,
    project: &Project,
    clicks: &HashMap<u32, Vec<InputEvent>>,
    settings: &Settings,
    tx: &Sender<Progress>,
    cancel: &AtomicBool,
) -> Result<PathBuf, JobError> {
    let take = project
        .primary_take()
        .ok_or("timeline references no take")?;
    let take_id = take.id;
    let video = package.resolve(&take.video)?;
    let fps = take.fps.max(1);
    let duration = project.timeline.duration_ns();
    let total = frame_count(duration, fps);
    if total == 0 {
        return Err("timeline is empty".into());
    }

    let (width, height) = layout::output_size(
        &project.style.ratio,
        settings.preset.short_side(take.height),
    );
    let (system_audio, music) = plan::audio_sources(package, project);
    let mut exporter = Exporter::create(
        &settings.dest,
        &ExportConfig {
            width,
            height,
            fps,
            crf: settings.quality.crf(),
            system_audio,
            music,
        },
    )?;
    let mut compositor = Compositor::new(width, height);
    let mut reader = match SequentialReader::open(&video, 0) {
        Ok(reader) => reader,
        Err(e) => {
            exporter.abort();
            return Err(e.into());
        }
    };

    for index in 0..total {
        if cancel.load(Ordering::Relaxed) {
            exporter.abort();
            return Err("export cancelled".into());
        }
        // Any failure past this point must take the half-written file with
        // it: the destination is a path the user picked, not a temp file.
        match compose_frame(
            &mut exporter,
            &mut compositor,
            &mut reader,
            project,
            clicks,
            take_id,
            frame_time_ns(index, fps).min(duration.saturating_sub(1)),
        ) {
            Ok(()) => {}
            Err(e) => {
                exporter.abort();
                return Err(e);
            }
        }
        let _ = tx.send(Progress {
            done: index + 1,
            total,
            finished: None,
        });
    }
    exporter_finish(exporter, &settings.dest)?;
    Ok(settings.dest.clone())
}

/// Close the encoder, removing the half-written file if the trailer never
/// lands.
fn exporter_finish(exporter: Exporter, dest: &std::path::Path) -> Result<(), JobError> {
    exporter.finish().map_err(|e| {
        let _ = std::fs::remove_file(dest);
        JobError::from(e)
    })
}

/// Decode, composite and push the frame shown at timeline time `t_ns`.
fn compose_frame(
    exporter: &mut Exporter,
    compositor: &mut Compositor,
    reader: &mut SequentialReader,
    project: &Project,
    clicks: &HashMap<u32, Vec<InputEvent>>,
    take_id: u32,
    t_ns: u64,
) -> Result<(), JobError> {
    let clip_time = project
        .timeline
        .resolve(t_ns)
        .ok_or("timeline resolved past its own end")?;
    // One export renders one take; a clip pointing elsewhere would decode
    // from the wrong file.
    if clip_time.take != take_id {
        return Err("timeline spans more than one take, which export cannot do yet".into());
    }
    let frame = reader.frame_at(clip_time.src_ns)?;
    let take_clicks = clicks.get(&take_id).map_or(&[][..], Vec::as_slice);
    let zoom = render::zoom_at(
        &project.timeline,
        t_ns,
        render::cursor_anchor(take_clicks, clip_time.src_ns),
    );
    let ripples = render::ripples_at(take_clicks, clip_time.src_ns);
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
    Ok(())
}
