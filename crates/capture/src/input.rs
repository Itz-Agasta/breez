//! Global input listener feeding the take's event log.
//!
//! Uses rdev (X11/Windows/macOS). Wayland has no global listener; there we
//! skip event logging entirely and say so once. Timestamps are anchored to
//! the first video frame so events line up with `stream_time_ns`-derived
//! take time; events arriving before that anchor are dropped.
//!
//! rdev's `listen` blocks its thread forever with no shutdown API, so the
//! listener runs on a detached thread that turns into a no-op once the
//! recording stops.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use breez_core::events::{EventLogWriter, InputEvent, InputKind, MouseButton};

pub(crate) struct InputLogger {
    active: Arc<AtomicBool>,
    writer: Arc<Mutex<Option<EventLogWriter>>>,
}

impl InputLogger {
    /// Start listening, or return `None` where global listening is not
    /// possible (Wayland) or the log file cannot be created.
    pub(crate) fn start(
        path: PathBuf,
        anchor: Arc<OnceLock<Instant>>,
        display_size: Arc<OnceLock<(u32, u32)>>,
    ) -> Option<Self> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            log::warn!("Wayland session: no global input listener, skipping event log");
            return None;
        }
        let writer = match EventLogWriter::create(&path) {
            Ok(w) => Arc::new(Mutex::new(Some(w))),
            Err(e) => {
                log::warn!("cannot create event log {path:?}: {e}");
                return None;
            }
        };
        let active = Arc::new(AtomicBool::new(true));
        let logger = Self {
            active: Arc::clone(&active),
            writer: Arc::clone(&writer),
        };

        std::thread::spawn(move || {
            let mut last_pos = (0.0f32, 0.0f32);
            let result = rdev::listen(move |event| {
                if !active.load(Ordering::Relaxed) {
                    return;
                }
                let Some(start) = anchor.get() else {
                    return;
                };
                let Some(&(w, h)) = display_size.get() else {
                    return;
                };
                let (kind, button, pos) = match event.event_type {
                    rdev::EventType::MouseMove { x, y } => {
                        let pos = (x as f32 / w as f32, y as f32 / h as f32);
                        last_pos = pos;
                        (InputKind::Move, None, pos)
                    }
                    rdev::EventType::ButtonPress(b) => (InputKind::Down, map_button(b), last_pos),
                    rdev::EventType::ButtonRelease(b) => (InputKind::Up, map_button(b), last_pos),
                    _ => return,
                };
                let record = InputEvent {
                    t_ns: start.elapsed().as_nanos() as u64,
                    kind,
                    button,
                    x: pos.0.clamp(0.0, 1.0),
                    y: pos.1.clamp(0.0, 1.0),
                };
                if let Ok(mut guard) = writer.lock()
                    && let Some(w) = guard.as_mut()
                {
                    let _ = w.write(&record);
                }
            });
            if let Err(e) = result {
                log::warn!("input listener unavailable: {e:?}");
            }
        });
        Some(logger)
    }

    /// Stop recording events and flush the log. The rdev thread stays parked
    /// (no shutdown API) but writes nothing further.
    pub(crate) fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        if let Ok(mut guard) = self.writer.lock()
            && let Some(mut w) = guard.take()
        {
            let _ = w.flush();
        }
    }
}

fn map_button(b: rdev::Button) -> Option<MouseButton> {
    match b {
        rdev::Button::Left => Some(MouseButton::Left),
        rdev::Button::Right => Some(MouseButton::Right),
        rdev::Button::Middle => Some(MouseButton::Middle),
        rdev::Button::Unknown(_) => None,
    }
}
