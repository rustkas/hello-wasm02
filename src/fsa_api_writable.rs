//! FileSystemWritableFileStream — поток записи в файл.
//!
//! * write(data) — data: BufferSource | Blob | {type, data, position?}.
//! * seek(position) — позиция в байтах.
//! * truncate(size) — обрезать до размера.
//! * close() — данные попадают на диск.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

fn call_promise(
    stream: &JsValue,
    method: &str,
    args: &[JsValue],
) -> Result<js_sys::Promise, JsValue> {
    let fn_js = js_sys::Reflect::get(stream, &JsValue::from_str(method))?;
    if fn_js.is_undefined() {
        return Err(JsValue::from_str(&format!("{method} недоступен")));
    }
    let func: js_sys::Function = fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{method} не функция")))?;
    let promise_js = match args.len() {
        0 => func.call0(stream)?,
        1 => func.call1(stream, &args[0])?,
        _ => return Err(JsValue::from_str("слишком много аргументов")),
    };
    promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{method} вернул не Promise")))
}

/// Запись среза байт.
pub async fn write_bytes(stream: &JsValue, data: &[u8]) -> Result<(), JsValue> {
    let uint8 = js_sys::Uint8Array::from(data);
    let promise = call_promise(stream, "write", &[uint8.into()])?;
    let _ = JsFuture::from(promise).await?;
    Ok(())
}

/// Запись в конкретную позицию.
pub async fn write_bytes_at(
    stream: &JsValue,
    data: &[u8],
    position: f64,
) -> Result<(), JsValue> {
    let uint8 = js_sys::Uint8Array::from(data);
    let payload = js_sys::Object::new();
    js_sys::Reflect::set(
        &payload,
        &JsValue::from_str("type"),
        &JsValue::from_str("write"),
    )?;
    js_sys::Reflect::set(&payload, &JsValue::from_str("position"), &JsValue::from_f64(position))?;
    js_sys::Reflect::set(&payload, &JsValue::from_str("data"), &uint8)?;
    let promise = call_promise(stream, "write", &[payload.into()])?;
    let _ = JsFuture::from(promise).await?;
    Ok(())
}

/// Запись Blob.
pub async fn write_blob(stream: &JsValue, blob: &web_sys::Blob) -> Result<(), JsValue> {
    let blob_js: &JsValue = blob.as_ref();
    let promise = call_promise(stream, "write", &[blob_js.clone()])?;
    let _ = JsFuture::from(promise).await?;
    Ok(())
}

/// Запись строки (через Blob — кодировка UTF-8).
pub async fn write_text(stream: &JsValue, text: &str) -> Result<(), JsValue> {
    let array = js_sys::Array::of1(&JsValue::from_str(text));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/plain;charset=utf-8");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&array, &options)?;
    write_blob(stream, &blob).await
}

/// seek(position) — переход в позицию.
pub async fn seek(stream: &JsValue, position: f64) -> Result<(), JsValue> {
    let promise = call_promise(stream, "seek", &[JsValue::from_f64(position)])?;
    let _ = JsFuture::from(promise).await?;
    Ok(())
}

/// truncate(size) — обрезка до размера.
pub async fn truncate(stream: &JsValue, size: f64) -> Result<(), JsValue> {
    let promise = call_promise(stream, "truncate", &[JsValue::from_f64(size)])?;
    let _ = JsFuture::from(promise).await?;
    Ok(())
}

/// close() — обязателен для фиксации данных на диске.
pub async fn close(stream: &JsValue) -> Result<(), JsValue> {
    let promise = call_promise(stream, "close", &[])?;
    let _ = JsFuture::from(promise).await?;
    Ok(())
}