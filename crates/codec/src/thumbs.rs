//! Filmstrip thumbnails: evenly spaced tiny JPEGs decoded from a take's
//! video into the package's disposable `cache/` area.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::CodecError;
use crate::backends::ffmpeg;

/// Generate `count` thumbnails of `width` px into `out_dir` (created if
/// missing) and return their paths in take order. Skips the decode when a
/// set for the same request is already cached, so callers can invoke it on
/// every session open. Blocking; run on a worker thread.
pub fn generate_thumbs(
    video: &Path,
    out_dir: &Path,
    count: u32,
    width: u32,
    duration_ns: u64,
) -> Result<Vec<PathBuf>, CodecError> {
    // A finished set lives in a directory named for the request it answers,
    // and a run publishes only by moving its whole directory into place.
    // That gives both halves of a correct cache hit: the name cannot match a
    // set decoded at some other count or width, and a set is never read
    // while a run is still filling it, so two workers on the same take (the
    // editor respawns one per take added to the package) cannot be seen
    // writing the same JPEG. ffmpeg legitimately emits fewer than `count` on
    // a short take, so the published directory, not a full file count, is
    // what says the decode is as complete as it is going to get.
    let set_dir = out_dir.join(format!("{count}x{width}"));
    if !set_dir.exists() {
        publish(video, out_dir, &set_dir, count, width, duration_ns)?;
    }
    Ok((1..=count)
        .map(|n| set_dir.join(format!("{n:03}.jpg")))
        .filter(|p| p.exists())
        .collect())
}

/// Decode into a directory of this run's own, then move it into place.
fn publish(
    video: &Path,
    out_dir: &Path,
    set_dir: &Path,
    count: u32,
    width: u32,
    duration_ns: u64,
) -> Result<(), CodecError> {
    /// Distinguishes concurrent runs inside one process; the pid does the
    /// same across processes.
    static RUN: AtomicU64 = AtomicU64::new(0);

    let staging = out_dir.join(format!(
        ".staging-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    // Whatever a crashed run with this name left behind is not ours.
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let decoded = ffmpeg::run_thumbs(video, &staging, count, width, duration_ns);
    if decoded.is_ok() {
        // Fails when another run published first, and that set answers the
        // same request, so only the decode's own error is worth reporting.
        let _ = std::fs::rename(&staging, set_dir);
    }
    let _ = std::fs::remove_dir_all(&staging);
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A video path that does not exist: reaching ffmpeg fails, so a call
    /// that succeeds proves the decode was skipped.
    const MISSING: &str = "/nonexistent/take.mp4";

    fn publish_set(out_dir: &Path, name: &str, thumbs: u32) {
        let set = out_dir.join(name);
        std::fs::create_dir_all(&set).expect("set dir");
        for n in 1..=thumbs {
            std::fs::write(set.join(format!("{n:03}.jpg")), []).expect("thumb");
        }
    }

    #[test]
    fn generate_thumbs_should_reuse_a_partial_set_that_finished_decoding() {
        // ffmpeg emits fewer than `count` on a short take, so requiring all
        // of them re-ran the whole decode on every session open.
        let dir = tempfile::tempdir().expect("tempdir");
        publish_set(dir.path(), "20x160", 2);

        let paths =
            generate_thumbs(Path::new(MISSING), dir.path(), 20, 160, 1_000_000_000).expect("hit");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn generate_thumbs_should_decode_when_nothing_was_published() {
        // A run that died partway leaves its staging directory, never a
        // published set, so the next call retries.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("001.jpg"), []).expect("thumb");
        assert!(generate_thumbs(Path::new(MISSING), dir.path(), 20, 160, 1_000_000_000).is_err());
    }

    #[test]
    fn generate_thumbs_should_not_serve_a_set_decoded_for_another_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        publish_set(dir.path(), "20x160", 20);

        // Same take, different count and different width: both must decode.
        assert!(generate_thumbs(Path::new(MISSING), dir.path(), 30, 160, 1_000_000_000).is_err());
        assert!(generate_thumbs(Path::new(MISSING), dir.path(), 20, 320, 1_000_000_000).is_err());
    }
}
