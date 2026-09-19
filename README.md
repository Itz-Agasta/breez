# Breez

> Record your product. Make it beautiful. Ship the demo.

Breez is a lightweight, opinionated screen recorder and demo editor for developers and product teams, written entirely in Rust. It is not a general-purpose video editor: one recording, a handful of high-leverage effects (zoom, styled backgrounds, click highlights, music), and a fast MP4 export.

**Workflow:** Record -> Trim -> Zoom & pan -> Style background -> Music -> Export

> **Status:** early development. The full Record -> Trim -> Zoom -> Music -> Export path is implemented and verified headlessly; the editor UI for it is still being exercised by hand, so expect rough edges. Undo/redo and crash recovery are not built yet (see the roadmap below).

[![CI](https://github.com/Itz-Agasta/breez/actions/workflows/ci.yml/badge.svg)](https://github.com/Itz-Agasta/breez/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)

## Why Breez

- **Lightweight.** Native code, immediate-mode UI, no Electron, no browser runtime. Frames stream straight from the capture backend into the encoder.
- **Crash-safe by design.** Recordings are fragmented MP4; if the app or machine dies mid-take, everything up to the last fragment is playable.
- **Non-destructive.** Editing only mutates a small JSON file. Your recorded media is never rewritten until you explicitly export.
- **Open format.** A `.rec` project is a plain directory of standard media files plus JSON. Nothing proprietary, inspect it with `ffprobe` and `cat`.

## The `.rec` format

A Breez project is a directory package containing standard streams, no custom binary containers:

```
MyDemo.rec/
  manifest.json          # format tag + crash-recovery flag
  project.json           # timeline + style (the only file edits touch)
  media/
    screen/take-000.mp4  # fragmented MP4 video, crash-recoverable
    audio/take-000.m4a   # AAC system audio
    music/               # imported tracks (copied in, self-contained)
  events/
    input-000.jsonl      # normalized mouse/keyboard events
  cache/                 # thumbnails, waveforms (disposable)
```

## Architecture

Cargo workspace, one concern per crate:

| Crate | Purpose |
|---|---|
| `breez-core` | Project model, timeline, `.rec` package I/O, input event log |
| `breez-capture` | Capture session (via [pinray](https://crates.io/crates/pinray)) -> encoded streams + event log |
| `breez-codec` | Encode/decode behind our own API (ffmpeg-sidecar as the first backend, fully internal) |
| `breez-render` | Frame compositor: wallpaper, zoom, radius, shadow, click ripples |
| `breez` (app) | egui/eframe UI: record view, editor, timeline |

Design rules that keep it fast and small:

- No async runtime: std threads + channels, the UI thread never blocks on IO
- ffmpeg runs out-of-process and never leaks into public APIs; hardware encoding (VAAPI/VideoToolbox/NVENC/QSV) is a flag, not a rewrite
- Frames travel as `Arc<[u8]>`, decoded on demand into a bounded cache

## Building

Requires Rust 1.88+. The ffmpeg binary is downloaded automatically on first run.

```sh
git clone https://github.com/Itz-Agasta/breez.git
cd breez
cargo run -p breez
```

Development checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

## Platform notes

| Platform | Capture | Input events |
|---|---|---|
| Linux Wayland | Display via desktop portal | Not available (portal limitation), effects degrade gracefully |
| Linux X11 | Display, window best-effort | rdev listener |
| macOS / Windows | Planned via pinray backends | rdev listener |

## Roadmap (MVP)

- [x] Workspace scaffold, CI, frameless themed window
- [x] Headless capture core: screen + system audio -> `.rec`
- [x] Record flow + editor shell
- [x] Playback, scrubbing, and trim
- [x] Zoom & pan keyframes with cursor-follow
- [x] Music import, waveforms, fades
- [x] MP4 export (H.264 + AAC; hardware encode still to come)
- [ ] Polish: undo/redo, autosave, crash recovery flow

Post-MVP: microphone/voiceover, camera bubble, re-rendered cursor, captions, GIF/WebM export.
