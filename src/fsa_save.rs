//! File System Access API: сохранение файла.
//!
//! **Требования:** Chrome/Edge + secure context + user gesture.
//! В Firefox и Safari модуль не используется — там fallback
//! на `<a download>` + Object URL.
//!
//! **Про вызовы.** web-sys 0.3.105 экспортирует `show_save_file_picker()`
//! **без аргументов**. Опции передаём через Reflect. Методы
//! `write` и `close` writable-потока тоже вызываем через Reflect —
//! так надёжнее и переносимее между версиями.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FileSystemFileHandle};

/// Сохраняет `Blob` в файл, выбранный пользователем.
///
/// Возвращает имя сохранённого файла (или `None`, если пользователь
/// отменил диалог).
pub async fn save_blob(
    blob: &Blob,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?;
    let window_js: &JsValue = window.as_ref();

    // showSaveFilePicker({ suggestedName })
    let func_js = js_sys::Reflect::get(
        window_js,
        &JsValue::from_str("showSaveFilePicker"),
    )?;
    if func_js.is_undefined() {
        return Err(JsValue::from_str("showSaveFilePicker недоступен"));
    }
    let func: js_sys::Function = func_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не функция"))?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;

    let promise_js = func.call1(window_js, &options)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;

    let handle_value = match JsFuture::from(promise).await {
        Ok(v) => v,
        Err(_) => return Ok(None), // пользователь отменил
    };

    let handle: FileSystemFileHandle = handle_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))?;
    let file_name = handle.name();

    // createWritable() → Promise<FileSystemWritableFileStream>
    let handle_js: &JsValue = handle.as_ref();
    let create_fn_js = js_sys::Reflect::get(
        handle_js,
        &JsValue::from_str("createWritable"),
    )?;
    let create_fn: js_sys::Function = create_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("createWritable не функция"))?;
    let writable_promise_js = create_fn.call0(handle_js)?;
    let writable_promise: js_sys::Promise = writable_promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let writable: JsValue = JsFuture::from(writable_promise).await?;

    // write(blob) — данные буферизуются.
    let writable_js: &JsValue = &writable;
    let write_fn_js = js_sys::Reflect::get(
        writable_js,
        &JsValue::from_str("write"),
    )?;
    let write_fn: js_sys::Function = write_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("write не функция"))?;
    let blob_js: &JsValue = blob.as_ref();
    let write_promise_js = write_fn.call1(writable_js, blob_js)?;
    let write_promise: js_sys::Promise = write_promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    JsFuture::from(write_promise).await?;

    // close() — данные попадают на диск.
    let close_fn_js = js_sys::Reflect::get(
        writable_js,
        &JsValue::from_str("close"),
    )?;
    let close_fn: js_sys::Function = close_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("close не функция"))?;
    let close_promise_js = close_fn.call0(writable_js)?;
    let close_promise: js_sys::Promise = close_promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    JsFuture::from(close_promise).await?;

    return Ok(Some(file_name));
}

/// Сохранить срез `u8` как бинарный файл.
pub async fn save_bytes(
    bytes: &[u8],
    mime_type: &str,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    let uint8 = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&uint8.buffer());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime_type);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options)?;
    return save_blob(&blob, suggested_name).await;
}

/// Сохранить строку как текстовый файл.
pub async fn save_text(
    text: &str,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    let array = js_sys::Array::of1(&JsValue::from_str(text));
    let blob = Blob::new_with_str_sequence(&array)?;
    return save_blob(&blob, suggested_name).await;
}