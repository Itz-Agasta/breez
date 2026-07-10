//! ffmpeg-sidecar backend: the only module allowed to import ffmpeg types.
//!
//! Encoders are ffmpeg child processes fed rawvideo/pcm over stdin, writing
//! fragmented MP4 (`frag_keyframe+empty_moov`) so a crash loses at most the
//! last fragment. Keyframe every second (`-g fps`) keeps scrubbing cheap.

use std::io::Write;
use std::path::Path;
use std::process::ChildStdin;

use ffmpeg_sidecar::child::FfmpegChild;
use ffmpeg_sidecar::command::FfmpegCommand;

use crate::CodecError;
use crate::encoder::{AudioEncoderConfig, PixelFormat, VideoEncoderConfig};

pub(crate) fn ensure_available() -> Result<(), CodecError> {
    ffmpeg_sidecar::download::auto_download().map_err(|e| CodecError::Backend(e.to_string()))
}

pub(crate) struct FfmpegSink {
    child: FfmpegChild,
    stdin: Option<ChildStdin>,
}

impl FfmpegSink {
    pub(crate) fn spawn_video(
        dest: &Path,
        config: &VideoEncoderConfig,
    ) -> Result<Self, CodecError> {
        let pix_fmt = match config.pixel_format {
            PixelFormat::Bgra => "bgra",
            PixelFormat::Rgba => "rgba",
        };
        let mut cmd = FfmpegCommand::new();
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            pix_fmt,
            "-s",
            &format!("{}x{}", config.width, config.height),
            "-r",
            &config.fps.to_string(),
            "-i",
            "pipe:0",
            // yuv420p needs even dimensions; crop a stray odd row/column.
            "-vf",
            "crop=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-g",
            &config.fps.to_string(),
            "-movflags",
            "+frag_keyframe+empty_moov",
            "-y",
        ]);
        cmd.arg(dest);
        Self::spawn(cmd)
    }

    pub(crate) fn spawn_audio(
        dest: &Path,
        config: &AudioEncoderConfig,
    ) -> Result<Self, CodecError> {
        let mut cmd = FfmpegCommand::new();
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "f32le",
            "-ar",
            &config.sample_rate.to_string(),
            "-ac",
            &config.channels.to_string(),
            "-i",
            "pipe:0",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+frag_keyframe+empty_moov",
            "-y",
        ]);
        cmd.arg(dest);
        Self::spawn(cmd)
    }

    fn spawn(mut cmd: FfmpegCommand) -> Result<Self, CodecError> {
        let mut child = cmd.spawn()?;
        let stdin = child
            .take_stdin()
            .ok_or_else(|| CodecError::Backend("ffmpeg stdin unavailable".to_owned()))?;
        Ok(Self {
            child,
            stdin: Some(stdin),
        })
    }

    pub(crate) fn write(&mut self, data: &[u8]) -> Result<(), CodecError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| CodecError::Backend("encoder already finished".to_owned()))?;
        stdin.write_all(data)?;
        Ok(())
    }

    /// Close stdin so ffmpeg flushes the trailer, then wait and check the
    /// exit status.
    pub(crate) fn finish(mut self) -> Result<(), CodecError> {
        drop(self.stdin.take());
        let status = self.child.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(CodecError::Backend(format!("ffmpeg exited with {status}")))
        }
    }
}
