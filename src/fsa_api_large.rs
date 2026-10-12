//! Работа с большими файлами.
//!
//! * adaptive_chunk_size(total) — размер чанка в границах [64 КиБ, 4 МиБ].
//! * copy_file(src, dst, on_progress, is_cancelled) — копирование
//!   файла в файл с прогрессом.

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use web_sys::{File, FileSystemFileHandle};

/// Размер чанка, адаптирующийся под объём.
pub fn adaptive_chunk_size(total: usize) -> usize {
    let target_iters = 200usize;
    (total / target_iters).clamp(64 * 1024, 4 * 1024 * 1024)
}

pub enum CopyOutcome {
    Ok(u64),
    Cancelled,
}

/// Копирование содержимого File в существующий FileSystemFileHandle.
pub async fn copy_file(
    src: &File,
    dst: &FileSystemFileHandle,
    mut on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<CopyOutcome, JsValue> {
    let stream = crate::fsa_api_file::create_writable(dst, false).await?;
    let total = src.size().max(1.0);
    let reader = web_sys::ReadableStreamDefaultReader::new(&src.stream())?;
    let mut written: f64 = 0.0;
    let mut last_yield = 0.0_f64;

    loop {
        if is_cancelled() {
            let _ = crate::fsa_api_writable::close(&stream).await;
            return Ok(CopyOutcome::Cancelled);
        }
        let result = wasm_bindgen_futures::JsFuture::from(reader.read()).await?;
        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
            .as_bool()
            .unwrap_or(true);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
        let chunk = js_sys::Uint8Array::new(&value);
        let bytes = chunk.to_vec();
        written += bytes.len() as f64;
        crate::fsa_api_writable::write_bytes(&stream, &bytes).await?;

        let now = js_sys::Date::now();
        if now - last_yield >= 16.0 {
            on_progress(written / total);
            TimeoutFuture::new(0).await;
            last_yield = now;
        }
    }
    crate::fsa_api_writable::close(&stream).await?;
    on_progress(1.0);
    Ok(CopyOutcome::Ok(written as u64))
}