//! Binary framing protocol definitions matching `tempterm/internal/proto/frame.go`.

use bytes::Bytes;
use std::io::{self, Read};
use thiserror::Error;

/// VT output from daemon to client that brings the client terminal to the pane's state.
pub const FRAME_OUTPUT: u8 = b'o';

/// Input typed by user from client to daemon.
pub const FRAME_INPUT: u8 = b'i';

/// Resize request from client to daemon: cols, rows, x_pixels, y_pixels (uint16s).
pub const FRAME_RESIZE: u8 = b'r';

/// Confirmation from daemon to client that pane took cols, rows.
pub const FRAME_RESIZED: u8 = b'z';

/// Notification from daemon to client that pane has closed.
pub const FRAME_CLOSED: u8 = b'c';

/// Update of 19 RGBA colors shown in terminal.
pub const FRAME_THEME: u8 = b't';

/// Key press by name (e.g. "shift+tab", "ctrl+enter", "esc", "f5").
pub const FRAME_KEY: u8 = b'k';

/// Bracketed paste text typed by user.
pub const FRAME_PASTE: u8 = b'p';

/// Maximum allowable frame size (16 MiB), matching Go `maxFrame = 16 << 20`.
pub const MAX_FRAME: usize = 16 << 20;

/// Errors that can occur when encoding or decoding binary frames.
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("proto: frame too large")]
    FrameTooLarge,
    #[error("unexpected end of file reading frame")]
    UnexpectedEof,
    #[error("clean end of file before frame")]
    CleanEof,
    #[error("truncated header reading frame")]
    TruncatedHeader,
    #[error("truncated payload reading frame")]
    TruncatedPayload,
}

/// A parsed binary protocol frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: u8,
    pub payload: Bytes,
}

impl Frame {
    /// Creates a new frame with given kind and payload.
    pub fn new(kind: u8, payload: impl Into<Bytes>) -> Self {
        Self {
            kind,
            payload: payload.into(),
        }
    }
}

/// Appends a frame to a destination buffer.
///
/// Wire layout:
/// - 1 byte `kind`
/// - 4 bytes `length` (Big-Endian `u32`)
/// - `payload` bytes
pub fn append_frame(dst: &mut Vec<u8>, kind: u8, payload: &[u8]) {
    let n = payload.len() as u32;
    dst.reserve(5 + payload.len());
    dst.push(kind);
    dst.extend_from_slice(&n.to_be_bytes());
    dst.extend_from_slice(payload);
}

/// Encodes small numbers (clamped to 0..0xFFFF) into a big-endian uint16 byte vector for resize frames.
pub fn uint16s(values: &[usize]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 2);
    for &v in values {
        let clamped = (v.min(0xFFFF)) as u16;
        out.extend_from_slice(&clamped.to_be_bytes());
    }
    out
}

/// Decodes big-endian uint16 numbers from byte slice.
/// Returns (values, true) if slice has exactly `2 * n` bytes, or (empty, false) otherwise.
pub fn parse_uint16s(b: &[u8], n: usize) -> (Vec<usize>, bool) {
    if b.len() != 2 * n {
        return (Vec::new(), false);
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let chunk: [u8; 2] = [b[2 * i], b[2 * i + 1]];
        out.push(u16::from_be_bytes(chunk) as usize);
    }
    (out, true)
}

/// Reads a frame from an `io::Read` stream.
///
/// Mirrors Go's `ReadFrame`:
/// - Reads 5-byte header.
/// - Validates length <= 16 MiB.
/// - If cursor has fewer bytes than declared length, rejects immediately without allocating.
pub fn read_frame<R: Read>(r: &mut R) -> Result<Frame, FrameError> {
    let mut hdr = [0u8; 5];
    let mut read_header = 0;
    while read_header < 5 {
        match r.read(&mut hdr[read_header..]) {
            Ok(0) => {
                if read_header == 0 {
                    return Err(FrameError::CleanEof);
                } else {
                    return Err(FrameError::TruncatedHeader);
                }
            }
            Ok(n) => read_header += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FrameError::Io(e)),
        }
    }

    let kind = hdr[0];
    let n = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;

    if n > MAX_FRAME {
        return Err(FrameError::FrameTooLarge);
    }

    // Incremental bounded allocation: do not allocate n bytes upfront.
    // Instead, allocate chunks up to 64 KiB, verifying incoming data availability.
    let mut payload = Vec::with_capacity(n.min(64 * 1024));
    let mut remaining = n;
    let mut chunk_buf = [0u8; 64 * 1024];

    while remaining > 0 {
        let to_read = remaining.min(chunk_buf.len());
        match r.read(&mut chunk_buf[..to_read]) {
            Ok(0) => return Err(FrameError::TruncatedPayload),
            Ok(bytes_read) => {
                payload.extend_from_slice(&chunk_buf[..bytes_read]);
                remaining -= bytes_read;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FrameError::Io(e)),
        }
    }

    Ok(Frame {
        kind,
        payload: Bytes::from(payload),
    })
}
