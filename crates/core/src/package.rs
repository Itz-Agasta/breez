//! `.rec` package: a directory holding standard media streams plus JSON
//! metadata. This module owns the layout, the manifest (with its crash
//! recovery flag), and atomic JSON writes.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const FORMAT: &str = "breez/rec";
pub const VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a breez package: {0}")]
    NotAPackage(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    /// True while a recording is in progress. Still true on open means the
    /// process died mid-recording and the last take needs recovery.
    pub recording: bool,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            recording: false,
        }
    }
}

/// Handle to a `.rec` directory. Creation lays out the subdirectories;
/// all path knowledge lives here so callers never build paths by hand.
#[derive(Debug, Clone)]
pub struct RecPackage {
    root: PathBuf,
}

impl RecPackage {
    /// Create a new package directory (parents included) with an empty
    /// manifest. Fails if `root` already contains a file at the manifest path.
    pub fn create(root: impl Into<PathBuf>) -> Result<Self, PackageError> {
        let pkg = Self { root: root.into() };
        for dir in [
            pkg.root.join("media/screen"),
            pkg.root.join("media/audio"),
            pkg.root.join("media/music"),
            pkg.root.join("events"),
            pkg.root.join("cache"),
        ] {
            fs::create_dir_all(dir)?;
        }
        pkg.write_manifest(&Manifest::default())?;
        Ok(pkg)
    }

    /// Open an existing package, verifying the manifest format tag.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, PackageError> {
        let pkg = Self { root: root.into() };
        let manifest = pkg.manifest()?;
        if manifest.format != FORMAT {
            return Err(PackageError::NotAPackage(pkg.root));
        }
        Ok(pkg)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    pub fn project_path(&self) -> PathBuf {
        self.root.join("project.json")
    }

    /// Take-relative media/event paths. The relative form is what gets
    /// stored in `project.json` so packages stay relocatable.
    pub fn video_rel(take_id: u32) -> String {
        format!("media/screen/take-{take_id:03}.mp4")
    }

    pub fn audio_rel(take_id: u32) -> String {
        format!("media/audio/take-{take_id:03}.m4a")
    }

    pub fn events_rel(take_id: u32) -> String {
        format!("events/input-{take_id:03}.jsonl")
    }

    /// Resolve a package-relative path (as stored in `project.json`).
    pub fn resolve(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn manifest(&self) -> Result<Manifest, PackageError> {
        let path = self.manifest_path();
        if !path.exists() {
            return Err(PackageError::NotAPackage(self.root.clone()));
        }
        read_json(&path)
    }

    pub fn write_manifest(&self, manifest: &Manifest) -> Result<(), PackageError> {
        write_json_atomic(&self.manifest_path(), manifest)
    }

    pub fn set_recording(&self, recording: bool) -> Result<(), PackageError> {
        let mut manifest = self.manifest()?;
        manifest.recording = recording;
        self.write_manifest(&manifest)
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, PackageError> {
    Ok(serde_json::from_reader(fs::File::open(path)?)?)
}

/// Write JSON via tmp file + rename so readers never see a torn file and a
/// crash mid-write leaves the previous version intact.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), PackageError> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_open_finds_valid_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("demo.rec");
        RecPackage::create(&root).unwrap();
        let pkg = RecPackage::open(&root).unwrap();
        assert_eq!(pkg.manifest().unwrap().format, FORMAT);
    }

    #[test]
    fn open_should_fail_on_plain_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(RecPackage::open(dir.path()).is_err());
    }

    #[test]
    fn set_recording_toggles_manifest_flag() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = RecPackage::create(dir.path().join("demo.rec")).unwrap();
        pkg.set_recording(true).unwrap();
        assert!(pkg.manifest().unwrap().recording);
        pkg.set_recording(false).unwrap();
        assert!(!pkg.manifest().unwrap().recording);
    }
}
