//! Music preview mix: one rodio player per music track on the session's
//! output mixer. Each tick sets every player's volume from the shared
//! `breez_core::render::music_gain_at` envelope (gain + fades), so the
//! preview mix and the export mix come from the same math. Position is
//! slaved to the playhead by re-seeking whenever it drifts.

use std::collections::HashMap;
use std::fs::File;
use std::time::Duration;

use rodio::Decoder;

use super::audio::AudioOut;
use crate::app::Session;

/// Re-seek a player when it is this far from where the playhead says it
/// should be (covers seeks, offset drags, and newly entered spans).
const RESYNC_NS: u64 = 250_000_000;

#[derive(Default)]
pub struct MusicMix {
    tracks: HashMap<String, TrackPlayer>,
}

#[derive(Default)]
struct TrackPlayer {
    player: Option<rodio::Player>,
    /// Decode or seek failed; don't retry every frame.
    failed: bool,
}

impl MusicMix {
    /// Per-frame update while playing. `audio` is the session output; with
    /// no device there is nothing to mix.
    pub fn tick(&mut self, audio: Option<&AudioOut>, session: &Session, playhead_ns: u64) {
        let Some(audio) = audio else {
            return;
        };
        let music = &session.project.timeline.music;
        self.tracks
            .retain(|rel, _| music.iter().any(|t| &t.file == rel));
        let duration = session.project.timeline.duration_ns();
        for track in music {
            let gain = breez_core::render::music_gain_at(track, playhead_ns, duration);
            let entry = self.tracks.entry(track.file.clone()).or_default();
            if gain <= 0.0 {
                entry.pause();
                continue;
            }
            entry.play_at(audio, session, track, playhead_ns - track.offset_ns, gain);
        }
    }

    pub fn pause_all(&self) {
        for entry in self.tracks.values() {
            entry.pause();
        }
    }
}

impl TrackPlayer {
    fn pause(&self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    /// Keep the player decoded, positioned at `src_ns`, audible at `gain`.
    fn play_at(
        &mut self,
        audio: &AudioOut,
        session: &Session,
        track: &breez_core::project::MusicTrack,
        src_ns: u64,
        gain: f32,
    ) {
        if self.failed {
            return;
        }
        if self.player.as_ref().is_none_or(|p| p.empty()) {
            // First use, or the source ran out (a seek back past the end of
            // a fully played track needs a fresh decoder).
            self.player = self.load(audio, session, &track.file);
            if self.player.is_none() {
                self.failed = true;
                return;
            }
        }
        let player = self.player.as_ref().expect("loaded above");
        let pos_ns = player.get_pos().as_nanos() as u64;
        if pos_ns.abs_diff(src_ns) > RESYNC_NS
            && let Err(e) = player.try_seek(Duration::from_nanos(src_ns))
        {
            log::warn!("music seek {}: {e}", track.file);
            self.failed = true;
            player.pause();
            return;
        }
        player.set_volume(gain);
        player.play();
    }

    fn load(&self, audio: &AudioOut, session: &Session, rel: &str) -> Option<rodio::Player> {
        let path = match session.package.resolve(rel) {
            Ok(path) => path,
            Err(e) => {
                log::warn!("music path {rel}: {e}");
                return None;
            }
        };
        let source = match File::open(&path)
            .map_err(|e| e.to_string())
            .and_then(|f| Decoder::try_from(f).map_err(|e| e.to_string()))
        {
            Ok(source) => source,
            Err(e) => {
                log::warn!("music decode {rel}: {e}");
                return None;
            }
        };
        let player = audio.new_player();
        player.pause();
        player.set_volume(0.0);
        player.append(source);
        Some(player)
    }
}
