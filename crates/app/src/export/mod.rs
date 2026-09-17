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

use breez_codec::{ExportConfig, Exporter, VideoDecoder, VideoFrame};
use breez_core::events::InputEvent;
use breez_core::layout;
use breez_core::package::RecPackage;
use breez_core::project::Project;
use breez_core::render;
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
        let total = plan::frame_count(
            project.timeline.duration_ns(),
            project.takes.first().map_or(30, |take| take.fps),
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
    let take = project.takes.first().ok_or("project has no takes")?;
    let fps = take.fps.max(1);
    let duration = project.timeline.duration_ns();
    let total = plan::frame_count(duration, fps);
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
    let mut decoder: Option<TakeDecoder> = None;

    for index in 0..total {
        if cancel.load(Ordering::Relaxed) {
            exporter.abort();
            return Err("export cancelled".into());
        }
        let t_ns = plan::frame_time_ns(index, fps).min(duration.saturating_sub(1));
        let Some(clip_time) = project.timeline.resolve(t_ns) else {
            break;
        };
        let frame = decode_at(
            package,
            project,
            &mut decoder,
            clip_time.take,
            clip_time.src_ns,
        )?;
        let take_clicks = clicks.get(&clip_time.take).map_or(&[][..], Vec::as_slice);
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
        let _ = tx.send(Progress {
            done: index + 1,
            total,
            finished: None,
        });
    }
    exporter.finish()?;
    Ok(settings.dest.clone())
}

/// The open decoder plus the source time it has already walked past, so a
/// linear export never seeks.
struct TakeDecoder {
    take: u32,
    decoder: VideoDecoder,
    last_pts_ns: u64,
}

/// Source frame at or after `src_ns`, reusing the open decoder while the walk
/// stays forward. A backwards jump (a new clip trimming to an earlier point)
/// seeks; export within one clip is monotonic, so that is rare.
fn decode_at(
    package: &RecPackage,
    project: &Project,
    slot: &mut Option<TakeDecoder>,
    take_id: u32,
    src_ns: u64,
) -> Result<VideoFrame, JobError> {
    let stale = match slot {
        Some(open) => open.take != take_id || open.last_pts_ns > src_ns,
        None => true,
    };
    if stale {
        match slot {
            // Same take, just rewound: seek rather than respawn.
            Some(open) if open.take == take_id => {
                open.decoder.seek(src_ns)?;
                open.last_pts_ns = 0;
            }
            _ => {
                let take = project
                    .takes
                    .iter()
                    .find(|take| take.id == take_id)
                    .ok_or("clip references a missing take")?;
                let path = package.resolve(&take.video)?;
                *slot = Some(TakeDecoder {
                    take: take_id,
                    decoder: VideoDecoder::open(&path, src_ns)?,
                    last_pts_ns: 0,
                });
            }
        }
    }

    let open = slot.as_mut().ok_or("decoder unavailable")?;
    loop {
        match open.decoder.next_frame()? {
            Some(frame) => {
                open.last_pts_ns = frame.pts_ns;
                if frame.pts_ns + 1 >= src_ns {
                    return Ok(frame);
                }
            }
            None => return Err("source ended before the timeline did".into()),
        }
    }
}
