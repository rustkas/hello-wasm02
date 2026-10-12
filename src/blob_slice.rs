//! Blob.slice — чтение частей файла без загрузки целиком.
//!
//! Модуль автономен: не зависит от других модулей проекта.
//!
//! `Blob.slice(start, end)` создаёт «окно» в исходный Blob — без
//! копирования данных. Это позволяет читать большие файлы по
//! частям и собирать их обратно без риска исчерпать память.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FileReader};

/// Возвращает Blob-срез от `start` до `end`.
///
/// Семантика совпадает с `Blob.slice(start, end)`:
/// * `start` включительно, `end` не включается;
/// * если `end` больше размера Blob, срез идёт до конца;
/// * если `start >= size`, вернётся пустой Blob.
///
/// Данные не копируются: срез — это «окно» в исходный Blob.
/// Реальное копирование происходит только при чтении.
pub fn slice_blob(blob: &Blob, start: f64, end: f64) -> Result<Blob, JsValue> {
    return blob.slice_with_f64_and_f64(start, end);
}

/// Читает Blob как `Vec<u8>`.
///
/// Приватный helper для этой подглавы: превращает Blob в массив
/// байтов Rust через `FileReader::readAsArrayBuffer`.
async fn read_blob_bytes(blob: &Blob) -> Result<Vec<u8>, JsValue> {
    let reader = FileReader::new()?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let reader_clone = reader.clone();
        let onload = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let onerror = Closure::once_into_js(move |err: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &err);
        });
        reader_clone.set_onload(Some(onload.unchecked_ref()));
        reader_clone.set_onerror(Some(onerror.unchecked_ref()));
    });
    reader.read_as_array_buffer(blob)?;
    JsFuture::from(promise).await?;

    let result = reader.result()?;
    let array_buffer: js_sys::ArrayBuffer = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let uint8 = js_sys::Uint8Array::new(&array_buffer);
    return Ok(uint8.to_vec());
}

/// Читает срез `[start, end)` как `Vec<u8>`.
pub async fn read_slice(blob: &Blob, start: f64, end: f64) -> Result<Vec<u8>, JsValue> {
    let slice = slice_blob(blob, start, end)?;
    return read_blob_bytes(&slice).await;
}

/// Читает первые `n` байт из Blob.
///
/// Удобная обёртка над `read_slice(blob, 0.0, n as f64)`.
/// Если файл короче `n`, вернётся всё содержимое.
pub async fn read_first_n(blob: &Blob, n: u32) -> Result<Vec<u8>, JsValue> {
    return read_slice(blob, 0.0, n as f64).await;
}

/// Считает количество чанков для файла размера `size`
/// при размере чанка `chunk_size`.
///
/// Возвращает `0`, если `size <= 0` или `chunk_size <= 0`.
pub fn chunk_count(size: f64, chunk_size: f64) -> u32 {
    if chunk_size <= 0.0 || size <= 0.0 {
        return 0;
    }
    let n: f64 = (size / chunk_size).ceil();
    return n as u32;
}

/// Разбивает Blob на последовательность чанков по `chunk_size` байт.
///
/// Последний чанк может быть короче — он содержит остаток.
/// Если `chunk_size <= 0`, возвращается пустой вектор.
pub fn split_into_chunks(blob: &Blob, chunk_size: f64) -> Vec<Blob> {
    let mut chunks: Vec<Blob> = Vec::new();
    if chunk_size <= 0.0 {
        return chunks;
    }

    let size = blob.size();
    let mut offset = 0.0_f64;
    while offset < size {
        let end: f64 = (offset + chunk_size).min(size);
        if let Ok(chunk) = blob.slice_with_f64_and_f64(offset, end) {
            chunks.push(chunk);
        }
        offset = end;
    }
    return chunks;
}

/// Склеивает последовательность Blob в один с заданным MIME-типом.
///
/// Использует конструктор `new Blob([...parts], {type})` — тот же
/// приём, что и `vec_to_blob` из подглавы 18.3, только на входе
/// не `Uint8Array`, а готовые Blob-объекты.
pub fn concat_blobs(chunks: &[Blob], mime_type: &str) -> Result<Blob, JsValue> {
    let parts = js_sys::Array::new();
    for chunk in chunks {
        parts.push(chunk.as_ref());
    }
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime_type);
    return Blob::new_with_blob_sequence_and_options(&parts, &options);
}