//! Waveform peaks for the music lane: one max-amplitude value per fixed
//! time bucket, decoded once per audio file and cached as a small JSON in
//! the package's disposable `cache/` area (raw audio never stays in RAM).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::CodecError;
use crate::backends::ffmpeg;

/// Peak buckets per second of audio. ~50 gives sub-pixel resolution at any
/// plausible timeline width while keeping a 5-minute track under 100 KB.
pub const PEAKS_PER_SEC: u32 = 50;

/// Decoded waveform summary: peak amplitude (0..1) per bucket, plus the
/// exact media duration the decode measured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioPeaks {
    pub duration_ns: u64,
    pub peaks_per_sec: u32,
    pub peaks: Vec<f32>,
}

/// Peaks for `audio`, served from `cache_path` when a matching cache exists,
/// otherwise decoded (blocking; run on a worker thread) and cached.
pub fn generate_peaks(audio: &Path, cache_path: &Path) -> Result<AudioPeaks, CodecError> {
    if let Ok(file) = fs::File::open(cache_path)
        && let Ok(cached) = serde_json::from_reader::<_, AudioPeaks>(file)
        && cached.peaks_per_sec == PEAKS_PER_SEC
    {
        return Ok(cached);
    }
    let peaks = ffmpeg::run_peaks(audio, PEAKS_PER_SEC)?;
    // A failed cache write only costs a re-decode next session, so an
    // unwritable package (read-only media, say) still gets its waveform.
    if let Err(e) = write_cache(cache_path, &peaks) {
        log::warn!("peaks cache {}: {e}", cache_path.display());
    }
    Ok(peaks)
}

fn write_cache(cache_path: &Path, peaks: &AudioPeaks) -> Result<(), CodecError> {
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec(peaks).map_err(|e| CodecError::Backend(e.to_string()))?;
    fs::write(cache_path, json)?;
    Ok(())
}
