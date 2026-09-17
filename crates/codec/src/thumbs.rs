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
    // ffmpeg legitimately emits fewer than `count` on a short take, so
    // "are all of them present?" is never true again for those takes and
    // would re-run the whole decode on every session. A marker written
    // after a successful run records that the set is as complete as it is
    // going to get; a run that died partway leaves no marker and retries.
    let marker = out_dir.join(".complete");
    if !marker.exists() {
        std::fs::create_dir_all(out_dir)?;
        ffmpeg::run_thumbs(video, out_dir, count, width, duration_ns)?;
        std::fs::write(&marker, [])?;
    }
    // ffmpeg may emit slightly fewer frames than asked on short takes; keep
    // whatever exists rather than failing the filmstrip.
    Ok(paths.into_iter().filter(|p| p.exists()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_thumbs_should_reuse_a_partial_set_that_finished_decoding() {
        // ffmpeg emits fewer than `count` on a short take, so requiring all
        // of them re-ran the whole decode on every session open.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".complete"), []).expect("marker");
        std::fs::write(dir.path().join("001.jpg"), []).expect("thumb");
        std::fs::write(dir.path().join("002.jpg"), []).expect("thumb");

        // A video path that does not exist: reaching ffmpeg would fail, so
        // this passing proves the decode was skipped.
        let paths = generate_thumbs(
            Path::new("/nonexistent/take.mp4"),
            dir.path(),
            20,
            160,
            1_000_000_000,
        )
        .expect("cache hit");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn generate_thumbs_should_decode_when_no_marker_was_written() {
        // A run that died partway leaves thumbs but no marker, and must retry.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("001.jpg"), []).expect("thumb");
        assert!(
            generate_thumbs(
                Path::new("/nonexistent/take.mp4"),
                dir.path(),
                20,
                160,
                1_000_000_000,
            )
            .is_err()
        );
    }
}
