//! Audio output for preview playback: one rodio device sink per session,
//! one player per loaded take audio file. The audio position is the master
//! playback clock (video slaves to it), so A/V drift cannot accumulate.
//!
//! Decoding happens inside rodio (symphonia AAC/MP4); the ffmpeg-backed
//! codec crate is not involved in audio preview.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink};

pub struct AudioOut {
    device: MixerDeviceSink,
    player: Option<rodio::Player>,
    loaded: Option<PathBuf>,
    /// Gain to give every player this loads: a fresh rodio player starts at
    /// 1.0, so a take loaded mid-playback would ignore the configured
    /// system-audio volume.
    volume: f32,
}

impl AudioOut {
    /// `None` when no output device is available; playback then falls back
    /// to a wallclock-driven playhead.
    pub fn open() -> Option<Self> {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(mut device) => {
                // The sink lives for the whole session; the drop log is
                // noise when a new session replaces the player.
                device.log_on_drop(false);
                Some(Self {
                    device,
                    player: None,
                    loaded: None,
                    volume: 1.0,
                })
            }
            Err(e) => {
                log::warn!("audio output unavailable: {e}");
                None
            }
        }
    }

    /// Make `path` the loaded source (idempotent). Returns false when the
    /// file cannot be decoded.
    pub fn load(&mut self, path: &Path) -> bool {
        // Only a player that still has audio queued can be reused. One that
        // played the take to its end is spent (seeking it is a no-op, so
        // replaying would run silent), and a failed load left none at all,
        // which a later attempt should redo rather than cache forever.
        if self.loaded.as_deref() == Some(path)
            && self.player.as_ref().is_some_and(|player| !player.empty())
        {
            return true;
        }
        // Dropping the old player stops it; clear() would block the UI.
        self.player = None;
        self.loaded = Some(path.to_path_buf());
        let source = match File::open(path).map_err(|e| e.to_string()) {
            Ok(file) => match Decoder::try_from(file) {
                Ok(source) => source,
                Err(e) => {
                    log::warn!("audio decode {}: {e}", path.display());
                    return false;
                }
            },
            Err(e) => {
                log::warn!("audio open {}: {e}", path.display());
                return false;
            }
        };
        let player = rodio::Player::connect_new(self.device.mixer());
        player.pause();
        player.set_volume(self.volume);
        player.append(source);
        self.player = Some(player);
        true
    }

    /// A fresh player on the shared output mixer (music tracks each own
    /// one so their volumes and positions are independent).
    pub fn new_player(&self) -> rodio::Player {
        rodio::Player::connect_new(self.device.mixer())
    }

    /// Seek the loaded source to `src_ns`. Returns false when seeking is
    /// not possible (caller falls back to the wallclock).
    pub fn seek(&self, src_ns: u64) -> bool {
        let Some(player) = &self.player else {
            return false;
        };
        match player.try_seek(Duration::from_nanos(src_ns)) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("audio seek: {e}");
                false
            }
        }
    }

    pub fn play(&self) {
        if let Some(player) = &self.player {
            player.play();
        }
    }

    pub fn pause(&self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        if let Some(player) = &self.player {
            player.set_volume(volume);
        }
    }

    /// Position within the loaded source, or `None` once it ran out (the
    /// clock then has nothing authoritative to say).
    pub fn src_pos_ns(&self) -> Option<u64> {
        let player = self.player.as_ref()?;
        if player.empty() {
            return None;
        }
        Some(player.get_pos().as_nanos() as u64)
    }
}
