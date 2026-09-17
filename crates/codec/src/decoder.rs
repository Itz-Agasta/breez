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

/// A decoder walked forward across a timeline, one output frame at a time.
///
/// Export samples source times in order, so the open decoder is reused and
/// only a backwards jump costs a seek. It keeps the last frame it decoded so
/// a take whose declared duration runs slightly past its last decodable
/// frame (a crash-truncated recording, or rounding at the tail) holds on the
/// final frame instead of failing the whole export.
pub struct SequentialReader {
    decoder: VideoDecoder,
    last: Option<VideoFrame>,
}

impl SequentialReader {
    pub fn open(path: &Path, start_ns: u64) -> Result<Self, CodecError> {
        Ok(Self {
            decoder: VideoDecoder::open(path, start_ns)?,
            last: None,
        })
    }

    /// Frame covering source time `src_ns`. Seeks only when `src_ns` falls
    /// behind where the walk has already reached.
    pub fn frame_at(&mut self, src_ns: u64) -> Result<VideoFrame, CodecError> {
        if self.last.as_ref().is_some_and(|f| f.pts_ns > src_ns) {
            self.decoder.seek(src_ns)?;
            self.last = None;
        }
        loop {
            if let Some(frame) = self.last.as_ref().filter(|f| f.pts_ns + 1 >= src_ns) {
                return Ok(frame.clone());
            }
            match self.decoder.next_frame()? {
                Some(frame) => self.last = Some(frame),
                None => {
                    return self.last.clone().ok_or_else(|| {
                        CodecError::BadInput("no decodable frames in source".to_owned())
                    });
                }
            }
        }
    }
}
