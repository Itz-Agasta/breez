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
        let mut project: Self = read_json(&package.project_path())?;
        if project.version != 1 {
            return Err(PackageError::UnsupportedVersion(project.version));
        }
        project.sanitize();
        Ok(project)
    }

    /// Clamp style values and drop/clamp timeline clips that a hand-edited
    /// or stale `project.json` could carry, so consumers (preview, playback,
    /// export) never see out-of-range data.
    pub fn sanitize(&mut self) {
        self.style.clamp();
        // Takes come from disk like everything else here. fps in particular
        // is a divisor all over playback and export, so a zero would panic
        // the UI thread the first time a frame is presented.
        for take in &mut self.takes {
            take.fps = take.fps.max(1);
            take.width = take.width.max(1);
            take.height = take.height.max(1);
        }
        let takes = &self.takes;
        self.timeline.clips.retain_mut(|clip| {
            let Some(take) = takes.iter().find(|t| t.id == clip.take) else {
                return false;
            };
            clip.src_out_ns = clip.src_out_ns.min(take.duration_ns);
            clip.src_in_ns = clip.src_in_ns.min(clip.src_out_ns);
            if !(clip.speed.is_finite() && clip.speed > 0.0) {
                clip.speed = 1.0;
            }
            clip.src_in_ns < clip.src_out_ns
        });
        self.sanitize_zoom();
        self.sanitize_music();
    }

    /// Clamp music gains into range and drop tracks without a file.
    fn sanitize_music(&mut self) {
        self.timeline.music.retain_mut(|track| {
            if !track.gain.is_finite() {
                track.gain = 1.0;
            }
            track.gain = track.gain.clamp(*GAIN_RANGE.start(), *GAIN_RANGE.end());
            !track.file.is_empty()
        });
    }

    /// Clamp zoom segments into valid ranges and restore the sorted,
    /// non-overlapping invariant the editor and renderers rely on.
    fn sanitize_zoom(&mut self) {
        let duration = self.timeline.duration_ns();
        self.timeline.zoom.retain_mut(|segment| {
            if !segment.level.is_finite() {
                segment.level = 1.8;
            }
            segment.level = segment
                .level
                .clamp(*ZOOM_LEVEL_RANGE.start(), *ZOOM_LEVEL_RANGE.end());
            for axis in &mut segment.anchor {
                if !axis.is_finite() {
                    *axis = 0.5;
                }
                *axis = axis.clamp(0.0, 1.0);
            }
            segment.out_ns = segment.out_ns.min(duration);
            segment.in_ns < segment.out_ns
        });
        self.timeline.zoom.sort_by_key(|s| s.in_ns);
        let mut last_end = 0u64;
        self.timeline.zoom.retain(|segment| {
            if segment.in_ns < last_end {
                return false;
            }
            last_end = segment.out_ns;
            true
        });
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
    /// Media length, filled in once the waveform analysis has decoded the
    /// file; 0 = not yet known.
    #[serde(default)]
    pub duration_ns: u64,
}

impl Project {
    /// The take the timeline shows first. Preview geometry and export both
    /// resolve it this way; reading `takes.first()` or `takes.last()`
    /// instead lets the two disagree about frame size whenever a package
    /// holds more takes than the timeline uses.
    pub fn primary_take(&self) -> Option<&Take> {
        let take_id = self.timeline.clips.first()?.take;
        self.takes.iter().find(|take| take.id == take_id)
    }
}

/// Valid style ranges (design-locked). UI controls and [`Style::clamp`]
/// share these so a loaded project can never carry out-of-range values.
pub const PADDING_RANGE: std::ops::RangeInclusive<u32> = 16..=140;
pub const RADIUS_RANGE: std::ops::RangeInclusive<u32> = 0..=28;
pub const SHADOW_RANGE: std::ops::RangeInclusive<u32> = 0..=100;
pub const GAIN_RANGE: std::ops::RangeInclusive<f32> = 0.0..=2.0;
pub const ZOOM_LEVEL_RANGE: std::ops::RangeInclusive<f32> = 1.0..=3.0;
/// Output aspect ratios the editor offers; `style.ratio` must be one of these.
pub const RATIOS: &[&str] = &["16:9", "9:16", "1:1"];

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

impl Style {
    /// Clamp every field into its valid range; unknown wallpaper ids are left
    /// alone (renderers fall back to the first preset).
    pub fn clamp(&mut self) {
        self.padding = self
            .padding
            .clamp(*PADDING_RANGE.start(), *PADDING_RANGE.end());
        self.radius = self
            .radius
            .clamp(*RADIUS_RANGE.start(), *RADIUS_RANGE.end());
        self.shadow = self
            .shadow
            .clamp(*SHADOW_RANGE.start(), *SHADOW_RANGE.end());
        if !self.system_audio_gain.is_finite() {
            self.system_audio_gain = 1.0;
        }
        self.system_audio_gain = self
            .system_audio_gain
            .clamp(*GAIN_RANGE.start(), *GAIN_RANGE.end());
        if !RATIOS.contains(&self.ratio.as_str()) {
            self.ratio = RATIOS[0].to_owned();
        }
    }
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
    fn sanitize_should_floor_take_fps_at_one() {
        // A zero here is a division by zero in playback's frame pacing.
        let mut project = Project::new("t");
        project.takes.push(Take {
            id: 0,
            video: "v".to_owned(),
            audio: None,
            events: None,
            width: 0,
            height: 0,
            fps: 0,
            duration_ns: 1_000_000_000,
        });
        project.sanitize();
        assert_eq!(project.takes[0].fps, 1);
        assert_eq!((project.takes[0].width, project.takes[0].height), (1, 1));
    }

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
        project.timeline.zoom.push(ZoomSegment {
            in_ns: 2_000_000_000,
            out_ns: 5_000_000_000,
            level: 1.8,
            anchor: [0.5, 0.25],
            follow_cursor: true,
            easing: Easing::Smooth,
        });
        project.timeline.music.push(MusicTrack {
            file: "media/music/song.mp3".to_owned(),
            offset_ns: 1_000_000_000,
            gain: 0.4,
            fade_in_ns: 500_000_000,
            fade_out_ns: 1_000_000_000,
            duration_ns: 8_000_000_000,
        });
        project.save(&pkg).unwrap();

        let loaded = Project::load(&pkg).unwrap();
        assert_eq!(loaded.takes.len(), 1);
        assert_eq!(loaded.takes[0].video, "media/screen/take-000.mp4");
        assert_eq!(loaded.timeline.clips[0].src_out_ns, 10_000_000_000);
        assert_eq!(loaded.style.wallpaper, "aurora");
        let zoom = &loaded.timeline.zoom[0];
        assert_eq!(zoom.out_ns, 5_000_000_000);
        assert_eq!(zoom.level, 1.8);
        assert_eq!(zoom.anchor, [0.5, 0.25]);
        assert!(zoom.follow_cursor);
        assert_eq!(zoom.easing, Easing::Smooth);
        let music = &loaded.timeline.music[0];
        assert_eq!(music.file, "media/music/song.mp3");
        assert_eq!(music.offset_ns, 1_000_000_000);
        assert_eq!(music.gain, 0.4);
        assert_eq!(music.duration_ns, 8_000_000_000);
    }

    #[test]
    fn sanitize_should_clamp_music_gain_and_drop_fileless_tracks() {
        let mut project = Project::new("demo");
        let track = |file: &str, gain: f32| MusicTrack {
            file: file.to_owned(),
            offset_ns: 0,
            gain,
            fade_in_ns: 0,
            fade_out_ns: 0,
            duration_ns: 0,
        };
        project.timeline.music = vec![
            track("media/music/a.mp3", 9.0),
            track("media/music/b.mp3", f32::NAN),
            track("", 1.0),
        ];
        project.sanitize();
        let music = &project.timeline.music;
        assert_eq!(music.len(), 2);
        assert_eq!(music[0].gain, 2.0);
        assert_eq!(music[1].gain, 1.0);
    }

    #[test]
    fn sanitize_should_clamp_sort_and_deoverlap_zoom_segments() {
        let mut project = Project::new("demo");
        project.takes.push(Take {
            id: 0,
            video: RecPackage::video_rel(0),
            audio: None,
            events: None,
            width: 1920,
            height: 1080,
            fps: 60,
            duration_ns: 10_000,
        });
        project.timeline.clips.push(Clip {
            take: 0,
            src_in_ns: 0,
            src_out_ns: 10_000,
            speed: 1.0,
        });
        let segment = |in_ns, out_ns| ZoomSegment {
            in_ns,
            out_ns,
            level: 9.0,
            anchor: [2.0, f32::NAN],
            follow_cursor: false,
            easing: Easing::Linear,
        };
        project.timeline.zoom = vec![
            segment(6_000, 99_000), // clamps to timeline end
            segment(1_000, 4_000),  // sorts first
            segment(3_000, 5_000),  // overlaps the previous, dropped
            segment(5_000, 5_000),  // empty, dropped
        ];
        project.sanitize();
        let zoom = &project.timeline.zoom;
        assert_eq!(zoom.len(), 2);
        assert_eq!((zoom[0].in_ns, zoom[0].out_ns), (1_000, 4_000));
        assert_eq!((zoom[1].in_ns, zoom[1].out_ns), (6_000, 10_000));
        assert_eq!(zoom[0].level, 3.0);
        assert_eq!(zoom[0].anchor, [1.0, 0.5]);
    }

    #[test]
    fn sanitize_should_clamp_style_into_valid_ranges() {
        let mut project = Project::new("demo");
        project.style.padding = 9999;
        project.style.shadow = 500;
        project.style.ratio = "4:3".to_owned();
        project.style.system_audio_gain = f32::NAN;
        project.sanitize();
        assert_eq!(project.style.padding, 140);
        assert_eq!(project.style.shadow, 100);
        assert_eq!(project.style.ratio, "16:9");
        assert_eq!(project.style.system_audio_gain, 1.0);
    }

    #[test]
    fn sanitize_should_drop_invalid_clips_and_clamp_trim() {
        let mut project = Project::new("demo");
        project.takes.push(Take {
            id: 0,
            video: RecPackage::video_rel(0),
            audio: None,
            events: None,
            width: 1920,
            height: 1080,
            fps: 60,
            duration_ns: 5_000,
        });
        project.timeline.clips.push(Clip {
            take: 0,
            src_in_ns: 1_000,
            src_out_ns: 99_000, // beyond the take
            speed: 0.0,         // invalid
        });
        project.timeline.clips.push(Clip {
            take: 7, // unknown take
            src_in_ns: 0,
            src_out_ns: 1_000,
            speed: 1.0,
        });
        project.sanitize();
        assert_eq!(project.timeline.clips.len(), 1);
        assert_eq!(project.timeline.clips[0].src_out_ns, 5_000);
        assert_eq!(project.timeline.clips[0].speed, 1.0);
    }
}
