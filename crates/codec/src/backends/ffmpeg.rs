//! ffmpeg-sidecar backend: the only module allowed to import ffmpeg types.
//!
//! Encoders are ffmpeg child processes fed rawvideo/pcm over stdin, writing
//! fragmented MP4 (`frag_keyframe+empty_moov`) so a crash loses at most the
//! last fragment. Keyframe every second (`-g fps`) keeps scrubbing cheap.
//! Decoders are the reverse: `-ss` input seek, rawvideo RGBA on stdout,
//! frames parsed by the sidecar's event iterator (which needs ffmpeg's
//! default loglevel to read stream metadata, so decode never lowers it).

use std::io::Write;
use std::path::Path;
use std::process::ChildStdin;
use std::sync::Arc;

use ffmpeg_sidecar::child::FfmpegChild;
use ffmpeg_sidecar::command::FfmpegCommand;
use ffmpeg_sidecar::event::{FfmpegEvent, LogLevel};
use ffmpeg_sidecar::iter::FfmpegIterator;

use crate::CodecError;
use crate::decoder::VideoFrame;
use crate::encoder::{AudioEncoderConfig, PixelFormat, VideoEncoderConfig};

pub(crate) fn ensure_available() -> Result<(), CodecError> {
    ffmpeg_sidecar::download::auto_download().map_err(|e| CodecError::Backend(e.to_string()))
}

pub(crate) struct FfmpegSink {
    child: FfmpegChild,
    stdin: Option<ChildStdin>,
}

impl FfmpegSink {
    pub(crate) fn spawn_video(
        dest: &Path,
        config: &VideoEncoderConfig,
    ) -> Result<Self, CodecError> {
        let pix_fmt = match config.pixel_format {
            PixelFormat::Bgra => "bgra",
            PixelFormat::Rgba => "rgba",
        };
        let mut cmd = FfmpegCommand::new();
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            pix_fmt,
            "-s",
            &format!("{}x{}", config.width, config.height),
            "-r",
            &config.fps.to_string(),
            "-i",
            "pipe:0",
            // yuv420p needs even dimensions; crop a stray odd row/column.
            "-vf",
            "crop=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-g",
            &config.fps.to_string(),
            "-movflags",
            "+frag_keyframe+empty_moov",
            "-y",
        ]);
        cmd.arg(dest);
        Self::spawn(cmd)
    }

    pub(crate) fn spawn_audio(
        dest: &Path,
        config: &AudioEncoderConfig,
    ) -> Result<Self, CodecError> {
        let mut cmd = FfmpegCommand::new();
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "f32le",
            "-ar",
            &config.sample_rate.to_string(),
            "-ac",
            &config.channels.to_string(),
            "-i",
            "pipe:0",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+frag_keyframe+empty_moov",
            "-y",
        ]);
        cmd.arg(dest);
        Self::spawn(cmd)
    }

    fn spawn(mut cmd: FfmpegCommand) -> Result<Self, CodecError> {
        let mut child = cmd.spawn()?;
        let stdin = child
            .take_stdin()
            .ok_or_else(|| CodecError::Backend("ffmpeg stdin unavailable".to_owned()))?;
        Ok(Self {
            child,
            stdin: Some(stdin),
        })
    }

    pub(crate) fn write(&mut self, data: &[u8]) -> Result<(), CodecError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| CodecError::Backend("encoder already finished".to_owned()))?;
        stdin.write_all(data)?;
        Ok(())
    }

    /// Close stdin so ffmpeg flushes the trailer, then wait and check the
    /// exit status.
    pub(crate) fn finish(mut self) -> Result<(), CodecError> {
        drop(self.stdin.take());
        let status = self.child.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(CodecError::Backend(format!("ffmpeg exited with {status}")))
        }
    }
}

/// Reap the child on error paths so dropped encoders never leave zombie
/// ffmpeg processes. After a clean `finish` the process is already waited,
/// so kill/wait are harmless no-ops.
impl Drop for FfmpegSink {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One decode process streaming RGBA frames from `start_ns` onward. Seeking
/// means dropping this and spawning a new one (`-ss` input seek lands on a
/// keyframe then decodes accurately to the target; capture encodes a
/// keyframe every second, so respawn stays cheap).
pub(crate) struct FfmpegFrameSource {
    child: FfmpegChild,
    events: FfmpegIterator,
    start_ns: u64,
    yielded_any: bool,
}

impl FfmpegFrameSource {
    pub(crate) fn spawn(path: &Path, start_ns: u64) -> Result<Self, CodecError> {
        let mut cmd = FfmpegCommand::new();
        cmd.args(["-ss", &format!("{:.6}", start_ns as f64 / 1e9), "-i"]);
        cmd.arg(path);
        cmd.args(["-an", "-f", "rawvideo", "-pix_fmt", "rgba", "-"]);
        let mut child = cmd.spawn()?;
        let events = child
            .iter()
            .map_err(|e| CodecError::Backend(e.to_string()))?;
        Ok(Self {
            child,
            events,
            start_ns,
            yielded_any: false,
        })
    }

    /// Next decoded frame, or `None` at end of stream. Errors before the
    /// first frame surface as `Err`; errors after that end the stream (the
    /// frames already decoded are valid).
    pub(crate) fn next_frame(&mut self) -> Result<Option<VideoFrame>, CodecError> {
        let mut last_error = None;
        for event in self.events.by_ref() {
            match event {
                FfmpegEvent::OutputFrame(frame) => {
                    self.yielded_any = true;
                    return Ok(Some(VideoFrame {
                        data: Arc::from(frame.data),
                        width: frame.width,
                        height: frame.height,
                        pts_ns: self.start_ns + (f64::from(frame.timestamp) * 1e9) as u64,
                    }));
                }
                FfmpegEvent::Error(e) | FfmpegEvent::Log(LogLevel::Error, e) => {
                    last_error = Some(e);
                }
                _ => {}
            }
        }
        match last_error {
            Some(e) if !self.yielded_any => Err(CodecError::Backend(e)),
            _ => Ok(None),
        }
    }
}

impl Drop for FfmpegFrameSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Decode an audio file to low-rate mono PCM and fold it into one peak
/// (max absolute sample) per `1/peaks_per_sec` bucket. Blocking; run on a
/// worker thread. `-loglevel error` keeps the unread stderr pipe from
/// filling while stdout is drained (peaks never use the event iterator).
pub(crate) fn run_peaks(
    audio: &Path,
    peaks_per_sec: u32,
) -> Result<crate::peaks::AudioPeaks, CodecError> {
    const SAMPLE_RATE: u32 = 8_000;
    let mut cmd = FfmpegCommand::new();
    cmd.args(["-hide_banner", "-loglevel", "error", "-i"]);
    cmd.arg(audio);
    cmd.args([
        "-map",
        "0:a:0",
        "-ac",
        "1",
        "-ar",
        &SAMPLE_RATE.to_string(),
        "-f",
        "f32le",
        "-",
    ]);
    let mut child = cmd.spawn()?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| CodecError::Backend("ffmpeg stdout unavailable".to_owned()))?;

    let bucket_len = (SAMPLE_RATE / peaks_per_sec.max(1)).max(1) as u64;
    let mut peaks = Vec::new();
    let mut bucket_peak = 0f32;
    let mut samples = 0u64;
    let mut reader = std::io::BufReader::with_capacity(1 << 16, stdout);
    let mut raw = [0u8; 4];
    while std::io::Read::read_exact(&mut reader, &mut raw).is_ok() {
        bucket_peak = bucket_peak.max(f32::from_le_bytes(raw).abs().min(1.0));
        samples += 1;
        if samples.is_multiple_of(bucket_len) {
            peaks.push(bucket_peak);
            bucket_peak = 0.0;
        }
    }
    if !samples.is_multiple_of(bucket_len) {
        peaks.push(bucket_peak);
    }

    let status = child.wait()?;
    if !status.success() || samples == 0 {
        // stderr is small under -loglevel error; safe to read after exit.
        let detail = child
            .take_stderr()
            .map(|mut err| {
                let mut s = String::new();
                let _ = std::io::Read::read_to_string(&mut err, &mut s);
                s.trim().to_owned()
            })
            .unwrap_or_default();
        return Err(CodecError::Backend(format!(
            "peaks decode failed ({status}): {detail}"
        )));
    }
    Ok(crate::peaks::AudioPeaks {
        duration_ns: samples * 1_000_000_000 / u64::from(SAMPLE_RATE),
        peaks_per_sec,
        peaks,
    })
}

/// Decode `count` evenly spaced frames into `%03d.jpg` thumbnails under
/// `out_dir`. Blocking; run on a worker thread.
pub(crate) fn run_thumbs(
    video: &Path,
    out_dir: &Path,
    count: u32,
    width: u32,
    duration_ns: u64,
) -> Result<(), CodecError> {
    let duration_s = (duration_ns as f64 / 1e9).max(0.001);
    let mut cmd = FfmpegCommand::new();
    cmd.arg("-i");
    cmd.arg(video);
    cmd.args([
        "-vf",
        &format!("fps={:.6},scale={width}:-2", f64::from(count) / duration_s),
        "-frames:v",
        &count.to_string(),
        "-q:v",
        "4",
        "-y",
    ]);
    cmd.arg(out_dir.join("%03d.jpg"));
    let mut child = cmd.spawn()?;
    let errors: Vec<String> = child
        .iter()
        .map_err(|e| CodecError::Backend(e.to_string()))?
        .filter_errors()
        .collect();
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(CodecError::Backend(format!(
            "thumbs ffmpeg exited with {status}: {}",
            errors.join("; ")
        )))
    }
}
