//! FileReader и FileReaderSync — чтение содержимого файлов.
//! Асинхронный вариант работает в основном потоке,
//! синхронный — только внутри Web Workers.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FileReader, FileReaderSync};

/// Асинхронно читает файл как ArrayBuffer.
/// Возвращает `js_sys::ArrayBuffer` в виде `JsValue`.
pub async fn read_as_array_buffer(blob: &Blob) -> Result<JsValue, JsValue> {
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
    reader.result()
}

/// Асинхронно читает `Blob` как текст.
///
/// Внутри оборачивает события `onload` / `onerror` в `Promise`,
/// чтобы вызывающая сторона могла использовать `async/await`.
pub async fn read_as_text(blob: &Blob) -> Result<String, JsValue> {
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

    reader.read_as_text(blob)?;
    JsFuture::from(promise).await?;

    return reader
        .result()?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Не удалось получить текст"));
}

/// Синхронно читает `Blob` как текст.
///
/// Доступно только в контексте Web Worker — в основном потоке
/// браузер выбросит исключение.
pub fn read_as_text_sync(blob: &Blob) -> Result<String, JsValue> {
    let sync_reader = FileReaderSync::new()?;
    return sync_reader.read_as_text(blob);
}

/// Синхронно читает Blob как ArrayBuffer.
/// Доступно только в контексте Web Worker.
pub fn read_as_array_buffer_sync(blob: &Blob) -> Result<js_sys::ArrayBuffer, JsValue> {
    let sync_reader = FileReaderSync::new()?;
    sync_reader.read_as_array_buffer(blob)
}