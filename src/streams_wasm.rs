//! Работа с потоками через крейт `wasm-streams`.
//!
//! Крейт предоставляет обёртки над `web_sys::ReadableStream`,
//! `web_sys::WritableStream` и `web_sys::TransformStream`, а также
//! конвертеры между ними и Rust `Stream`/`Sink`.
//!
//! ## Про AsyncRead
//!
//! `wasm-streams::into_stream` не пробуждает Waker, когда приходит
//! следующий чанк. Поэтому для `AsyncRead` мы используем **наш**
//! `streams::js_to_rust_stream` (на mpsc) и свой адаптер
//! `StreamToAsyncRead`.

use futures::StreamExt;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_streams::readable::ReadableStream as WasmReadableStream;
use wasm_streams::transform::TransformStream as WasmTransformStream;
use wasm_streams::writable::WritableStream as WasmWritableStream;

use crate::fetch::{classify_error, FetchError};

// ============================================================
// 17.5: JS → Rust Stream через wasm-streams
// ============================================================

/// Преобразует `web_sys::ReadableStream` в Rust `Stream` через `wasm-streams`.
///
/// Одна строка: `WasmReadableStream::from_raw(stream).into_stream()`.
/// Подходит для сценариев с `collect`, `filter_map`, `map`. Не
/// подходит для `AsyncRead` — см. `js_to_rust_stream_mpsc`.
pub fn js_to_rust_stream(
    stream: web_sys::ReadableStream,
) -> impl futures::Stream<Item = Result<Vec<u8>, FetchError>> {
    let wrapped = WasmReadableStream::from_raw(stream);
    let stream = wrapped.into_stream();

    stream.map(|result: Result<JsValue, JsValue>| {
        result
            .map(|chunk: JsValue| {
                if let Ok(arr) = chunk.clone().dyn_into::<js_sys::Uint8Array>() {
                    arr.to_vec()
                } else if let Some(s) = chunk.as_string() {
                    s.into_bytes()
                } else {
                    Vec::new()
                }
            })
            .map_err(classify_error)
    })
}

/// Преобразует `web_sys::ReadableStream` в Rust `Stream` через `mpsc`.
///
/// В отличие от `js_to_rust_stream` (через `wasm-streams`), эта
/// версия использует свой mpsc-канал и корректно пробуждает Waker.
/// Работает всегда — в том числе в связке с `AsyncRead`.
///
/// Это обёртка над `crate::streams::js_to_rust_stream`, добавленная
/// для симметрии: обе конвертации доступны из одного модуля.
pub fn js_to_rust_stream_mpsc(
    stream: web_sys::ReadableStream,
) -> impl futures::Stream<Item = Result<Vec<u8>, FetchError>> {
    crate::streams::js_to_rust_stream(stream)
}

// ============================================================
// 17.5: Rust Stream → JS ReadableStream
// ============================================================

/// Преобразует Rust `Stream` в `web_sys::ReadableStream`.
///
/// `WasmReadableStream::from_stream` создаёт JS-поток из Rust-потока.
/// `into_raw` возвращает исходный `web_sys::ReadableStream`, который
/// можно передать в `fetch`, `pipeThrough` или куда угодно ещё.
pub fn rust_stream_to_js<S>(stream: S) -> web_sys::ReadableStream
where
    S: futures::Stream<Item = Result<JsValue, JsValue>> + 'static,
{
    let wrapped = WasmReadableStream::from_stream(stream);
    wrapped.into_raw()
}

// ============================================================
// 17.5: JS WritableStream → Rust Sink
// ============================================================

/// Преобразует `web_sys::WritableStream` в Rust `Sink`.
///
/// `into_sink` даёт `futures::Sink<JsValue, Error = JsValue>`.
/// Дальше работают `send`, `feed`, `close` из `SinkExt`.
pub fn js_to_rust_sink(
    stream: web_sys::WritableStream,
) -> impl futures::Sink<JsValue, Error = JsValue> {
    let wrapped = WasmWritableStream::from_raw(stream);
    wrapped.into_sink()
}

// ============================================================
// 17.5: JS ReadableStream → Rust AsyncRead
// ============================================================

/// Адаптер: `futures::Stream<Item = Result<Vec<u8>, FetchError>>`
/// → `futures::io::AsyncRead`.
///
/// Работает поверх нашего mpsc-потока (`streams::js_to_rust_stream`).
/// `mpsc::UnboundedSender::send` корректно будит получателя, поэтому
/// `poll_next` не виснет.
pub struct StreamToAsyncRead<S> {
    stream: std::pin::Pin<Box<S>>,
    buffer: Vec<u8>,
    offset: usize,
}

impl<S> StreamToAsyncRead<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream: Box::pin(stream),
            buffer: Vec::new(),
            offset: 0,
        }
    }
}

impl<S> futures::io::AsyncRead for StreamToAsyncRead<S>
where
    S: futures::Stream<Item = Result<Vec<u8>, FetchError>>,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::task::Poll;

        let this = self.get_mut();

        // Отдаём остаток предыдущего чанка, если есть.
        if this.offset < this.buffer.len() {
            let n = (this.buffer.len() - this.offset).min(buf.len());
            buf[..n].copy_from_slice(&this.buffer[this.offset..this.offset + n]);
            this.offset += n;
            if this.offset >= this.buffer.len() {
                this.buffer.clear();
                this.offset = 0;
            }
            return Poll::Ready(Ok(n));
        }

        match this.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let n = bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                if n < bytes.len() {
                    this.buffer = bytes[n..].to_vec();
                    this.offset = 0;
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("stream error: {:?}", e),
            ))),
            Poll::Ready(None) => Poll::Ready(Ok(0)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Преобразует `web_sys::ReadableStream` в `futures::io::AsyncRead`.
///
/// Использует **наш** `streams::js_to_rust_stream` (на mpsc).
pub fn js_to_async_read(
    stream: web_sys::ReadableStream,
) -> Result<impl futures::io::AsyncRead, JsValue> {
    let rust_stream = crate::streams::js_to_rust_stream(stream);
    Ok(StreamToAsyncRead::new(rust_stream))
}

/// Читает весь `AsyncRead` в `Vec<u8>`.
///
/// Читаем в цикле фиксированными чанками: `read` возвращает 0 при EOF.
pub async fn read_async_read(
    mut reader: impl futures::io::AsyncRead + Unpin,
) -> Result<Vec<u8>, FetchError> {
    use futures::io::AsyncReadExt;

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(e) => {
                return Err(FetchError::Network(format!("AsyncRead error: {e}")));
            }
        }
    }

    Ok(buffer)
}

// ============================================================
// 17.5: TransformStream — обёртка
// ============================================================

/// Оборачивает `web_sys::TransformStream` в пару обёрток `wasm-streams`.
pub fn wrap_transform(
    transform: web_sys::TransformStream,
) -> (WasmReadableStream, WasmWritableStream) {
    let wrapped = WasmTransformStream::from_raw(transform);
    let readable = wrapped.readable();
    let writable = wrapped.writable();
    (readable, writable)
}

// ============================================================
// 17.5: duplex: "half" — отправка Rust-потока в теле fetch
// ============================================================

/// Готовит `RequestInit` для POST с потоковым телом (`duplex: "half"`).
///
/// **Требует Chrome 105+.** В Firefox и Safari на момент написания
/// книги не работает. Функция не используется в демо: цель —
/// показать, как передать Rust-поток в тело `fetch`.
///
/// Пример использования (только для поддерживающих браузеров):
///
/// ```ignore
/// let stream = futures::stream::iter(vec![...]);
/// let init = rust_stream_to_fetch_body(stream)?;
/// let request = web_sys::Request::new_with_str_and_init("/upload", &init)?;
/// let window = web_sys::window().unwrap();
/// let response = wasm_bindgen_futures::JsFuture::from(
///     window.fetch_with_request(&request),
/// ).await?;
/// ```
pub fn rust_stream_to_fetch_body<S>(stream: S) -> Result<web_sys::RequestInit, JsValue>
where
    S: futures::Stream<Item = Result<JsValue, JsValue>> + 'static,
{
    let js_stream = rust_stream_to_js(stream);
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&js_stream);

    // web-sys 0.3.105 не имеет типизированного set_duplex.
    // Устанавливаем через Reflect: init — это словарь, у него
    // есть метод as_ref() -> &JsValue.
    let init_js: &JsValue = init.as_ref();
    js_sys::Reflect::set(
        init_js,
        &JsValue::from_str("duplex"),
        &JsValue::from_str("half"),
    )?;

    Ok(init)
}