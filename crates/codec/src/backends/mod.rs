//! Codec backends. ffmpeg is the first and only one; keep every ffmpeg
//! import inside `ffmpeg.rs` so the boundary stays compiler-enforced.

pub(crate) mod ffmpeg;
pub(crate) mod graph;
