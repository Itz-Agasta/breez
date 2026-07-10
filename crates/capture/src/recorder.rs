//! Recording pipeline: one pinray session in, one `.rec` take out.
//!
//! The capture thread drains `next_event` and feeds two ffmpeg-backed
//! encoders. Video is paced to constant frame rate by duplicating the last
//! frame into missed slots (this also absorbs Gap events); audio gaps are
//! filled with silence based on packet timestamps. Encoders are created
//! lazily from the first frame of each stream, since width/height/sample
//! rate are only known then.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use breez_codec::{
    AudioEncoder, AudioEncoderConfig, CodecError, PixelFormat, VideoEncoder, VideoEncoderConfig,
};
use breez_core::package::{PackageError, RecPackage};
use breez_core::project::{Clip, Project, Take};
use pinray::{
    AudioCapture, CaptureEvent, CaptureSession, PinrayError, SourceId, VideoCaptureTarget,
};

use crate::input::InputLogger;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture: {0}")]
    Pinray(String),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("unsupported stream: {0}")]
    Unsupported(String),
    #[error("capture thread panicked")]
    ThreadPanic,
}

#[derive(Debug, Clone)]
pub struct RecordConfig {
    pub fps: u32,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self { fps: 60 }
    }
}

#[derive(Debug, Clone)]
pub struct TakeSummary {
    pub take_id: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_ns: u64,
    pub video_frames: u64,
    pub audio_packets: u64,
}

pub struct Recorder {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<TakeSummary, CaptureError>>,
}

impl Recorder {
    /// Create (or open) the package at `path` and start recording the
    /// primary display plus system audio into a new take.
    pub fn start(path: impl Into<PathBuf>, config: RecordConfig) -> Result<Self, CaptureError> {
        let path = path.into();
        let package = if path.join("manifest.json").exists() {
            RecPackage::open(&path)?
        } else {
            RecPackage::create(&path)?
        };
        package.set_recording(true)?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let result = run_capture(&package, &config, &stop_flag);
            // Clear the recovery flag only after a clean finish.
            if result.is_ok() {
                package.set_recording(false)?;
            }
            result
        });
        Ok(Self { stop, thread })
    }

    /// True once the capture thread has exited on its own (backend end or
    /// error). `stop` then returns without blocking.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Signal the capture thread and wait for the finished take.
    pub fn stop(self) -> Result<TakeSummary, CaptureError> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().map_err(|_| CaptureError::ThreadPanic)?
    }
}

fn run_capture(
    package: &RecPackage,
    config: &RecordConfig,
    stop: &AtomicBool,
) -> Result<TakeSummary, CaptureError> {
    let mut project = if package.project_path().exists() {
        Project::load(package)?
    } else {
        let name = package
            .root()
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned());
        Project::new(name)
    };
    let take_id = project.takes.len() as u32;

    let mut session = CaptureSession::builder()
        .video_target(VideoCaptureTarget::Display(SourceId::new("auto")))
        .audio(AudioCapture::SystemMix)
        .frame_rate(Some(config.fps))
        .build()
        .map_err(|e| CaptureError::Pinray(e.to_string()))?;
    log::info!("capture backend: {:?}", session.backend_info().kind);
    session
        .start()
        .map_err(|e| CaptureError::Pinray(e.to_string()))?;

    let anchor: Arc<OnceLock<Instant>> = Arc::new(OnceLock::new());
    let display_size: Arc<OnceLock<(u32, u32)>> = Arc::new(OnceLock::new());
    let events_rel = RecPackage::events_rel(take_id);
    let input_logger = InputLogger::start(
        package.resolve(&events_rel),
        Arc::clone(&anchor),
        Arc::clone(&display_size),
    );

    let mut video = VideoStream::new(package, take_id, config.fps);
    let mut audio = AudioStream::new(package, take_id);

    let capture_result = (|| -> Result<(), CaptureError> {
        while !stop.load(Ordering::Relaxed) {
            match session.next_event(Some(Duration::from_millis(100))) {
                Ok(CaptureEvent::Video(frame)) => {
                    video.push(&frame)?;
                    anchor.get_or_init(Instant::now);
                    display_size.get_or_init(|| (frame.width, frame.height));
                }
                Ok(CaptureEvent::Audio(frame)) => audio.push(&frame)?,
                Ok(CaptureEvent::Gap(gap)) => {
                    // Video slots refill from the last frame automatically;
                    // audio refills with silence on the next packet.
                    log::warn!(
                        "capture gap: {:?} (dropped: {:?})",
                        gap.reason,
                        gap.dropped_frames
                    );
                }
                Ok(CaptureEvent::End) => break,
                Err(PinrayError::Timeout(_)) => {}
                Err(e) => return Err(CaptureError::Pinray(e.to_string())),
            }
        }
        Ok(())
    })();

    if let Some(logger) = &input_logger {
        logger.stop();
    }
    let _ = session.stop();
    capture_result?;

    let audio_packets = audio.packets;
    let (video_summary, video_rel) = video.finish()?;
    let audio_rel = audio.finish()?;

    let duration_ns = video_summary.frames * 1_000_000_000 / config.fps as u64;
    project.takes.push(Take {
        id: take_id,
        video: video_rel,
        audio: audio_rel,
        events: input_logger.map(|_| events_rel),
        width: video_summary.width,
        height: video_summary.height,
        fps: config.fps,
        duration_ns,
    });
    if project.timeline.clips.is_empty() {
        project.timeline.clips.push(Clip {
            take: take_id,
            src_in_ns: 0,
            src_out_ns: duration_ns,
            speed: 1.0,
        });
    }
    project.save(package)?;

    Ok(TakeSummary {
        take_id,
        width: video_summary.width,
        height: video_summary.height,
        fps: config.fps,
        duration_ns,
        video_frames: video_summary.frames,
        audio_packets,
    })
}

struct VideoSummary {
    width: u32,
    height: u32,
    frames: u64,
}

/// CFR pacing state. Slot N is the output frame at `start + N/fps`; incoming
/// frames land in the nearest slot, missed slots repeat the previous frame,
/// and a frame landing in an already-written slot replaces nothing (dropped).
struct VideoStream<'a> {
    package: &'a RecPackage,
    take_id: u32,
    fps: u32,
    encoder: Option<VideoEncoder>,
    width: u32,
    height: u32,
    start_ns: i64,
    next_slot: u64,
    last_frame: Vec<u8>,
    frames: u64,
}

impl<'a> VideoStream<'a> {
    fn new(package: &'a RecPackage, take_id: u32, fps: u32) -> Self {
        Self {
            package,
            take_id,
            fps,
            encoder: None,
            width: 0,
            height: 0,
            start_ns: 0,
            next_slot: 0,
            last_frame: Vec::new(),
            frames: 0,
        }
    }

    fn push(&mut self, frame: &pinray::VideoFrame) -> Result<(), CaptureError> {
        let Some(bytes) = frame.to_tight_bytes() else {
            return Err(CaptureError::Unsupported(format!(
                "non-host or non-RGBA frame data ({:?})",
                frame.pixel_format
            )));
        };
        if self.encoder.is_none() {
            let pixel_format = match frame.pixel_format {
                pinray::PixelFormat::Bgra8888 => PixelFormat::Bgra,
                pinray::PixelFormat::Rgba8888 => PixelFormat::Rgba,
                other => {
                    return Err(CaptureError::Unsupported(format!("pixel format {other:?}")));
                }
            };
            let config = VideoEncoderConfig {
                width: frame.width,
                height: frame.height,
                fps: self.fps,
                pixel_format,
            };
            let dest = self.package.resolve(&RecPackage::video_rel(self.take_id));
            self.encoder = Some(VideoEncoder::create(&dest, &config)?);
            self.width = frame.width;
            self.height = frame.height;
            self.start_ns = frame.stream_time_ns;
        }
        if frame.width != self.width || frame.height != self.height {
            return Err(CaptureError::Unsupported(format!(
                "resolution changed mid-take: {}x{} -> {}x{}",
                self.width, self.height, frame.width, frame.height
            )));
        }

        let elapsed = (frame.stream_time_ns - self.start_ns).max(0) as f64;
        let slot = (elapsed * self.fps as f64 / 1e9).round() as u64;
        if slot < self.next_slot {
            // Faster than fps: keep the newest pixels for future duplicates.
            self.last_frame = bytes;
            return Ok(());
        }
        let encoder = self.encoder.as_mut().expect("initialized above");
        while self.next_slot < slot {
            encoder.push_frame(&self.last_frame)?;
            self.frames += 1;
            self.next_slot += 1;
        }
        encoder.push_frame(&bytes)?;
        self.frames += 1;
        self.next_slot = slot + 1;
        self.last_frame = bytes;
        Ok(())
    }

    fn finish(self) -> Result<(VideoSummary, String), CaptureError> {
        let Some(encoder) = self.encoder else {
            return Err(CaptureError::Unsupported(
                "no video frames captured".to_owned(),
            ));
        };
        encoder.finish()?;
        Ok((
            VideoSummary {
                width: self.width,
                height: self.height,
                frames: self.frames,
            },
            RecPackage::video_rel(self.take_id),
        ))
    }
}

/// Audio pass-through with silence insertion: if a packet starts later than
/// the previous one ended (beyond a small jitter allowance), the hole is
/// filled with zero samples so A/V sync survives dropouts.
struct AudioStream<'a> {
    package: &'a RecPackage,
    take_id: u32,
    encoder: Option<AudioEncoder>,
    sample_rate: u32,
    channels: u16,
    expected_ns: Option<i64>,
    packets: u64,
}

const AUDIO_GAP_TOLERANCE_NS: i64 = 20_000_000;

impl<'a> AudioStream<'a> {
    fn new(package: &'a RecPackage, take_id: u32) -> Self {
        Self {
            package,
            take_id,
            encoder: None,
            sample_rate: 0,
            channels: 0,
            expected_ns: None,
            packets: 0,
        }
    }

    fn push(&mut self, frame: &pinray::AudioFrame) -> Result<(), CaptureError> {
        if frame.sample_format != pinray::SampleFormat::F32 {
            return Err(CaptureError::Unsupported(format!(
                "audio sample format {:?}",
                frame.sample_format
            )));
        }
        let bytes = match &frame.data {
            pinray::AudioData::Interleaved(b) => b.clone(),
            pinray::AudioData::Planar(planes) => interleave_f32(planes),
        };
        if self.encoder.is_none() {
            let config = AudioEncoderConfig {
                sample_rate: frame.sample_rate,
                channels: frame.channels,
            };
            let dest = self.package.resolve(&RecPackage::audio_rel(self.take_id));
            self.encoder = Some(AudioEncoder::create(&dest, &config)?);
            self.sample_rate = frame.sample_rate;
            self.channels = frame.channels;
        }
        let encoder = self.encoder.as_mut().expect("initialized above");

        let bytes_per_second = self.sample_rate as i64 * self.channels as i64 * 4;
        if let Some(expected) = self.expected_ns {
            let gap_ns = frame.stream_time_ns - expected;
            if gap_ns > AUDIO_GAP_TOLERANCE_NS {
                let mut silence_bytes =
                    (gap_ns as i128 * bytes_per_second as i128 / 1_000_000_000) as usize;
                silence_bytes -= silence_bytes % (self.channels as usize * 4);
                encoder.push_samples(&vec![0u8; silence_bytes])?;
            }
        }
        encoder.push_samples(&bytes)?;
        self.packets += 1;

        let samples = bytes.len() as i64 / (self.channels as i64 * 4);
        let duration_ns = samples * 1_000_000_000 / self.sample_rate as i64;
        self.expected_ns = Some(frame.stream_time_ns + duration_ns);
        Ok(())
    }

    fn finish(self) -> Result<Option<String>, CaptureError> {
        match self.encoder {
            Some(encoder) => {
                encoder.finish()?;
                Ok(Some(RecPackage::audio_rel(self.take_id)))
            }
            None => Ok(None),
        }
    }
}

fn interleave_f32(planes: &[Vec<u8>]) -> Vec<u8> {
    let channels = planes.len();
    let plane_len = planes.first().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(plane_len * channels);
    let mut offset = 0;
    while offset + 4 <= plane_len {
        for plane in planes {
            out.extend_from_slice(&plane[offset..offset + 4]);
        }
        offset += 4;
    }
    out
}
