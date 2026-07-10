//! Streaming encoders: raw frames or samples in, an encoded file out.
//! One stream per encoder, matching the `.rec` layout where video and audio
//! are independently addressable files.

use std::path::Path;

use crate::CodecError;
use crate::backends::ffmpeg::FfmpegSink;

/// Pixel layout of the raw bytes pushed into a [`VideoEncoder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra,
    Rgba,
}

#[derive(Debug, Clone)]
pub struct VideoEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub pixel_format: PixelFormat,
}

/// H.264 fragmented-MP4 encoder. Frames must arrive at a constant rate
/// (`fps`); the capture pipeline is responsible for pacing/duplication.
pub struct VideoEncoder {
    sink: FfmpegSink,
    frame_len: usize,
}

impl VideoEncoder {
    pub fn create(dest: &Path, config: &VideoEncoderConfig) -> Result<Self, CodecError> {
        let sink = FfmpegSink::spawn_video(dest, config)?;
        Ok(Self {
            sink,
            frame_len: config.width as usize * config.height as usize * 4,
        })
    }

    /// Push one tightly-packed frame (no row padding).
    pub fn push_frame(&mut self, data: &[u8]) -> Result<(), CodecError> {
        if data.len() != self.frame_len {
            return Err(CodecError::BadInput(format!(
                "frame is {} bytes, expected {}",
                data.len(),
                self.frame_len
            )));
        }
        self.sink.write(data)
    }

    pub fn finish(self) -> Result<(), CodecError> {
        self.sink.finish()
    }
}

#[derive(Debug, Clone)]
pub struct AudioEncoderConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

/// AAC fragmented-MP4 (m4a) encoder for interleaved f32 samples.
pub struct AudioEncoder {
    sink: FfmpegSink,
}

impl AudioEncoder {
    pub fn create(dest: &Path, config: &AudioEncoderConfig) -> Result<Self, CodecError> {
        let sink = FfmpegSink::spawn_audio(dest, config)?;
        Ok(Self { sink })
    }

    /// Push raw interleaved f32le sample bytes.
    pub fn push_samples(&mut self, data: &[u8]) -> Result<(), CodecError> {
        self.sink.write(data)
    }

    pub fn finish(self) -> Result<(), CodecError> {
        self.sink.finish()
    }
}
