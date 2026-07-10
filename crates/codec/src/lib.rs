//! breez-codec: encode and decode media through our own API.
//!
//! Public surface is Breez types only (`VideoEncoder`, `AudioEncoder`, and
//! later the decoders). ffmpeg-sidecar is the first backend and is confined
//! to `backends/ffmpeg.rs`; no ffmpeg type may appear in a public signature.

mod backends;
pub mod encoder;

pub use encoder::{
    AudioEncoder, AudioEncoderConfig, PixelFormat, VideoEncoder, VideoEncoderConfig,
};

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend: {0}")]
    Backend(String),
    #[error("bad input: {0}")]
    BadInput(String),
}

/// Make sure an ffmpeg binary is available: uses the system install when
/// present, otherwise downloads a static build next to the app data.
pub fn ensure_ffmpeg() -> Result<(), CodecError> {
    backends::ffmpeg::ensure_available()
}
