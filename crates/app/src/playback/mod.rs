//! Preview playback: a `Player` per editing session drives the decode
//! thread, the frame cache, and audio output, and owns the playhead.
//!
//! Clocking: while playing, the audio position is the master clock and the
//! video texture shows the newest decoded frame at or before it; without
//! audio (no device, silent take) a wallclock anchor drives the playhead.
//! Paused scrubbing seeks the decoder (or hits the LRU cache) and shows a
//! single frame.

mod audio;
mod cache;
mod decode;
mod music;

use std::collections::VecDeque;
use std::time::Instant;

use breez_codec::VideoFrame;
use breez_core::timeline::ClipTime;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::app::Session;
use audio::AudioOut;
use cache::{FrameCache, FrameKey};
use decode::{DecodeHandle, SeekCmd};
use music::MusicMix;

pub struct Player {
    ctx: egui::Context,
    decode: DecodeHandle,
    audio: Option<AudioOut>,
    music: MusicMix,
    cache: FrameCache,
    texture: Option<TextureHandle>,
    pending: VecDeque<VideoFrame>,
    generation: u64,
    /// A decoder seek was issued and its first frame has not arrived yet.
    awaiting_seek: bool,
    /// Seek requested while one was in flight; issued once the first lands.
    deferred_seek_ns: Option<u64>,
    /// (take, source ns) the decoder will emit next; `None` = needs a seek.
    decoder_pos: Option<(u32, u64)>,
    playing: bool,
    playhead_ns: u64,
    current_clip: Option<usize>,
    /// Wallclock fallback anchor: (instant, playhead at that instant).
    wall_anchor: Option<(Instant, u64)>,
    /// Timeline offset of the audio source: playhead = offset + audio pos.
    audio_offset_ns: Option<i64>,
    volume: f32,
}

impl Player {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            decode: DecodeHandle::spawn(ctx.clone()),
            audio: AudioOut::open(),
            music: MusicMix::default(),
            ctx,
            cache: FrameCache::new(cache::DEFAULT_CAP_BYTES),
            texture: None,
            pending: VecDeque::new(),
            generation: 0,
            awaiting_seek: false,
            deferred_seek_ns: None,
            decoder_pos: None,
            playing: false,
            playhead_ns: 0,
            current_clip: None,
            wall_anchor: None,
            audio_offset_ns: None,
            volume: 1.0,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn playhead_ns(&self) -> u64 {
        self.playhead_ns
    }

    /// Current preview frame, if one has been decoded.
    pub fn texture(&self) -> Option<&TextureHandle> {
        self.texture.as_ref()
    }

    pub fn toggle(&mut self, session: &Session) {
        if self.playing {
            self.pause();
        } else {
            self.play(session);
        }
    }

    pub fn play(&mut self, session: &Session) {
        let duration = session.project.timeline.duration_ns();
        if duration == 0 || self.playing {
            return;
        }
        if self.playhead_ns >= duration {
            self.playhead_ns = 0;
        }
        let Some(ct) = resolve(session, self.playhead_ns) else {
            return;
        };
        self.position_for(session, ct);
        self.playing = true;
        self.wall_anchor = Some((Instant::now(), self.playhead_ns));
        if let Some(audio) = &self.audio {
            audio.set_volume(self.volume);
            audio.play();
        }
        self.ctx.request_repaint();
    }

    pub fn pause(&mut self) {
        self.playing = false;
        if let Some(audio) = &self.audio {
            audio.pause();
        }
        self.music.pause_all();
    }

    /// Move the playhead. Paused seeks serve from cache when possible and
    /// throttle decoder respawns to one in flight; seeks while playing do a
    /// full reposition so audio and video restart together.
    pub fn seek(&mut self, session: &Session, t_ns: u64) {
        let duration = session.project.timeline.duration_ns();
        self.playhead_ns = t_ns.min(duration);
        // The exact end resolves inside the last frame, not past it.
        let target = self.playhead_ns.min(duration.saturating_sub(1));
        let Some(ct) = resolve(session, target) else {
            return;
        };
        if self.playing {
            self.position_for(session, ct);
            self.wall_anchor = Some((Instant::now(), self.playhead_ns));
            if let Some(audio) = &self.audio {
                audio.play();
            }
            return;
        }
        let Some(take) = take_of(session, ct.take) else {
            return;
        };
        let key = FrameKey {
            take: ct.take,
            frame: frame_index(ct.src_ns, take.fps),
        };
        if let Some(frame) = self.cache.get(key) {
            self.upload(&frame);
            // The decoder stream no longer matches the playhead; play()
            // repositions it.
            self.decoder_pos = None;
            if self.awaiting_seek {
                // An older seek is in flight; retarget it here so its frame
                // cannot land after (and overwrite) this newer position.
                self.deferred_seek_ns = Some(self.playhead_ns);
            }
            return;
        }
        if self.awaiting_seek {
            self.deferred_seek_ns = Some(self.playhead_ns);
        } else {
            self.seek_decoder(session, ct);
        }
    }

    /// Gain for preview audio; applied live while playing.
    pub fn set_volume(&mut self, volume: f32) {
        if (volume - self.volume).abs() > f32::EPSILON {
            self.volume = volume;
            if let Some(audio) = &self.audio {
                audio.set_volume(volume);
            }
        }
    }

    /// Per-frame pump: drain decoded frames, advance the clock, pick the
    /// frame for the playhead. Call once per UI frame while editing.
    pub fn tick(&mut self, session: &Session) {
        self.drain_frames(session);
        if !self.playing {
            return;
        }

        let duration = session.project.timeline.duration_ns();
        self.playhead_ns = self.clock_ns().min(duration);
        if self.playhead_ns >= duration {
            self.playhead_ns = duration;
            self.pause();
        }

        let target = self.playhead_ns.min(duration.saturating_sub(1));
        if let Some(ct) = resolve(session, target) {
            // Crossing into another clip repositions decoder + audio.
            if self.playing && self.current_clip != Some(ct.clip) {
                self.position_for(session, ct);
                if let Some(audio) = &self.audio {
                    audio.play();
                }
            }
            self.present(session, ct);
        }
        if self.playing {
            self.music
                .tick(self.audio.as_ref(), session, self.playhead_ns);
            self.ctx.request_repaint();
        }
    }

    /// Pull decoded frames off the channel. Only drains while playing or
    /// waiting on a seek: a parked channel is what pauses the decoder.
    fn drain_frames(&mut self, session: &Session) {
        if !self.playing && !self.awaiting_seek {
            return;
        }
        let mut first_after_seek = None;
        while let Ok(decoded) = self.decode.frame_rx.try_recv() {
            if decoded.generation != self.generation {
                continue;
            }
            let Some(frame) = decoded.frame else {
                // This generation is finished and produced nothing further.
                // Clearing the flag is what keeps a later scrub able to
                // issue a fresh seek instead of deferring forever.
                self.awaiting_seek = false;
                if let Some(t_ns) = self.deferred_seek_ns.take() {
                    self.seek(session, t_ns);
                }
                continue;
            };
            if let Some((take, _)) = self.decoder_pos {
                let fps = take_of(session, take).map_or(60, |t| t.fps);
                self.cache.insert(
                    FrameKey {
                        take,
                        frame: frame_index(frame.pts_ns, fps),
                    },
                    frame.clone(),
                );
                self.decoder_pos = Some((take, frame.pts_ns + 1_000_000_000 / u64::from(fps)));
            }
            if self.awaiting_seek {
                self.awaiting_seek = false;
                first_after_seek = Some(frame.clone());
            }
            self.pending.push_back(frame);
            // Stop pulling once satisfied (paused) or far enough ahead;
            // leaving the channel full is what parks the decoder.
            if !self.awaiting_seek && (!self.playing || self.pending.len() >= 8) {
                break;
            }
        }
        if let Some(frame) = first_after_seek {
            if !self.playing {
                self.upload(&frame);
                self.pending.clear();
            }
            if let Some(t_ns) = self.deferred_seek_ns.take() {
                self.seek(session, t_ns);
            }
        }
    }

    /// Show the newest pending frame at or before the playhead's source
    /// position, dropping older ones.
    fn present(&mut self, session: &Session, ct: ClipTime) {
        let fps = take_of(session, ct.take).map_or(60, |t| t.fps);
        let deadline = ct.src_ns + 500_000_000 / u64::from(fps);
        let mut show = None;
        while let Some(front) = self.pending.front() {
            if front.pts_ns <= deadline {
                show = self.pending.pop_front();
            } else {
                break;
            }
        }
        if let Some(frame) = show {
            self.upload(&frame);
        }
    }

    /// Point decoder and audio at `ct` (used by play, playing-seek, and clip
    /// transitions).
    fn position_for(&mut self, session: &Session, ct: ClipTime) {
        self.current_clip = Some(ct.clip);
        let near = self.decoder_pos.is_some_and(|(take, pos)| {
            let fps = take_of(session, ct.take).map_or(60, |t| t.fps);
            take == ct.take && pos.abs_diff(ct.src_ns) <= 500_000_000 / u64::from(fps)
        });
        if !near {
            self.seek_decoder(session, ct);
        }
        self.audio_offset_ns = None;
        let Some(audio) = &mut self.audio else {
            return;
        };
        let loaded = take_of(session, ct.take)
            .and_then(|take| take.audio.as_deref())
            .and_then(|rel| session.package.resolve(rel).ok())
            .is_some_and(|path| audio.load(&path));
        if loaded && audio.seek(ct.src_ns) {
            // playhead = offset + audio source position (speed 1.0 clips).
            self.audio_offset_ns = Some(self.playhead_ns as i64 - ct.src_ns as i64);
        } else {
            audio.pause();
        }
    }

    fn seek_decoder(&mut self, session: &Session, ct: ClipTime) {
        let Some(path) =
            take_of(session, ct.take).and_then(|take| session.package.resolve(&take.video).ok())
        else {
            return;
        };
        self.generation += 1;
        self.awaiting_seek = true;
        self.pending.clear();
        self.decoder_pos = Some((ct.take, ct.src_ns));
        let _ = self.decode.cmd_tx.send(SeekCmd {
            generation: self.generation,
            path,
            start_ns: ct.src_ns,
        });
    }

    /// Playhead time while playing: audio position when it is driving,
    /// otherwise the wallclock anchor.
    fn clock_ns(&self) -> u64 {
        if let (Some(offset), Some(audio)) = (self.audio_offset_ns, &self.audio)
            && let Some(pos) = audio.src_pos_ns()
        {
            return (offset + pos as i64).max(0) as u64;
        }
        match self.wall_anchor {
            Some((instant, at_ns)) => at_ns + instant.elapsed().as_nanos() as u64,
            None => self.playhead_ns,
        }
    }

    fn upload(&mut self, frame: &VideoFrame) {
        let image = ColorImage::from_rgba_unmultiplied(
            [frame.width as usize, frame.height as usize],
            &frame.data,
        );
        match &mut self.texture {
            Some(texture) => texture.set(image, TextureOptions::LINEAR),
            None => {
                self.texture = Some(self.ctx.load_texture(
                    "preview",
                    image,
                    TextureOptions::LINEAR,
                ));
            }
        }
    }
}

fn resolve(session: &Session, t_ns: u64) -> Option<ClipTime> {
    session.project.timeline.resolve(t_ns)
}

fn take_of(session: &Session, id: u32) -> Option<&breez_core::project::Take> {
    session.project.takes.iter().find(|t| t.id == id)
}

fn frame_index(src_ns: u64, fps: u32) -> u64 {
    (src_ns as f64 * f64::from(fps) / 1e9).round() as u64
}
