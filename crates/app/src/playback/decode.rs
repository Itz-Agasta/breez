//! Decode thread: owns the `VideoDecoder` (one ffmpeg process at a time)
//! and streams frames to the UI over a small bounded channel. The bounded
//! send is the pacing mechanism: when the UI stops draining (paused), the
//! thread parks a few frames ahead instead of decoding the whole file.
//!
//! Every seek carries a generation number; frames echo it back so the UI can
//! discard output from a superseded position.

use std::path::PathBuf;

use breez_codec::{VideoDecoder, VideoFrame};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};
use eframe::egui;

pub struct SeekCmd {
    pub generation: u64,
    pub path: PathBuf,
    pub start_ns: u64,
}

pub struct DecodedFrame {
    pub generation: u64,
    pub frame: VideoFrame,
}

pub struct DecodeHandle {
    pub cmd_tx: Sender<SeekCmd>,
    pub frame_rx: Receiver<DecodedFrame>,
}

impl DecodeHandle {
    /// Spawn the decode thread. It exits when the handle (both channels) is
    /// dropped; dropping the decoder reaps the ffmpeg child.
    pub fn spawn(ctx: egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = unbounded::<SeekCmd>();
        let (frame_tx, frame_rx) = bounded::<DecodedFrame>(4);
        std::thread::Builder::new()
            .name("breez-decode".to_owned())
            .spawn(move || run(&cmd_rx, &frame_tx, &ctx))
            .expect("spawn decode thread");
        Self { cmd_tx, frame_rx }
    }
}

fn run(cmd_rx: &Receiver<SeekCmd>, frame_tx: &Sender<DecodedFrame>, ctx: &egui::Context) {
    let mut decoder: Option<(VideoDecoder, u64)> = None;
    loop {
        // Idle (no decoder) blocks on the next command; while streaming,
        // commands are polled between frames. Only the newest seek matters.
        let cmd = if decoder.is_some() {
            cmd_rx.try_iter().last()
        } else {
            match cmd_rx.recv() {
                Ok(first) => cmd_rx.try_iter().last().or(Some(first)),
                Err(_) => return,
            }
        };
        if let Some(cmd) = cmd {
            let reuse = decoder
                .take()
                .map(|(existing, _)| existing)
                .filter(|d| d.path() == cmd.path);
            let opened = match reuse {
                Some(mut d) => d.seek(cmd.start_ns).map(|()| d),
                None => VideoDecoder::open(&cmd.path, cmd.start_ns),
            };
            match opened {
                Ok(d) => decoder = Some((d, cmd.generation)),
                Err(e) => log::error!("decode open {}: {e}", cmd.path.display()),
            }
        }

        let Some((active, generation)) = &mut decoder else {
            continue;
        };
        match active.next_frame() {
            Ok(Some(frame)) => {
                let decoded = DecodedFrame {
                    generation: *generation,
                    frame,
                };
                // Blocking send paces decode to UI consumption. A full
                // channel with a pending seek resolves because the UI keeps
                // draining while it waits for the new generation.
                match frame_tx.try_send(decoded) {
                    Ok(()) => ctx.request_repaint(),
                    Err(TrySendError::Full(decoded)) => {
                        if frame_tx.send(decoded).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
            Ok(None) => decoder = None,
            Err(e) => {
                log::error!("decode: {e}");
                decoder = None;
            }
        }
    }
}
