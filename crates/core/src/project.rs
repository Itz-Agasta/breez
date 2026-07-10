//! `project.json` v1: serde model of takes, the non-destructive timeline,
//! and style settings. Editing mutates this file only; media is immutable.

use serde::{Deserialize, Serialize};

use crate::package::{PackageError, RecPackage, read_json, write_json_atomic};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub takes: Vec<Take>,
    #[serde(default)]
    pub timeline: Timeline,
    #[serde(default)]
    pub style: Style,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            takes: Vec::new(),
            timeline: Timeline::default(),
            style: Style::default(),
        }
    }

    pub fn load(package: &RecPackage) -> Result<Self, PackageError> {
        let project: Self = read_json(&package.project_path())?;
        if project.version != 1 {
            return Err(PackageError::UnsupportedVersion(project.version));
        }
        Ok(project)
    }

    pub fn save(&self, package: &RecPackage) -> Result<(), PackageError> {
        write_json_atomic(&package.project_path(), self)
    }
}

/// One recording session: encoded streams plus the raw input-event log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Take {
    pub id: u32,
    /// Package-relative paths (see `RecPackage::resolve`).
    pub video: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<String>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_ns: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Timeline {
    pub clips: Vec<Clip>,
    pub zoom: Vec<ZoomSegment>,
    pub music: Vec<MusicTrack>,
}

/// A trimmed slice of a take. `src_in_ns..src_out_ns` selects source time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub take: u32,
    pub src_in_ns: u64,
    pub src_out_ns: u64,
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoomSegment {
    pub in_ns: u64,
    pub out_ns: u64,
    pub level: f32,
    /// Normalized 0..1 focus point.
    pub anchor: [f32; 2],
    pub follow_cursor: bool,
    pub easing: Easing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Easing {
    Linear,
    Smooth,
    Snap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicTrack {
    /// Package-relative path under `media/music/`.
    pub file: String,
    pub offset_ns: u64,
    pub gain: f32,
    pub fade_in_ns: u64,
    pub fade_out_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub wallpaper: String,
    pub padding: u32,
    pub radius: u32,
    pub shadow: u32,
    pub ratio: String,
    pub cursor: CursorStyle,
    pub system_audio_gain: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            wallpaper: "aurora".to_owned(),
            padding: 64,
            radius: 14,
            shadow: 72,
            ratio: "16:9".to_owned(),
            cursor: CursorStyle::default(),
            system_audio_gain: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorStyle {
    pub click_highlight: bool,
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            click_highlight: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::RecPackage;

    #[test]
    fn save_then_load_preserves_takes_and_timeline() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = RecPackage::create(dir.path().join("demo.rec")).unwrap();
        let mut project = Project::new("demo");
        project.takes.push(Take {
            id: 0,
            video: RecPackage::video_rel(0),
            audio: Some(RecPackage::audio_rel(0)),
            events: None,
            width: 2560,
            height: 1440,
            fps: 60,
            duration_ns: 10_000_000_000,
        });
        project.timeline.clips.push(Clip {
            take: 0,
            src_in_ns: 0,
            src_out_ns: 10_000_000_000,
            speed: 1.0,
        });
        project.save(&pkg).unwrap();

        let loaded = Project::load(&pkg).unwrap();
        assert_eq!(loaded.takes.len(), 1);
        assert_eq!(loaded.takes[0].video, "media/screen/take-000.mp4");
        assert_eq!(loaded.timeline.clips[0].src_out_ns, 10_000_000_000);
        assert_eq!(loaded.style.wallpaper, "aurora");
    }
}
