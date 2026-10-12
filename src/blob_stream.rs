//! Blob.stream() — потоковое чтение больших файлов.
//!
//! Модуль автономен: не зависит от других модулей проекта.
//!
//! Размеры байтов хранятся как `f64` — `u32` переполняется на
//! файлах > 4 ГБ, а `Blob::size()` возвращает именно `f64`.
//! `f64` точно представляет целые до 2^53 ≈ 9 ПБ.
//!
//! **Важно:** результат `reader.read()` — это plain JS-объект
//! `{ done: bool, value: any }`, а не инстанс типа
//! `ReadableStreamReadResult` из web-sys. Поля извлекаются через
//! `js_sys::Reflect::get` — у web-sys-типа есть только сеттеры,
//! геттеров нет.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, ReadableStreamDefaultReader};

/// Один прочитанный чанк из потока.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// Порядковый номер чанка (с нуля).
    pub index: u32,
    /// Размер этого чанка в байтах. Один чанк всегда << 4 ГБ,
    /// поэтому `u32` достаточно.
    pub size: u32,
    /// Суммарный объём прочитанного, включая этот чанк.
    /// `f64` — чтобы не переполняться на файлах > 4 ГБ.
    pub total_so_far: f64,
}

/// Статистика потокового чтения.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamStats {
    /// Суммарный объём всех чанков (f64 — не переполняется).
    pub total_bytes: f64,
    /// Количество чанков.
    pub chunk_count: u32,
    /// Размер самого маленького чанка.
    pub min_chunk: u32,
    /// Размер самого большого чанка.
    pub max_chunk: u32,
    /// Средний размер чанка.
    pub avg_chunk: f64,
}

/// Извлекает поля `done` и `value` из результата `reader.read()`.
fn extract_read_result(value: &JsValue) -> Result<Option<js_sys::Uint8Array>, JsValue> {
    let obj: &js_sys::Object = value
        .dyn_ref()
        .ok_or_else(|| JsValue::from_str("read() вернул не Object"))?;

    let done_js = js_sys::Reflect::get(obj, &JsValue::from_str("done"))?;
    let done: bool = done_js.as_bool().unwrap_or(false);
    if done {
        return Ok(None);
    }

    let chunk_js = js_sys::Reflect::get(obj, &JsValue::from_str("value"))?;
    let uint8: js_sys::Uint8Array = chunk_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("чанк не Uint8Array"))?;
    return Ok(Some(uint8));
}

/// Читает `Blob` потоково, вызывая `on_chunk` на каждый чанк.
///
/// Возвращает общее количество прочитанных байт как `f64`.
pub async fn stream_chunks<F>(blob: &Blob, mut on_chunk: F) -> Result<f64, JsValue>
where
    F: FnMut(&Chunk),
{
    let stream = blob.stream();
    let reader_js = stream.get_reader();
    let reader: ReadableStreamDefaultReader = reader_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ReadableStreamDefaultReader"))?;

    let mut index: u32 = 0;
    let mut total: f64 = 0.0;

    loop {
        let read_promise = reader.read();
        let value = JsFuture::from(read_promise).await?;

        let chunk_opt = extract_read_result(&value)?;
        let uint8 = match chunk_opt {
            Some(u) => u,
            None => break,
        };

        let size: u32 = uint8.length();
        total += size as f64;

        let chunk = Chunk {
            index,
            size,
            total_so_far: total,
        };
        on_chunk(&chunk);

        index = index.wrapping_add(1);
    }

    return Ok(total);
}

/// Читает `Blob` потоково и возвращает статистику по чанкам.
pub async fn stream_stats(blob: &Blob) -> Result<StreamStats, JsValue> {
    let mut count: u32 = 0;
    let mut total: f64 = 0.0;
    let mut min_chunk: u32 = u32::MAX;
    let mut max_chunk: u32 = 0;

    let on_chunk = |chunk: &Chunk| {
        count = count.wrapping_add(1);
        total = chunk.total_so_far;
        if chunk.size < min_chunk {
            min_chunk = chunk.size;
        }
        if chunk.size > max_chunk {
            max_chunk = chunk.size;
        }
    };

    let _ = stream_chunks(blob, on_chunk).await?;

    let avg: f64;
    if count > 0 {
        avg = total / count as f64;
    } else {
        avg = 0.0;
    }

    let min_final: u32;
    if count > 0 {
        min_final = min_chunk;
    } else {
        min_final = 0;
    }

    return Ok(StreamStats {
        total_bytes: total,
        chunk_count: count,
        min_chunk: min_final,
        max_chunk,
        avg_chunk: avg,
    });
}