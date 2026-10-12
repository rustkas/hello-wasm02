//! Хеширование файлов: SHA-256 через SubtleCrypto и через crate `sha2`.
//!
//! Два потоковых подхода:
//!
//! * `sha256_stream_cancellable` — slice-based. Файл режется на чанки
//!   через `Blob.slice`, каждый читается `Blob::array_buffer()`. Размер
//!   чанка контролируем мы, что даёт минимум Rust↔JS переходов.
//! * `sha256_stream_native_cancellable` — stream-based. Использует
//!   `Blob.stream()` и `ReadableStreamDefaultReader`. Браузер сам
//!   решает размер чанка (обычно 64 КБ) и отдаёт данные по мере
//!   поступления от диска, без создания промежуточных Blob-объектов.
//!
//! Оба yield'ят через `MessageChannel` — это не throttлится в фоне,
//! в отличие от `setTimeout`. Это критично: Chrome режет `setTimeout(0)`
//! до 1000 мс в неактивных вкладках, и на 4 ГБ файле yield съедает
//! больше времени, чем само хеширование.

use std::cell::Cell;
use std::rc::Rc;

use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, File, ReadableStreamDefaultReader};

// ---------------------------------------------------------------
// Public API: хеши через sha2 и SubtleCrypto
// ---------------------------------------------------------------

/// Считает SHA-256 от среза байт через crate `sha2` (синхронно).
pub fn sha256_via_sha2(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    return hex::encode(hash.as_slice());
}

/// Считает SHA-256 от среза байт через `crypto.subtle.digest` (асинхронно).
pub async fn sha256_via_subtle(data: &[u8]) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let crypto = window
        .crypto()
        .map_err(|_| JsValue::from_str("no crypto"))?;
    let subtle = crypto.subtle();

    let uint8 = js_sys::Uint8Array::new_from_slice(data);
    let obj: &js_sys::Object = uint8.as_ref();
    let promise = subtle.digest_with_str_and_buffer_source("SHA-256", obj)?;
    let value = JsFuture::from(promise).await?;
    let array_buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("subtle.digest вернул не ArrayBuffer"))?;
    let result = js_sys::Uint8Array::new(&array_buffer);
    return Ok(hex::encode(result.to_vec()));
}

// ---------------------------------------------------------------
// Public API: потоковые версии
// ---------------------------------------------------------------

/// Slice-based streaming без отмены (обёртка для тестов).
pub async fn sha256_stream_with_progress<F>(
    blob: &Blob,
    chunk_size: u32,
    on_progress: F,
) -> Result<String, JsValue>
where
    F: FnMut(f64, f64),
{
    let never = Rc::new(Cell::new(false));
    let result = sha256_stream_cancellable(blob, chunk_size, never, on_progress).await?;
    return result.ok_or_else(|| JsValue::from_str("неожиданная отмена"));
}

/// Slice-based streaming с отменой.
///
/// Файл режется на чанки по `chunk_size` через `Blob.slice`. Каждый
/// чанк читается `Blob::array_buffer()`. Большие чанки (4–16 МБ)
/// уменьшают число Rust↔JS переходов, что критично для I/O-bound
/// сценариев.
pub async fn sha256_stream_cancellable<F>(
    blob: &Blob,
    chunk_size: u32,
    cancel: Rc<Cell<bool>>,
    mut on_progress: F,
) -> Result<Option<String>, JsValue>
where
    F: FnMut(f64, f64),
{
    let mut hasher = Sha256::new();
    let size = blob.size();
    let chunk_size_f = chunk_size as f64;
    let mut offset = 0.0_f64;

    on_progress(0.0, size);

    let mut last_yield_ms = js_sys::Date::now();

    while offset < size {
        if cancel.get() {
            return Ok(None);
        }

        let end = (offset + chunk_size_f).min(size);
        let slice = blob.slice_with_f64_and_f64(offset, end)?;
        let bytes = read_blob_bytes(&slice).await?;

        if cancel.get() {
            return Ok(None);
        }

        hasher.update(&bytes);
        offset = end;
        on_progress(offset, size);

        let now = js_sys::Date::now();
        if now - last_yield_ms >= 16.0 {
            yield_via_message_channel().await?;
            last_yield_ms = js_sys::Date::now();
        }
    }

    let hash = hasher.finalize();
    return Ok(Some(hex::encode(hash.as_slice())));
}

/// Native stream-based streaming с отменой.
///
/// Использует `Blob.stream()` — браузер сам режет файл на чанки
/// (обычно ~64 КБ) и отдаёт их по мере поступления от диска.
/// Никаких промежуточных `Blob`-объектов не создаётся, переходов
/// между Rust и JS меньше по накладным расходам (каждый чанк — 
/// уже готовый `Uint8Array` из `ReadableStream`).
///
/// Размер чанка не контролируется — это свойство браузера.
/// Если нужен контроль, используйте slice-based версию.
pub async fn sha256_stream_native_cancellable<F>(
    blob: &Blob,
    cancel: Rc<Cell<bool>>,
    mut on_progress: F,
) -> Result<Option<String>, JsValue>
where
    F: FnMut(f64, f64),
{
    let mut hasher = Sha256::new();
    let size = blob.size();
    let mut offset = 0.0_f64;

    on_progress(0.0, size);

    let stream = blob.stream();
    let reader_js = stream.get_reader();
    let reader: ReadableStreamDefaultReader = reader_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ReadableStreamDefaultReader"))?;

    let mut last_yield_ms = js_sys::Date::now();

    loop {
        if cancel.get() {
            return Ok(None);
        }

        let read_promise = reader.read();
        let value = JsFuture::from(read_promise).await?;

        // Результат `reader.read()` — plain JS-объект `{ done, value }`,
        // а не инстанс `ReadableStreamReadResult`. Читаем через Reflect.
        let obj: &js_sys::Object = match value.dyn_ref() {
            Some(o) => o,
            None => {
                return Err(JsValue::from_str("read() вернул не Object"));
            }
        };
        let done_js = js_sys::Reflect::get(obj, &JsValue::from_str("done"))?;
        let done = done_js.as_bool().unwrap_or(false);
        if done {
            break;
        }

        if cancel.get() {
            return Ok(None);
        }

        let chunk_js = js_sys::Reflect::get(obj, &JsValue::from_str("value"))?;
        let uint8: js_sys::Uint8Array = chunk_js
            .dyn_into()
            .map_err(|_| JsValue::from_str("чанк не Uint8Array"))?;
        let bytes = uint8.to_vec();

        hasher.update(&bytes);
        offset += bytes.len() as f64;
        on_progress(offset, size);

        let now = js_sys::Date::now();
        if now - last_yield_ms >= 16.0 {
            yield_via_message_channel().await?;
            last_yield_ms = js_sys::Date::now();
        }
    }

    let hash = hasher.finalize();
    return Ok(Some(hex::encode(hash.as_slice())));
}

/// Быстрый отпечаток файла — размер и дата изменения.
pub fn quick_fingerprint(file: &File) -> String {
    return format!(
        "{} байт, изменён {}",
        file.size() as u64,
        file.last_modified() as u64,
    );
}

// ---------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------

/// Читает Blob целиком как `Vec<u8>` через `Blob::array_buffer()`.
async fn read_blob_bytes(blob: &Blob) -> Result<Vec<u8>, JsValue> {
    let promise = blob.array_buffer();
    let value = JsFuture::from(promise).await?;
    let array_buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    return Ok(js_sys::Uint8Array::new(&array_buffer).to_vec());
}

/// Отдаёт управление браузеру через MessageChannel.
///
/// В отличие от `setTimeout(0)`, не throttлится в фоновых вкладках:
/// Chrome режет `setTimeout` до 1000 мс, а `postMessage` между
/// портами MessageChannel работает без задержек.
async fn yield_via_message_channel() -> Result<(), JsValue> {
    let channel = web_sys::MessageChannel::new()?;
    let port1 = channel.port1();
    let port2 = channel.port2();

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let resolve_c = resolve.clone();
        let cb = Closure::once_into_js(move |_event: web_sys::MessageEvent| {
            let _ = resolve_c.call0(&JsValue::NULL);
        });
        port1.set_onmessage(Some(cb.unchecked_ref()));
        port2.post_message(&JsValue::NULL).ok();
    });

    JsFuture::from(promise).await?;
    return Ok(());
}