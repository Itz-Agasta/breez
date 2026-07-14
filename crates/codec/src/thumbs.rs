//! Filmstrip thumbnails: evenly spaced tiny JPEGs decoded from a take's
//! video into the package's disposable `cache/` area.

use std::path::{Path, PathBuf};

use crate::CodecError;
use crate::backends::ffmpeg;

/// Generate `count` thumbnails of `width` px into `out_dir` (created if
/// missing) and return their paths in take order. Skips the decode when the
/// directory already holds the full set, so callers can invoke it on every
/// session open. Blocking; run on a worker thread.
pub fn generate_thumbs(
    video: &Path,
    out_dir: &Path,
    count: u32,
    width: u32,
    duration_ns: u64,
) -> Result<Vec<PathBuf>, CodecError> {
    let paths: Vec<PathBuf> = (1..=count)
        .map(|n| out_dir.join(format!("{n:03}.jpg")))
        .collect();
    if !paths.iter().all(|p| p.exists()) {
        std::fs::create_dir_all(out_dir)?;
        ffmpeg::run_thumbs(video, out_dir, count, width, duration_ns)?;
    }
    // ffmpeg may emit slightly fewer frames than asked on short takes; keep
    // whatever exists rather than failing the filmstrip.
    Ok(paths.into_iter().filter(|p| p.exists()).collect())
}
