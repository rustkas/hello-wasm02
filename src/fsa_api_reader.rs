//! Чтение файлов.
//!
//! * read_text(file) → String
//! * read_bytes(file) → Vec<u8>
//! * read_slice(file, start, end) → Vec<u8>
//! * read_stream(file, on_progress, is_cancelled) — потоковое чтение
//!   с yield раз в 16 мс.

use gloo_timers::future::TimeoutFuture;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, ReadableStreamDefaultReader};

pub enum ReadOutcome {
    Ok(Vec<u8>),
    Cancelled,
}

/// file.text() — прочитать как UTF-8 строку.
pub async fn read_text(file: &File) -> Result<String, JsValue> {
    let promise: js_sys::Promise = file.text();
    let value = JsFuture::from(promise).await?;
    value
        .as_string()
        .ok_or_else(|| JsValue::from_str("не строка"))
}

/// file.arrayBuffer() — прочитать как Vec<u8>.
pub async fn read_bytes(file: &File) -> Result<Vec<u8>, JsValue> {
    let promise: js_sys::Promise = file.array_buffer();
    let value = JsFuture::from(promise).await?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let uint8 = Uint8Array::new(&buffer);
    Ok(uint8.to_vec())
}

/// file.slice(start, end).arrayBuffer() — чтение диапазона.
pub async fn read_slice(file: &File, start: f64, end: f64) -> Result<Vec<u8>, JsValue> {
    let slice = file.slice_with_f64_and_f64(start, end)?;
    let promise: js_sys::Promise = slice.array_buffer();
    let value = JsFuture::from(promise).await?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// Потоковое чтение через Blob.stream с прогрессом и отменой.
pub async fn read_stream(
    file: &File,
    mut on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<ReadOutcome, JsValue> {
    let stream = file.stream();
    let reader = ReadableStreamDefaultReader::new(&stream)?;
    let total = file.size().max(1.0);
    let mut received: f64 = 0.0;
    let mut chunks: Vec<u8> = Vec::new();
    let mut last_yield = 0.0_f64;

    loop {
        if is_cancelled() {
            return Ok(ReadOutcome::Cancelled);
        }
        let result = JsFuture::from(reader.read()).await?;
        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
            .as_bool()
            .unwrap_or(true);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
        let chunk = Uint8Array::new(&value);
        let len = chunk.length() as f64;
        chunks.extend_from_slice(&chunk.to_vec());
        received += len;

        let now = js_sys::Date::now();
        if now - last_yield >= 16.0 {
            on_progress(received / total);
            TimeoutFuture::new(0).await;
            last_yield = now;
        }
    }
    on_progress(1.0);
    Ok(ReadOutcome::Ok(chunks))
}