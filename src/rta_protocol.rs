//! 39. Real-Time protocol engine.
//!
//! Транспорт-независимый кодек для real-time сообщений.
//! Работает поверх WebSocket, WebTransport или WebRTC без изменений.
//!
//! # Формат кадра
//!
//! ```text
//! ┌──────┬────────┬──────┬───────┬───────┬──────────────┐
//! │ MAGIC│ VERSION│ KIND │  SEQ  │  LEN  │   PAYLOAD    │
//! │ 2 B  │  1 B   │ 1 B  │  4 B  │  4 B  │   N байт     │
//! └──────┴────────┴──────┴───────┴───────┴──────────────┘
//!  0x52 0x54  0x01   ...   u32 BE  u32 BE   ...
//! ```
//!
//! Заголовок — 12 байт, big-endian для сетевого порядка.
//! Payload — произвольные байты, до MAX_PAYLOAD.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

use crate::format_str::format_str;

pub const MAGIC: [u8; 2] = [0x52, 0x54]; // "RT"
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 12;
pub const MAX_PAYLOAD: usize = 1024 * 1024; // 1 MiB

// ============================================================
//                    ТИПЫ КАДРОВ
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Text = 0x01,
    Binary = 0x02,
    Ping = 0x03,
    Pong = 0x04,
    Close = 0x05,
    Error = 0x06,
}

impl FrameKind {
    pub fn from_byte(b: u8) -> Option<Self> {
        return match b {
            0x01 => Some(FrameKind::Text),
            0x02 => Some(FrameKind::Binary),
            0x03 => Some(FrameKind::Ping),
            0x04 => Some(FrameKind::Pong),
            0x05 => Some(FrameKind::Close),
            0x06 => Some(FrameKind::Error),
            _ => None,
        };
    }
    pub fn as_byte(self) -> u8 {
        return self as u8;
    }
    pub fn as_str(self) -> &'static str {
        return match self {
            FrameKind::Text => "text",
            FrameKind::Binary => "binary",
            FrameKind::Ping => "ping",
            FrameKind::Pong => "pong",
            FrameKind::Close => "close",
            FrameKind::Error => "error",
        };
    }
}

// ============================================================
//                    КАДР
// ============================================================

#[derive(Clone, Debug)]
pub struct Frame {
    pub kind: FrameKind,
    pub seq: u32,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn text(seq: u32, s: &str) -> Self {
        return Self {
            kind: FrameKind::Text,
            seq,
            payload: s.as_bytes().to_vec(),
        };
    }
    pub fn binary(seq: u32, bytes: &[u8]) -> Self {
        return Self {
            kind: FrameKind::Binary,
            seq,
            payload: bytes.to_vec(),
        };
    }
    pub fn ping(seq: u32) -> Self {
        return Self {
            kind: FrameKind::Ping,
            seq,
            payload: Vec::new(),
        };
    }
    pub fn pong(seq: u32) -> Self {
        return Self {
            kind: FrameKind::Pong,
            seq,
            payload: Vec::new(),
        };
    }
    pub fn total_size(&self) -> usize {
        return HEADER_LEN + self.payload.len();
    }
    pub fn as_text(&self) -> String {
        return String::from_utf8_lossy(&self.payload).into_owned();
    }
}

// ============================================================
//                    ENCODE
// ============================================================

pub fn encode(frame: &Frame) -> Result<Vec<u8>, String> {
    if frame.payload.len() > MAX_PAYLOAD {
        return Err(format_str(format_args!(
            "payload too large: {} > {}",
            frame.payload.len(),
            MAX_PAYLOAD,
        )));
    }
    let mut out = Vec::with_capacity(frame.total_size());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(frame.kind.as_byte());
    out.extend_from_slice(&frame.seq.to_be_bytes());
    out.extend_from_slice(&(frame.payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&frame.payload);
    return Ok(out);
}

// ============================================================
//                    DECODE
// ============================================================

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    BadMagic { got: [u8; 2] },
    BadVersion { got: u8 },
    BadKind { got: u8 },
    PayloadTooLarge { len: u32 },
    IncompleteHeader { have: usize },
    IncompletePayload { have: usize, need: usize },
}

impl DecodeError {
    pub fn as_str(&self) -> String {
        return match self {
            DecodeError::BadMagic { got } => format_str(format_args!(
                "bad magic: {:02x} {:02x}",
                got[0],
                got[1],
            )),
            DecodeError::BadVersion { got } => format_str(format_args!(
                "bad version: {got}",
            )),
            DecodeError::BadKind { got } => format_str(format_args!(
                "unknown kind: 0x{got:02x}",
            )),
            DecodeError::PayloadTooLarge { len } => format_str(format_args!(
                "payload too large: {len}",
            )),
            DecodeError::IncompleteHeader { have } => format_str(format_args!(
                "incomplete header: {have}/{HEADER_LEN}",
            )),
            DecodeError::IncompletePayload { have, need } => format_str(format_args!(
                "incomplete payload: {have}/{need}",
            )),
        };
    }
    pub fn is_incomplete(&self) -> bool {
        return matches!(
            self,
            DecodeError::IncompleteHeader { .. }
                | DecodeError::IncompletePayload { .. }
        );
    }
}

pub fn try_decode_one(buf: &[u8]) -> Result<(Frame, usize), DecodeError> {
    if buf.len() < HEADER_LEN {
        return Err(DecodeError::IncompleteHeader { have: buf.len() });
    }
    if buf[0] != MAGIC[0] || buf[1] != MAGIC[1] {
        return Err(DecodeError::BadMagic { got: [buf[0], buf[1]] });
    }
    if buf[2] != VERSION {
        return Err(DecodeError::BadVersion { got: buf[2] });
    }
    let kind = FrameKind::from_byte(buf[3])
        .ok_or(DecodeError::BadKind { got: buf[3] })?;
    let mut seq_bytes = [0u8; 4];
    seq_bytes.copy_from_slice(&buf[4..8]);
    let seq = u32::from_be_bytes(seq_bytes);
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&buf[8..12]);
    let len = u32::from_be_bytes(len_bytes);
    if len as usize > MAX_PAYLOAD {
        return Err(DecodeError::PayloadTooLarge { len });
    }
    let total = HEADER_LEN + len as usize;
    if buf.len() < total {
        return Err(DecodeError::IncompletePayload {
            have: buf.len() - HEADER_LEN,
            need: len as usize,
        });
    }
    let payload = buf[HEADER_LEN..total].to_vec();
    return Ok((Frame { kind, seq, payload }, total));
}

// ============================================================
//                    STATEFUL DECODER
// ============================================================

pub struct FrameDecoder {
    buf: Vec<u8>,
    frames_emitted: u64,
    bytes_consumed: u64,
}

impl FrameDecoder {
    pub fn new() -> Self {
        return Self {
            buf: Vec::with_capacity(1024),
            frames_emitted: 0,
            bytes_consumed: 0,
        };
    }

    pub fn frames_emitted(&self) -> u64 {
        return self.frames_emitted;
    }
    pub fn bytes_consumed(&self) -> u64 {
        return self.bytes_consumed;
    }
    pub fn pending_bytes(&self) -> usize {
        return self.buf.len();
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<Result<Frame, DecodeError>> {
        self.buf.extend_from_slice(chunk);
        let mut out: Vec<Result<Frame, DecodeError>> = Vec::new();
        loop {
            match try_decode_one(&self.buf) {
                Ok((frame, consumed)) => {
                    self.buf.drain(0..consumed);
                    self.frames_emitted += 1;
                    self.bytes_consumed += consumed as u64;
                    out.push(Ok(frame));
                }
                Err(e) if e.is_incomplete() => {
                    break;
                }
                Err(e) => {
                    self.buf.clear();
                    out.push(Err(e));
                    break;
                }
            }
        }
        return out;
    }

    pub fn reset(&mut self) {
        self.buf.clear();
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        return Self::new();
    }
}

// ============================================================
//                    UI HELPERS
// ============================================================

pub fn hex_preview(bytes: &[u8], max: usize) -> String {
    let n = bytes.len().min(max);
    let mut out = String::with_capacity(n * 3 + 16);
    for (i, b) in bytes.iter().take(n).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format_str(format_args!("{:02x}", b)));
    }
    if bytes.len() > max {
        out.push_str(" …");
    }
    return out;
}

pub struct FrameAnatomy {
    pub magic: [u8; 2],
    pub version: u8,
    pub kind_byte: u8,
    pub seq: u32,
    pub len: u32,
    pub payload_preview: String,
}

pub fn anatomy(bytes: &[u8]) -> Option<FrameAnatomy> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    let mut seq_bytes = [0u8; 4];
    seq_bytes.copy_from_slice(&bytes[4..8]);
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&bytes[8..12]);
    return Some(FrameAnatomy {
        magic: [bytes[0], bytes[1]],
        version: bytes[2],
        kind_byte: bytes[3],
        seq: u32::from_be_bytes(seq_bytes),
        len: u32::from_be_bytes(len_bytes),
        payload_preview: hex_preview(&bytes[HEADER_LEN..], 16),
    });
}