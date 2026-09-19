//! Export: composited RGBA frames in, one H.264 + AAC faststart MP4 out.
//!
//! The caller pushes every output frame over a pipe; ffmpeg opens the take
//! audio and the music files itself and mixes them through the filter graph
//! in `backends::graph`, so no PCM ever crosses this boundary and there is no
//! DSP of our own to keep in sync with the preview.

use std::path::{Path, PathBuf};

use crate::CodecError;
use crate::backends::ffmpeg::{self, FfmpegSink};

/// One timeline clip's slice of the take audio.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub src_in_ns: u64,
    pub src_out_ns: u64,
    pub speed: f32,
}

/// The take's recorded system audio, trimmed to the timeline's clips.
#[derive(Debug, Clone)]
pub struct SystemAudio {
    pub path: PathBuf,
    pub gain: f32,
    pub clips: Vec<AudioClip>,
}

/// One music track placed on the timeline.
#[derive(Debug, Clone)]
pub struct MusicSource {
    pub path: PathBuf,
    pub gain: f32,
    pub offset_ns: u64,
    pub fade_in_ns: u64,
    pub fade_out_ns: u64,
    /// Length of the audible span measured from `offset_ns`, that is
    /// `min(offset_ns + media, timeline_end) - offset_ns`. The caller derives
    /// it the same way `breez_core::render::music_gain_at` does, so the fade
    /// anchors cannot disagree between preview and export.
    pub audible_ns: u64,
}

#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// x264 constant rate factor; lower is better quality.
    pub crf: u8,
    pub system_audio: Option<SystemAudio>,
    pub music: Vec<MusicSource>,
}

pub struct Exporter {
    /// Declared before `dest` so ffmpeg is killed and reaped before the
    /// file it was writing is removed.
    sink: FfmpegSink,
    dest: Destination,
    frame_bytes: usize,
}

/// The output path, removed when the exporter goes away without a
/// successful `finish`. The destination is a path the user picked, not a
/// temp file, so a truncated MP4 left there looks like a finished export.
/// Holding the cleanup in its own field (rather than a `Drop` on
/// `Exporter`) is what lets `finish` consume the sink.
struct Destination {
    path: PathBuf,
    keep: bool,
}

impl Drop for Destination {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Exporter {
    pub fn create(dest: &Path, config: &ExportConfig) -> Result<Self, CodecError> {
        if config.width == 0 || config.height == 0 || config.fps == 0 {
            return Err(CodecError::BadInput(
                "export size and fps must be non-zero".to_owned(),
            ));
        }
        let frame_bytes = (config.width as usize)
            .checked_mul(config.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                CodecError::BadInput(format!(
                    "export size {}x{} has no representable frame",
                    config.width, config.height
                ))
            })?;
        Ok(Self {
            sink: ffmpeg::spawn_export(dest, config)?,
            dest: Destination {
                path: dest.to_path_buf(),
                keep: false,
            },
            frame_bytes,
        })
    }

    /// Push one composited RGBA8 frame.
    pub fn push_frame(&mut self, rgba: &[u8]) -> Result<(), CodecError> {
        if rgba.len() != self.frame_bytes {
            return Err(CodecError::BadInput(format!(
                "frame is {} bytes, expected {}",
                rgba.len(),
                self.frame_bytes
            )));
        }
        self.sink.write(rgba)
    }

    /// Close the pipe and wait for ffmpeg to write the trailer. Only a
    /// success keeps the file: any other way out of the exporter, a write
    /// error the caller propagated or a panic included, removes it.
    pub fn finish(mut self) -> Result<(), CodecError> {
        self.sink.finish()?;
        self.dest.keep = true;
        Ok(())
    }

    /// Cancel: dropping is the whole job, since the sink's `Drop` kills and
    /// reaps ffmpeg and `Destination`'s takes the partial file. Consuming
    /// `self` is the point, so the exporter cannot be used afterwards.
    pub fn abort(self) {}
}
