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
    /// Decoding this file failed; don't retry every frame.
    failed: bool,
    /// Source position at which the player last ran dry. A file shorter than
    /// the span the envelope thinks it covers (its length is unknown until
    /// the waveform pass fills `duration_ns`) would otherwise be decoded
    /// again on every tick; only a playhead back before this point, which
    /// does have audio left to play, reloads it.
    ended_ns: Option<u64>,
    /// Consecutive seek failures. A seek can fail transiently (a player that
    /// just ran dry, a format that needs a fresh decoder), so those drop the
    /// player and reload rather than muting the track for the session; only
    /// a run of them gives up.
    seek_failures: u8,
}

/// Seek failures tolerated before a track is treated as undecodable.
const MAX_SEEK_FAILURES: u8 = 3;

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
        if let Some(player) = &self.player
            && player.empty()
        {
            self.ended_ns = Some(player.get_pos().as_nanos() as u64);
            self.player = None;
        }
        if self.ended_ns.is_some_and(|end| src_ns >= end) {
            // The source is simply over; reloading it would decode the file
            // only to run dry again.
            return;
        }
        self.ended_ns = None;
        if self.player.is_none() {
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
            player.pause();
            self.seek_failures += 1;
            if self.seek_failures >= MAX_SEEK_FAILURES {
                self.failed = true;
            } else {
                // Retry from a fresh decoder on the next tick.
                self.player = None;
            }
            return;
        }
        self.seek_failures = 0;
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
