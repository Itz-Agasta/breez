use std::io::Write;
use wincode::{SchemaRead, SchemaWrite};

pub const MAGIC: &[u8; 8] = b"RECSTUDI"; // 8 bit file signature
pub const VERSION: u32 = 1;

#[derive(SchemaWrite, SchemaRead, Debug)]
pub struct RecHeader {
    pub version: u8,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub created_at: u64, // unix timestamp
}

#[derive(SchemaWrite, SchemaRead, Clone, Debug)]
pub struct FrameMeta {
    pub timestamp_ms: u64,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub cursor_visible: bool,
    pub byte_len: u32, // raw BGRA frame size
}

// Writer: appends frames sequentially
pub struct RecWriter<W: Write> {
    inner: W,
}

impl<W: Write> RecWriter<W> {
    /// Opens a new `.rec` file by writing the 8-byte magic signature `RECSTUDI`
    /// followed by the length-prefixed, wincode-serialized header.
    ///
    /// After `new` returns, frames can be appended via [`write_frame`].
    pub fn new(mut inner: W, header: &RecHeader) -> anyhow::Result<Self> {
        inner.write_all(MAGIC)?;
        let header_bytes = wincode::serialize(header)?;
        let len = header_bytes.len() as u32;
        inner.write_all(&len.to_le_bytes())?;
        inner.write_all(&header_bytes)?;
        Ok(Self { inner })
    }

    /// Appends a single raw BGRA frame to the `.rec` file.
    ///
    /// Each frame is stored as:
    /// - `meta_len` (4 bytes, LE): byte size of the serialized `FrameMeta`
    /// - `meta`: wincode-serialized frame metadata (timestamp, cursor, `byte_len`)
    /// - `bgra`: raw pixel data, exactly `meta.byte_len` bytes
    ///
    /// The BGRA data is expected to be `width × height × 4` bytes (4 bytes per pixel).
    pub fn write_frame(&mut self, meta: &FrameMeta, bgra: &[u8]) -> anyhow::Result<()> {
        let meta_bytes = wincode::serialize(meta)?;
        let meta_len = meta_bytes.len() as u32;
        self.inner.write_all(&meta_len.to_le_bytes())?;
        self.inner.write_all(&meta_bytes)?;
        self.inner.write_all(bgra)?;
        Ok(())
    }
}
