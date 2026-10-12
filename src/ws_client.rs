//! 36. WebSocket клиент.
//!
//! Обёртка над браузерным WebSocket:
//!   • жизненный цикл (Connecting → Open → Closing → Closed);
//!   • text- и binary-сообщения;
//!   • events через Rust-колбэк Fn(WsEvent);
//!   • стратегия reconnect с exponential backoff и jitter;
//!   • защита от лавины событий через токен поколения.
//!
//! # Про closures
//!
//! Все closures, навешиваемые на WebSocket, вызывают `forget()`.
//! Причина: JS-объект WebSocket переживает Rust-обёртку на несколько
//! GC-циклов. Если хранить closures в Vec и дропать их при пересоздании
//! сокета, получаем состояние «wasm-функция уничтожена, JS-ссылка жива»
//! → ошибка «closure invoked recursively or after being dropped»
//! при следующей установке обработчиков.
//!
//! `forget()` оставляет JS-функцию валидной навсегда. Небольшая утечка
//! (несколько сотен байт на reconnect) — приемлемая цена за корректность.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{BinaryType, CloseEvent, MessageEvent, WebSocket};

// ============================================================
//                    СОСТОЯНИЯ
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsState {
    Connecting,
    Open,
    Closing,
    Closed,
}

impl WsState {
    pub fn from_code(code: u16) -> Self {
        return match code {
            0 => WsState::Connecting,
            1 => WsState::Open,
            2 => WsState::Closing,
            3 => WsState::Closed,
            _ => WsState::Closed,
        };
    }

    pub fn as_str(self) -> &'static str {
        return match self {
            WsState::Connecting => "connecting",
            WsState::Open => "open",
            WsState::Closing => "closing",
            WsState::Closed => "closed",
        };
    }
}

// ============================================================
//                    СОБЫТИЯ
// ============================================================

#[derive(Clone, Debug)]
pub enum WsEvent {
    Opened,
    Text(String),
    Binary(Vec<u8>),
    Error(String),
    Closed {
        code: u16,
        reason: String,
        was_clean: bool,
    },
    Reconnecting { attempt: u32, delay_ms: u32 },
    ReconnectExhausted,
}

// ============================================================
//                    КОНФИГУРАЦИЯ
// ============================================================

#[derive(Clone, Debug)]
pub struct WsConfig {
    pub url: String,
    pub max_reconnect_attempts: u32,
    pub base_reconnect_ms: u32,
    pub max_reconnect_ms: u32,
    pub binary_as_arraybuffer: bool,
}

impl WsConfig {
    pub fn new(url: impl Into<String>) -> Self {
        return Self {
            url: url.into(),
            max_reconnect_attempts: 5,
            base_reconnect_ms: 500,
            max_reconnect_ms: 15_000,
            binary_as_arraybuffer: true,
        };
    }
}

// ============================================================
//                    СТРАТЕГИЯ RECONNECT
// ============================================================

pub fn backoff_delay_ms(attempt: u32, base_ms: u32, max_ms: u32) -> u32 {
    if attempt == 0 {
        return base_ms.min(max_ms);
    }
    let shift = attempt.saturating_sub(1).min(16);
    let factor = 1u32.checked_shl(shift).unwrap_or(u32::MAX);
    let raw = base_ms.saturating_mul(factor);
    let capped = raw.min(max_ms);
    let jitter_range = (capped / 5).max(1);
    let jitter = random_i32(jitter_range as i32);
    let with_jitter = (capped as i64 + jitter as i64).max(1) as u32;
    return with_jitter.min(max_ms);
}

fn random_i32(limit: i32) -> i32 {
    let r = js_sys::Math::random();
    let span = (limit as f64) * 2.0;
    return (r * span - limit as f64).round() as i32;
}

// ============================================================
//                    INNER
// ============================================================

struct Inner {
    config: WsConfig,
    ws: Option<WebSocket>,
    generation: u32,
    reconnect_attempt: u32,
    should_reconnect: bool,
    reconnect_timeout: Option<Timeout>,
    state: WsState,
}

// ============================================================
//                    КЛИЕНТ
// ============================================================

pub struct WsClient {
    inner: Rc<RefCell<Inner>>,
    on_event: Rc<dyn Fn(WsEvent)>,
}

impl WsClient {
    pub fn new(config: WsConfig, on_event: Rc<dyn Fn(WsEvent)>) -> Rc<Self> {
        let inner = Rc::new(RefCell::new(Inner {
            config,
            ws: None,
            generation: 0,
            reconnect_attempt: 0,
            should_reconnect: false,
            reconnect_timeout: None,
            state: WsState::Closed,
        }));
        return Rc::new(Self { inner, on_event });
    }

    pub fn state(&self) -> WsState {
        return self.inner.borrow().state;
    }

    pub fn config(&self) -> WsConfig {
        return self.inner.borrow().config.clone();
    }

    pub fn connect(self: &Rc<Self>) -> Result<(), JsValue> {
        {
            let mut inner = self.inner.borrow_mut();
            inner.should_reconnect = true;
            inner.reconnect_attempt = 0;
            inner.reconnect_timeout = None;
            inner.state = WsState::Connecting;
        }
        return open_socket(&self.inner, self.on_event.clone());
    }

    pub fn close(&self) {
        let ws_opt = {
            let mut inner = self.inner.borrow_mut();
            inner.should_reconnect = false;
            inner.reconnect_timeout = None;
            inner.ws.clone()
        };
        // Borrow отпущен до JS-вызова: onclose может сработать
        // синхронно и попытаться взять RefCell.
        if let Some(ws) = ws_opt {
            if ws.ready_state() == WebSocket::OPEN
                || ws.ready_state() == WebSocket::CONNECTING
            {
                let _ = ws.close();
            }
        }
    }

    pub fn send_text(&self, text: &str) -> Result<(), JsValue> {
        let ws = {
            let inner = self.inner.borrow();
            let Some(ws) = inner.ws.as_ref() else {
                return Err(JsValue::from_str("no socket"));
            };
            if ws.ready_state() != WebSocket::OPEN {
                return Err(JsValue::from_str("socket not open"));
            }
            ws.clone()
        };
        return ws.send_with_str(text);
    }

    pub fn send_binary(&self, bytes: &[u8]) -> Result<(), JsValue> {
        let ws = {
            let inner = self.inner.borrow();
            let Some(ws) = inner.ws.as_ref() else {
                return Err(JsValue::from_str("no socket"));
            };
            if ws.ready_state() != WebSocket::OPEN {
                return Err(JsValue::from_str("socket not open"));
            }
            ws.clone()
        };
        return ws.send_with_u8_array(bytes);
    }

    pub fn simulate_drop(&self) -> Result<(), JsValue> {
        let ws = {
            let inner = self.inner.borrow();
            let Some(ws) = inner.ws.as_ref() else {
                return Err(JsValue::from_str("no active socket"));
            };
            if ws.ready_state() != WebSocket::OPEN {
                return Err(JsValue::from_str("socket not open"));
            }
            ws.clone()
        };
        return ws.close_with_code_and_reason(4000, "simulate-drop");
    }
}

// ============================================================
//                    ОТКРЫТИЕ СОКЕТА
// ============================================================

fn open_socket(
    inner: &Rc<RefCell<Inner>>,
    on_event: Rc<dyn Fn(WsEvent)>,
) -> Result<(), JsValue> {
    let (url, binary_type, gen_id) = {
        let mut inner_ref = inner.borrow_mut();
        inner_ref.generation = inner_ref.generation.wrapping_add(1);
        let url = inner_ref.config.url.clone();
        let bt = if inner_ref.config.binary_as_arraybuffer {
            BinaryType::Arraybuffer
        } else {
            BinaryType::Blob
        };
        (url, bt, inner_ref.generation)
    };

    let ws = WebSocket::new(&url)?;
    ws.set_binary_type(binary_type);

    // ---- onopen ----
    {
        let weak = Rc::downgrade(inner);
        let cb = on_event.clone();
        let cl = Closure::<dyn FnMut(JsValue)>::new(move |_ev: JsValue| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.borrow().generation != gen_id {
                return;
            }
            {
                let mut i = inner.borrow_mut();
                i.state = WsState::Open;
                i.reconnect_attempt = 0;
            }
            cb(WsEvent::Opened);
        });
        ws.set_onopen(Some(cl.as_ref().unchecked_ref()));
        cl.forget();
    }

    // ---- onmessage ----
    {
        let weak = Rc::downgrade(inner);
        let cb = on_event.clone();
        let cl = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.borrow().generation != gen_id {
                return;
            }
            let Ok(msg) = ev.dyn_into::<MessageEvent>() else {
                return;
            };
            let data = msg.data();
            if let Some(text) = data.as_string() {
                cb(WsEvent::Text(text));
                return;
            }
            if let Ok(ab) = data.dyn_into::<ArrayBuffer>() {
                let u8 = Uint8Array::new(&ab);
                cb(WsEvent::Binary(u8.to_vec()));
                return;
            }
        });
        ws.set_onmessage(Some(cl.as_ref().unchecked_ref()));
        cl.forget();
    }

    // ---- onerror ----
    {
        let weak = Rc::downgrade(inner);
        let cb = on_event.clone();
        let cl = Closure::<dyn FnMut(JsValue)>::new(move |_ev: JsValue| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.borrow().generation != gen_id {
                return;
            }
            cb(WsEvent::Error("transport error".to_string()));
        });
        ws.set_onerror(Some(cl.as_ref().unchecked_ref()));
        cl.forget();
    }

    // ---- onclose ----
    {
        let weak = Rc::downgrade(inner);
        let cb = on_event.clone();
        let cl = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.borrow().generation != gen_id {
                return;
            }

            let (code, reason, was_clean) = if let Ok(ce) = ev.dyn_into::<CloseEvent>()
            {
                (ce.code(), ce.reason(), ce.was_clean())
            } else {
                (1006, String::new(), false)
            };

            {
                let mut i = inner.borrow_mut();
                i.state = WsState::Closed;
                i.ws = None;
                i.reconnect_timeout = None;
            }

            cb(WsEvent::Closed {
                code,
                reason,
                was_clean,
            });

            let (should, attempt, max_attempts, base, max_ms) = {
                let i = inner.borrow();
                (
                    i.should_reconnect,
                    i.reconnect_attempt + 1,
                    i.config.max_reconnect_attempts,
                    i.config.base_reconnect_ms,
                    i.config.max_reconnect_ms,
                )
            };

            if !should || max_attempts == 0 {
                return;
            }
            if attempt > max_attempts {
                cb(WsEvent::ReconnectExhausted);
                return;
            }

            let delay = backoff_delay_ms(attempt, base, max_ms);
            {
                let mut i = inner.borrow_mut();
                i.reconnect_attempt = attempt;
            }
            cb(WsEvent::Reconnecting {
                attempt,
                delay_ms: delay,
            });

            let weak2 = Rc::downgrade(&inner);
            let cb2 = cb.clone();
            let timeout = Timeout::new(delay, move || {
                let Some(inner) = weak2.upgrade() else { return };
                if !inner.borrow().should_reconnect {
                    return;
                }
                let _ = open_socket(&inner, cb2);
            });
            inner.borrow_mut().reconnect_timeout = Some(timeout);
        });
        ws.set_onclose(Some(cl.as_ref().unchecked_ref()));
        cl.forget();
    }

    // Сохраняем только WebSocket. Closures — forgotten, живут в JS.
    {
        let mut i = inner.borrow_mut();
        i.ws = Some(ws);
    }

    return Ok(());
}

// ============================================================
//                    УТИЛИТЫ
// ============================================================

/// Форматирует первые N байт как hex-строку.
pub fn hex_preview(bytes: &[u8], max_bytes: usize) -> String {
    let n = bytes.len().min(max_bytes);
    let mut out = String::with_capacity(n * 3 + 16);
    for (i, b) in bytes.iter().take(n).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:02x}", b));
    }
    if bytes.len() > max_bytes {
        out.push_str(" …");
    }
    return out;
}