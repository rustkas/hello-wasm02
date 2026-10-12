//! 37. WebTransport клиент.
//!
//! Обёртка над браузерным WebTransport API:
//!   • надёжные потоки (unidirectional и bidirectional);
//!   • ненадёжные датаграммы;
//!   • lifecycle (ready → active → closed);
//!   • события через Rust-колбэк Fn(WtEvent).
//!
//! # WebTransport vs WebSocket
//!
//! WebSocket — одна надёжная упорядоченная труба.
//! WebTransport — фабрика потоков и датаграмм поверх QUIC:
//!   • multiplexing без head-of-line blocking;
//!   • выбор reliable / unreliable per-message;
//!   • быстрый handshake (0-RTT / 1-RTT).
//!
//! # Требования
//!
//! web-sys WebTransport помечен unstable. Требует:
//!   --cfg=web_sys_unstable_apis
//! Плюс HTTPS-соединение (secure context).
//!
//! # Особенности API web-sys
//!
//! Streams/Readers в разных версиях web-sys имеют несовместимые
//! сигнатуры (внутренний `Object<JsValue>` вместо `Result`).
//! Поэтому все вызовы методов у reader'ов делаем через
//! `Reflect::get` + `Function::call*` — независимо от версии.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    WebTransport, WebTransportBidirectionalStream,
    WebTransportDatagramDuplexStream, WebTransportOptions,
};

// ============================================================
//                    СОСТОЯНИЯ
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WtState {
    Idle,
    Connecting,
    Ready,
    Closed,
    Failed,
}

impl WtState {
    pub fn as_str(self) -> &'static str {
        return match self {
            WtState::Idle => "idle",
            WtState::Connecting => "connecting",
            WtState::Ready => "ready",
            WtState::Closed => "closed",
            WtState::Failed => "failed",
        };
    }
}

// ============================================================
//                    СОБЫТИЯ
// ============================================================

#[derive(Clone, Debug)]
pub enum WtEvent {
    Connecting,
    Ready,
    Text(String),
    Datagram(Vec<u8>),
    StreamData(Vec<u8>),
    Closed { reason: String },
    Error(String),
    Stats,
}

// ============================================================
//                    КОНФИГУРАЦИЯ
// ============================================================

#[derive(Clone, Debug)]
pub struct WtConfig {
    pub url: String,
    pub datagram_buffer: u32,
    pub use_streams: bool,
}

impl WtConfig {
    pub fn new(url: impl Into<String>) -> Self {
        return Self {
            url: url.into(),
            datagram_buffer: 32,
            use_streams: true,
        };
    }
}

// ============================================================
//                    СЧЁТЧИКИ
// ============================================================

#[derive(Default, Clone, Copy)]
pub struct WtStats {
    pub datagrams_sent: u64,
    pub datagrams_recv: u64,
    pub streams_opened: u64,
    pub streams_completed: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub errors: u64,
}

// ============================================================
//                    INNER
// ============================================================

struct Inner {
    config: WtConfig,
    wt: Option<WebTransport>,
    datagrams: Option<WebTransportDatagramDuplexStream>,
    generation: u32,
    state: WtState,
    stats: WtStats,
}

// ============================================================
//                    КЛИЕНТ
// ============================================================

pub struct WtClient {
    inner: Rc<RefCell<Inner>>,
    on_event: Rc<dyn Fn(WtEvent)>,
}

impl WtClient {
    pub fn new(config: WtConfig, on_event: Rc<dyn Fn(WtEvent)>) -> Rc<Self> {
        let inner = Rc::new(RefCell::new(Inner {
            config,
            wt: None,
            datagrams: None,
            generation: 0,
            state: WtState::Idle,
            stats: WtStats::default(),
        }));
        return Rc::new(Self { inner, on_event });
    }

    pub fn state(&self) -> WtState {
        return self.inner.borrow().state;
    }

    pub fn stats(&self) -> WtStats {
        return self.inner.borrow().stats;
    }

    pub fn config(&self) -> WtConfig {
        return self.inner.borrow().config.clone();
    }

    pub fn connect(self: &Rc<Self>) -> Result<(), JsValue> {
        {
            let mut inner = self.inner.borrow_mut();
            inner.generation = inner.generation.wrapping_add(1);
            inner.state = WtState::Connecting;
            inner.stats = WtStats::default();
        }
        let gen_id = self.inner.borrow().generation;
        // on_event — поле, не метод.
        (self.on_event)(WtEvent::Connecting);

        let url = self.inner.borrow().config.url.clone();
        let use_streams = self.inner.borrow().config.use_streams;
        let buffer = self.inner.borrow().config.datagram_buffer;

        let options = make_options(buffer)?;
        let wt = WebTransport::new_with_options(&url, &options)?;

        {
            let mut inner = self.inner.borrow_mut();
            inner.wt = Some(wt.clone());
        }

        let weak = Rc::downgrade(&self.inner);
        let cb = self.on_event.clone();
        spawn_local(async move {
            wait_ready_and_attach(weak, wt, cb, gen_id, use_streams).await;
        });

        return Ok(());
    }

    pub fn send_datagram(self: &Rc<Self>, bytes: &[u8]) -> Result<(), JsValue> {
        let datagrams = {
            let inner = self.inner.borrow();
            let Some(d) = inner.datagrams.as_ref() else {
                return Err(JsValue::from_str("not ready"));
            };
            d.clone()
        };

        let writable = datagrams.writable();
        let writer = call0(&writable.unchecked_ref::<JsValue>(), "getWriter")?;

        let u8_arr = Uint8Array::from(bytes);
        let promise_val = call1(
            &writer,
            "write",
            u8_arr.unchecked_ref::<JsValue>(),
        )?;

        spawn_local(async move {
            if let Ok(p) = promise_val.dyn_into::<js_sys::Promise>() {
                let _ = JsFuture::from(p).await;
            }
        });

        {
            let mut inner = self.inner.borrow_mut();
            inner.stats.datagrams_sent += 1;
            inner.stats.bytes_sent += bytes.len() as u64;
        }
        (self.on_event)(WtEvent::Stats);
        return Ok(());
    }

    pub fn send_over_stream(self: &Rc<Self>, bytes: &[u8]) -> Result<(), JsValue> {
        // Копируем в owned Vec для spawn_local ('static).
        let bytes_owned: Vec<u8> = bytes.to_vec();

        let wt = {
            let inner = self.inner.borrow();
            let Some(w) = inner.wt.as_ref() else {
                return Err(JsValue::from_str("not connected"));
            };
            w.clone()
        };

        let weak = Rc::downgrade(&self.inner);
        let cb = self.on_event.clone();
        spawn_local(async move {
            if let Err(e) = stream_send(wt, bytes_owned, weak.clone(), cb).await
            {
                if let Some(inner) = weak.upgrade() {
                    inner.borrow_mut().stats.errors += 1;
                }
                let _ = e;
            }
        });
        return Ok(());
    }

    pub fn close(&self) {
        let wt_opt = {
            let mut inner = self.inner.borrow_mut();
            inner.state = WtState::Closed;
            inner.wt.clone()
        };
        if let Some(wt) = wt_opt {
            let _ = wt.close();
        }
    }
}

// ============================================================
//                    ASYNC HELPERS
// ============================================================

async fn wait_ready_and_attach(
    weak: Weak<RefCell<Inner>>,
    wt: WebTransport,
    cb: Rc<dyn Fn(WtEvent)>,
    gen_id: u32,
    use_streams: bool,
) {
    let ready_promise = wt.ready();
    let ready_result = JsFuture::from(ready_promise).await;

    let Some(inner) = weak.upgrade() else { return };
    if inner.borrow().generation != gen_id {
        return;
    }

    match ready_result {
        Ok(_) => {
            {
                let mut i = inner.borrow_mut();
                i.state = WtState::Ready;
                i.datagrams = Some(wt.datagrams());
            }
            cb(WtEvent::Ready);

            spawn_datagram_reader(weak.clone(), wt.clone(), cb.clone(), gen_id);
            if use_streams {
                spawn_incoming_streams(
                    weak.clone(),
                    wt.clone(),
                    cb.clone(),
                    gen_id,
                );
            }
        }
        Err(e) => {
            // ready() rejected — соединение не установлено.
            // НЕ ждём closed(): он либо зависнет, либо повторит
            // ту же ошибку. Состояние — Failed, дальше ждём
            // действий пользователя.
            {
                let mut i = inner.borrow_mut();
                i.state = WtState::Failed;
                i.stats.errors += 1;
                i.wt = None;
                i.datagrams = None;
            }
            cb(WtEvent::Error(format_err("connect", &e)));
            return;
        }
    }

    // Ждём closed только при успешном open.
    let closed_promise = match get_closed_promise(&wt) {
        Ok(p) => p,
        Err(_) => return,
    };
    let close_reason = match JsFuture::from(closed_promise).await {
        Ok(v) => extract_reason(&v),
        Err(e) => format_err("closed", &e),
    };

    let Some(inner) = weak.upgrade() else { return };
    if inner.borrow().generation != gen_id {
        return;
    }
    {
        let mut i = inner.borrow_mut();
        i.state = WtState::Closed;
        i.wt = None;
        i.datagrams = None;
    }
    cb(WtEvent::Closed {
        reason: close_reason,
    });
}

fn get_closed_promise(wt: &WebTransport) -> Result<js_sys::Promise, JsValue> {
    let v = Reflect::get(
        wt.unchecked_ref::<JsValue>(),
        &JsValue::from_str("closed"),
    )?;
    return v
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| JsValue::from_str("closed is not a Promise"));
}

fn spawn_datagram_reader(
    weak: Weak<RefCell<Inner>>,
    wt: WebTransport,
    cb: Rc<dyn Fn(WtEvent)>,
    gen_id: u32,
) {
    let datagrams = wt.datagrams();
    let readable = datagrams.readable();

    // НЕ используем typed `readable.get_reader()` — эта сигнатура
    // в разных версиях web-sys даёт несовместимые типы. Идём через
    // Reflect: тот же JS-метод, но без типовой привязки.
    let readable_js: JsValue = readable.unchecked_ref::<JsValue>().clone();
    let reader = match call0(&readable_js, "getReader") {
        Ok(r) => r,
        Err(_) => return,
    };

    spawn_local(async move {
        loop {
            {
                let Some(inner) = weak.upgrade() else { return };
                if inner.borrow().generation != gen_id {
                    return;
                }
            }

            let promise_val = match call0(&reader, "read") {
                Ok(p) => p,
                Err(_) => return,
            };
            let promise: js_sys::Promise = match promise_val.dyn_into() {
                Ok(p) => p,
                Err(_) => return,
            };
            let result = match JsFuture::from(promise).await {
                Ok(v) => v,
                Err(_) => return,
            };
            let done = Reflect::get(&result, &JsValue::from_str("done"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if done {
                return;
            }
            let value = match Reflect::get(&result, &JsValue::from_str("value"))
            {
                Ok(v) => v,
                Err(_) => return,
            };
            let bytes = extract_bytes(&value);

            {
                let Some(inner) = weak.upgrade() else { return };
                let mut i = inner.borrow_mut();
                i.stats.datagrams_recv += 1;
                i.stats.bytes_recv += bytes.len() as u64;
            }
            cb(WtEvent::Datagram(bytes));
            cb(WtEvent::Stats);
        }
    });
}

fn spawn_incoming_streams(
    weak: Weak<RefCell<Inner>>,
    wt: WebTransport,
    cb: Rc<dyn Fn(WtEvent)>,
    gen_id: u32,
) {
    let wt_js: JsValue = wt.unchecked_ref::<JsValue>().clone();
    let incoming = match Reflect::get(
        &wt_js,
        &JsValue::from_str("incomingBidirectionalStreams"),
    ) {
        Ok(v) => v,
        Err(_) => return,
    };
    let reader = match call0(&incoming, "getReader") {
        Ok(r) => r,
        Err(_) => return,
    };

    spawn_local(async move {
        loop {
            {
                let Some(inner) = weak.upgrade() else { return };
                if inner.borrow().generation != gen_id {
                    return;
                }
            }

            let promise_val = match call0(&reader, "read") {
                Ok(p) => p,
                Err(_) => return,
            };
            let promise: js_sys::Promise = match promise_val.dyn_into() {
                Ok(p) => p,
                Err(_) => return,
            };
            let result = match JsFuture::from(promise).await {
                Ok(v) => v,
                Err(_) => return,
            };
            let done = Reflect::get(&result, &JsValue::from_str("done"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if done {
                return;
            }
            let stream = match Reflect::get(
                &result,
                &JsValue::from_str("value"),
            ) {
                Ok(v) => v,
                Err(_) => return,
            };
            let stream: WebTransportBidirectionalStream =
                match stream.dyn_into() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
            let weak2 = weak.clone();
            let cb2 = cb.clone();
            spawn_local(async move {
                read_bidi_stream(stream, weak2, cb2).await;
            });
        }
    });
}

async fn read_bidi_stream(
    stream: WebTransportBidirectionalStream,
    weak: Weak<RefCell<Inner>>,
    cb: Rc<dyn Fn(WtEvent)>,
) {
    let readable = stream.readable();
    let readable_js: JsValue = readable.unchecked_ref::<JsValue>().clone();
    let reader = match call0(&readable_js, "getReader") {
        Ok(r) => r,
        Err(_) => return,
    };

    let mut all_bytes: Vec<u8> = Vec::new();
    loop {
        let promise_val = match call0(&reader, "read") {
            Ok(p) => p,
            Err(_) => break,
        };
        let promise: js_sys::Promise = match promise_val.dyn_into() {
            Ok(p) => p,
            Err(_) => break,
        };
        let result = match JsFuture::from(promise).await {
            Ok(v) => v,
            Err(_) => break,
        };
        let done = Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if done {
            break;
        }
        let value = match Reflect::get(&result, &JsValue::from_str("value"))
        {
            Ok(v) => v,
            Err(_) => break,
        };
        let chunk = extract_bytes(&value);
        all_bytes.extend_from_slice(&chunk);
    }

    if !all_bytes.is_empty() {
        let Some(inner) = weak.upgrade() else { return };
        {
            let mut i = inner.borrow_mut();
            i.stats.streams_completed += 1;
            i.stats.bytes_recv += all_bytes.len() as u64;
        }
        cb(WtEvent::StreamData(all_bytes));
        cb(WtEvent::Stats);
    }
}

async fn stream_send(
    wt: WebTransport,
    bytes: Vec<u8>,
    weak: Weak<RefCell<Inner>>,
    cb: Rc<dyn Fn(WtEvent)>,
) -> Result<(), JsValue> {
    let wt_js: JsValue = wt.unchecked_ref::<JsValue>().clone();
    let stream = call0(&wt_js, "createBidirectionalStream")?;
    let stream: js_sys::Promise = stream
        .dyn_into()
        .map_err(|_| JsValue::from_str("create returned non-Promise"))?;
    let stream_val = JsFuture::from(stream).await?;
    let stream: WebTransportBidirectionalStream = stream_val.dyn_into()?;

    {
        let Some(inner) = weak.upgrade() else {
            return Ok(());
        };
        inner.borrow_mut().stats.streams_opened += 1;
    }

    let writable = stream.writable();
    let writable_js: JsValue = writable.unchecked_ref::<JsValue>().clone();
    let writer = call0(&writable_js, "getWriter")?;

    let u8_arr = Uint8Array::from(bytes.as_slice());
    let write_promise_val = call1(
        &writer,
        "write",
        u8_arr.unchecked_ref::<JsValue>(),
    )?;
    let write_promise: js_sys::Promise = write_promise_val
        .dyn_into()
        .map_err(|_| JsValue::from_str("write returned non-Promise"))?;
    let _ = JsFuture::from(write_promise).await;

    // close на writer — не критично, если не сработает.
    let _ = call0(&writer, "close");

    if let Some(inner) = weak.upgrade() {
        let mut i = inner.borrow_mut();
        i.stats.bytes_sent += bytes.len() as u64;
    }

    cb(WtEvent::Text(format!(
        "stream отправлен: {} байт",
        bytes.len()
    )));
    cb(WtEvent::Stats);
    return Ok(());
}

// ============================================================
//                    JS CALL HELPERS
// ============================================================

/// Вызов `target.method()` без аргументов через Reflect.
///
/// Используется для методов Streams API, сигнатуры которых в
/// web-sys зависят от версии (`getReader`, `read`, `write`,
/// `close`). Работает всегда.
fn call0(target: &JsValue, method: &str) -> Result<JsValue, JsValue> {
    let f = Reflect::get(target, &JsValue::from_str(method))
        .map_err(|_| JsValue::from_str(method))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| JsValue::from_str(method))?;
    return f.call0(target).map_err(|_| JsValue::from_str(method));
}

/// Вызов `target.method(arg)` с одним аргументом через Reflect.
fn call1(target: &JsValue, method: &str, arg: &JsValue) -> Result<JsValue, JsValue> {
    let f = Reflect::get(target, &JsValue::from_str(method))
        .map_err(|_| JsValue::from_str(method))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| JsValue::from_str(method))?;
    return f.call1(target, arg).map_err(|_| JsValue::from_str(method));
}

// ============================================================
//                    UTILITIES
// ============================================================

/// Создаём options как plain js_sys::Object.
///
/// `WebTransportOptions::new()` в разных версиях web-sys даёт
/// разные сигнатуры. Dictionary-типы хранятся как generic
/// `Object<JsValue>`, из-за чего `.unchecked_ref::<JsValue>()`
/// не срабатывает. Оборачиваем через `js_sys::Object` и
/// `unchecked_into`.
fn make_options(datagram_buffer: u32) -> Result<WebTransportOptions, JsValue> {
    let opts = js_sys::Object::new();
    let opts_ref: &JsValue = opts.unchecked_ref();
    let _ = Reflect::set(
        opts_ref,
        &JsValue::from_str("datagramIncomingHighWaterMark"),
        &JsValue::from_f64(datagram_buffer as f64),
    );
    return Ok(opts.unchecked_into::<WebTransportOptions>());
}

fn extract_bytes(value: &JsValue) -> Vec<u8> {
    if let Ok(u8arr) = value.clone().dyn_into::<Uint8Array>() {
        return u8arr.to_vec();
    }
    if let Ok(buf) = value.clone().dyn_into::<js_sys::ArrayBuffer>() {
        let u8arr = Uint8Array::new(&buf);
        return u8arr.to_vec();
    }
    return Vec::new();
}

fn extract_reason(v: &JsValue) -> String {
    if v.is_undefined() || v.is_null() {
        return String::from("normal");
    }
    if let Some(s) = v.as_string() {
        return s;
    }
    if let Ok(msg) = Reflect::get(v, &JsValue::from_str("message")) {
        if let Some(s) = msg.as_string() {
            return s;
        }
    }
    return String::from("closed");
}

fn format_err(context: &str, e: &JsValue) -> String {
    let name = Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let msg = Reflect::get(e, &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| format!("{:?}", e));
    return format!("{context}: {name} — {msg}");
}

/// Форматирование байтов для UI.
pub fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    if n < 1024 * 1024 {
        return format!("{:.1} KB", n as f64 / 1024.0);
    }
    return format!("{:.2} MB", n as f64 / (1024.0 * 1024.0));
}