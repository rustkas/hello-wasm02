//! Запись файлов.
//!
//! * write_all_bytes(handle, data) → имя файла
//! * write_all_text(handle, text) → имя файла
//! * write_chunked(handle, data, on_progress, is_cancelled) — запись
//!   чанками с yield раз в 16 мс.

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use web_sys::FileSystemFileHandle;

pub enum WriteOutcome {
    Ok,
    Cancelled,
}

/// Записать весь буфер за один вызов.
pub async fn write_all_bytes(
    handle: &FileSystemFileHandle,
    data: &[u8],
) -> Result<String, JsValue> {
    let stream = crate::fsa_api_file::create_writable(handle, false).await?;
    crate::fsa_api_writable::write_bytes(&stream, data).await?;
    crate::fsa_api_writable::close(&stream).await?;
    Ok(handle.name())
}

/// Записать текст.
pub async fn write_all_text(
    handle: &FileSystemFileHandle,
    text: &str,
) -> Result<String, JsValue> {
    let stream = crate::fsa_api_file::create_writable(handle, false).await?;
    crate::fsa_api_writable::write_text(&stream, text).await?;
    crate::fsa_api_writable::close(&stream).await?;
    Ok(handle.name())
}

/// Записать буфер чанками с прогрессом и отменой.
pub async fn write_chunked(
    handle: &FileSystemFileHandle,
    data: &[u8],
    mut on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<WriteOutcome, JsValue> {
    let stream = crate::fsa_api_file::create_writable(handle, false).await?;
    let total = data.len().max(1);
    let chunk_size = crate::fsa_api_large::adaptive_chunk_size(data.len());
    let mut last_yield = 0.0_f64;

    for (i, chunk) in data.chunks(chunk_size).enumerate() {
        if is_cancelled() {
            let _ = crate::fsa_api_writable::close(&stream).await;
            return Ok(WriteOutcome::Cancelled);
        }
        crate::fsa_api_writable::write_bytes(&stream, chunk).await?;
        let processed = ((i + 1) * chunk_size).min(data.len());
        let now = js_sys::Date::now();
        if now - last_yield >= 16.0 {
            on_progress(processed as f64 / total as f64);
            TimeoutFuture::new(0).await;
            last_yield = now;
        }
    }
    crate::fsa_api_writable::close(&stream).await?;
    on_progress(1.0);
    Ok(WriteOutcome::Ok)
}