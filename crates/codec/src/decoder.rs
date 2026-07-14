//! Streaming video decoder: RGBA frames out of an encoded file, starting at
//! an arbitrary source time. Sequential playback pulls `next_frame`
//! repeatedly; scrubbing calls `seek`, which restarts the backend process at
//! the new position.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::CodecError;
use crate::backends::ffmpeg::FfmpegFrameSource;

/// One decoded frame. `data` is tightly packed RGBA and shared, never
/// copied, between the decode thread, frame cache, and texture upload.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    /// Presentation time in source nanoseconds (file time, not timeline).
    pub pts_ns: u64,
}

pub struct VideoDecoder {
    path: PathBuf,
    source: FfmpegFrameSource,
}

impl VideoDecoder {
    /// Open `path` positioned at `start_ns` source time.
    pub fn open(path: &Path, start_ns: u64) -> Result<Self, CodecError> {
        Ok(Self {
            path: path.to_path_buf(),
            source: FfmpegFrameSource::spawn(path, start_ns)?,
        })
    }

    /// Reposition to `start_ns`. The old backend process is torn down first
    /// so at most one decode process runs per decoder.
    pub fn seek(&mut self, start_ns: u64) -> Result<(), CodecError> {
        self.source = FfmpegFrameSource::spawn(&self.path, start_ns)?;
        Ok(())
    }

    /// Next frame in presentation order, `None` at end of stream.
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>, CodecError> {
        self.source.next_frame()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
