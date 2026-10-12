//! Общие хелперы для криптомодулей: кодирование/декодирование байтов.
//!
//! Методы web-sys с суффиксом `_with_object` принимают `&js_sys::Object`.
//! `Uint8Array` наследует `Object`, поэтому `bytes_to_object` возвращает
//! именно `Object` — этого достаточно для всех `BufferSource`-параметров.

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

/// Преобразует `&[u8]` в `js_sys::Object`, содержащий `Uint8Array`.
pub fn bytes_to_object(bytes: &[u8]) -> js_sys::Object {
    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    arr.copy_from(bytes);
    let obj: js_sys::Object = arr.into();
    return obj;
}

/// Извлекает `Vec<u8>` из `JsValue`, который может быть:
/// - `Uint8Array` (результат `subtle.digest`, `subtle.encrypt` и т.п.),
/// - `ArrayBuffer`.
pub fn jsvalue_to_bytes(value: &JsValue) -> Result<Vec<u8>, String> {
    if value.is_undefined() || value.is_null() {
        return Err("jsvalue_to_bytes: value is null/undefined".to_string());
    }
    if let Some(arr) = value.dyn_ref::<js_sys::Uint8Array>() {
        return Ok(arr.to_vec());
    }
    if let Some(buf) = value.dyn_ref::<js_sys::ArrayBuffer>() {
        let arr = js_sys::Uint8Array::new(buf);
        return Ok(arr.to_vec());
    }
    return Err("jsvalue_to_bytes: unsupported type".to_string());
}

/// Кодирует байты в hex-строку.
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&crate::format_str::format_str(
            format_args!("{:02x}", b),
        ));
    }
    return out;
}