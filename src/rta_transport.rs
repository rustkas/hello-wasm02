//! 39. Unified transport layer.
//!
//! Абстракция над WebSocket, WebTransport и WebRTC.
//! Протокол-движок (`rta_protocol`) не знает, по какому каналу
//! идут байты — только `send_binary` и callback приёма.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

use std::rc::Rc;

use crate::format_str::format_str;
use crate::ws_client::{WsClient, WsConfig, WsEvent, WsState};
use crate::wt_client::{WtClient, WtConfig, WtEvent, WtState};

// ============================================================
//                    ТИП ТРАНСПОРТА
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportKind {
    WebSocket,
    WebTransport,
    WebRtc,
}

impl TransportKind {
    pub fn as_str(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "websocket",
            TransportKind::WebTransport => "webtransport",
            TransportKind::WebRtc => "webrtc",
        };
    }
    pub fn from_str(s: &str) -> Self {
        return match s {
            "websocket" => TransportKind::WebSocket,
            "webtransport" => TransportKind::WebTransport,
            "webrtc" => TransportKind::WebRtc,
            _ => TransportKind::WebSocket,
        };
    }
    pub fn default_url(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "wss://echo.websocket.org",
            TransportKind::WebTransport => "https://127.0.0.1:4433/echo",
            TransportKind::WebRtc => "—",
        };
    }
    pub fn reliability(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "reliable + ordered",
            TransportKind::WebTransport => "reliable + unreliable",
            TransportKind::WebRtc => "configurable per channel",
        };
    }
    pub fn multiplexing(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "нет (1 поток)",
            TransportKind::WebTransport => "да (N streams + datagrams)",
            TransportKind::WebRtc => "да (data channels + media)",
        };
    }
    pub fn latency(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "~50-200 мс",
            TransportKind::WebTransport => "~20-100 мс",
            TransportKind::WebRtc => "~5-50 мс",
        };
    }
    pub fn browser_support(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "все браузеры",
            TransportKind::WebTransport => "Chrome / Edge / Safari 26.4+",
            TransportKind::WebRtc => "все браузеры",
        };
    }
    pub fn setup(self) -> &'static str {
        return match self {
            TransportKind::WebSocket => "минимум",
            TransportKind::WebTransport => "сертификат + QUIC-сервер",
            TransportKind::WebRtc => "STUN/TURN + signaling",
        };
    }
}

// ============================================================
//                    СОСТОЯНИЕ
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportState {
    Idle,
    Connecting,
    Ready,
    Closed,
    Failed,
}

impl TransportState {
    pub fn as_str(self) -> &'static str {
        return match self {
            TransportState::Idle => "idle",
            TransportState::Connecting => "connecting",
            TransportState::Ready => "ready",
            TransportState::Closed => "closed",
            TransportState::Failed => "failed",
        };
    }
}

// ============================================================
//                    СОБЫТИЯ
// ============================================================

#[derive(Clone, Debug)]
pub enum TransportEvent {
    Connected,
    Disconnected(String),
    Bytes(Vec<u8>),
    Error(String),
}

// ============================================================
//                    TRANSPORT
// ============================================================

enum Backend {
    WebSocket(Rc<WsClient>),
    WebTransport(Rc<WtClient>),
}

pub struct Transport {
    kind: TransportKind,
    backend: Backend,
}

impl Transport {
    pub fn ws(url: &str, on_event: Rc<dyn Fn(TransportEvent)>) -> Rc<Transport> {
        let cb = on_event.clone();
        let adapter: Rc<dyn Fn(WsEvent)> = Rc::new(move |ev: WsEvent| {
            match ev {
                WsEvent::Opened => cb(TransportEvent::Connected),
                WsEvent::Closed { code, reason, was_clean } => {
                    // Строка формируется в отдельной функции,
                    // чтобы избежать rust-analyzer-проблемы с
                    // format! в сложной позиции.
                    let info = closed_info(code, &reason, was_clean);
                    cb(TransportEvent::Disconnected(info));
                }
                WsEvent::Binary(v) => cb(TransportEvent::Bytes(v)),
                WsEvent::Text(s) => cb(TransportEvent::Bytes(s.into_bytes())),
                WsEvent::Error(e) => cb(TransportEvent::Error(e)),
                WsEvent::Reconnecting { attempt, delay_ms } => {
                    let msg = reconnecting_info(attempt, delay_ms);
                    cb(TransportEvent::Error(msg));
                }
                WsEvent::ReconnectExhausted => {
                    cb(TransportEvent::Error(
                        "reconnect exhausted".to_string(),
                    ));
                }
            }
        });
        let client = WsClient::new(WsConfig::new(url), adapter);
        return Rc::new(Transport {
            kind: TransportKind::WebSocket,
            backend: Backend::WebSocket(client),
        });
    }

    pub fn wt(url: &str, on_event: Rc<dyn Fn(TransportEvent)>) -> Rc<Transport> {
        let cb = on_event.clone();
        let adapter: Rc<dyn Fn(WtEvent)> = Rc::new(move |ev: WtEvent| {
            match ev {
                WtEvent::Ready => cb(TransportEvent::Connected),
                WtEvent::Closed { reason } => {
                    cb(TransportEvent::Disconnected(reason));
                }
                WtEvent::Datagram(v) => cb(TransportEvent::Bytes(v)),
                WtEvent::StreamData(v) => cb(TransportEvent::Bytes(v)),
                WtEvent::Error(e) => cb(TransportEvent::Error(e)),
                WtEvent::Text(s) => cb(TransportEvent::Error(s)),
                WtEvent::Connecting | WtEvent::Stats => {}
            }
        });
        let client = WtClient::new(WtConfig::new(url), adapter);
        return Rc::new(Transport {
            kind: TransportKind::WebTransport,
            backend: Backend::WebTransport(client),
        });
    }

    pub fn kind(&self) -> TransportKind {
        return self.kind;
    }

    pub fn state(&self) -> TransportState {
        return match &self.backend {
            Backend::WebSocket(c) => match c.state() {
                WsState::Connecting => TransportState::Connecting,
                WsState::Open => TransportState::Ready,
                WsState::Closing => TransportState::Connecting,
                WsState::Closed => TransportState::Closed,
            },
            Backend::WebTransport(c) => match c.state() {
                WtState::Idle => TransportState::Idle,
                WtState::Connecting => TransportState::Connecting,
                WtState::Ready => TransportState::Ready,
                WtState::Closed => TransportState::Closed,
                WtState::Failed => TransportState::Failed,
            },
        };
    }

    pub fn connect(&self) -> Result<(), String> {
        return match &self.backend {
            Backend::WebSocket(c) => c
                .connect()
                .map_err(|e| format_str(format_args!("ws connect: {e:?}"))),
            Backend::WebTransport(c) => c
                .connect()
                .map_err(|e| format_str(format_args!("wt connect: {e:?}"))),
        };
    }

    pub fn send_binary(&self, bytes: &[u8]) -> Result<(), String> {
        return match &self.backend {
            Backend::WebSocket(c) => c
                .send_binary(bytes)
                .map_err(|e| format_str(format_args!("ws send: {e:?}"))),
            Backend::WebTransport(c) => c
                .send_datagram(bytes)
                .map_err(|e| format_str(format_args!("wt send: {e:?}"))),
        };
    }

    pub fn close(&self) {
        match &self.backend {
            Backend::WebSocket(c) => c.close(),
            Backend::WebTransport(c) => c.close(),
        }
    }
}

// ============================================================
//                    HELPERS
// ============================================================

/// Формирует строку для WsEvent::Closed в отдельной функции —
/// иначе rust-analyzer теряет тип у format! внутри if/else.
fn closed_info(code: u16, reason: &str, was_clean: bool) -> String {
    if reason.is_empty() {
        return format_str(format_args!(
            "closed code={code} clean={was_clean}",
        ));
    }
    return format_str(format_args!(
        "closed code={code} reason={reason} clean={was_clean}",
    ));
}

/// Формирует строку для WsEvent::Reconnecting.
fn reconnecting_info(attempt: u32, delay_ms: u32) -> String {
    return format_str(format_args!(
        "reconnecting: attempt={attempt} delay={delay_ms}мс",
    ));
}