//! Кодирование/декодирование для WebAuthn.
//!
//! WebAuthn использует два формата для бинарных данных:
//! - `Uint8Array` — внутри JS API (challenge, rawId, signature).
//! - base64url — при передаче по сети (JSON).
//!
//! При этом в JSON-формате challenge и credential.id обычно
//! кодируются в base64url без padding.

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

/// Кодирует байты в base64url (без padding).
pub fn bytes_to_base64url(bytes: &[u8]) -> String {
    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    arr.copy_from(bytes);
    // btoa не умеет в base64url — заменяем символы.
    let window = web_sys::window().expect("no window");
    let btoa = js_sys::Reflect::get(&window, &JsValue::from_str("btoa"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let Some(btoa) = btoa else {
        return String::new();
    };
    // Преобразуем байты в Latin-1 строку.
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        s.push(b as char);
    }
    let result = btoa
        .call1(&JsValue::NULL, &JsValue::from_str(&s))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    return result
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_string();
}

/// Декодирует base64url (с или без padding) в байты.
pub fn base64url_to_bytes(s: &str) -> Result<Vec<u8>, String> {
    // Добавляем padding.
    let padded = match s.len() % 4 {
        0 => s.to_string(),
        2 => format!("{s}=="),
        3 => format!("{s}="),
        _ => return Err("invalid base64url length".to_string()),
    };
    let standard = padded.replace('-', "+").replace('_', "/");

    let window = web_sys::window().ok_or("no window")?;
    let atob = js_sys::Reflect::get(&window, &JsValue::from_str("atob"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let Some(atob) = atob else {
        return Err("no atob".to_string());
    };
    let decoded = atob
        .call1(&JsValue::NULL, &JsValue::from_str(&standard))
        .map_err(|_| "atob failed".to_string())?;
    let decoded_str = decoded.as_string().ok_or("atob: not a string")?;
    return Ok(decoded_str.bytes().collect());
}

/// Копирует содержимое `ArrayBuffer`/`Uint8Array` в `Vec<u8>`.
pub fn jsvalue_to_bytes(value: &JsValue) -> Result<Vec<u8>, String> {
    if value.is_null() || value.is_undefined() {
        return Err("jsvalue_to_bytes: null/undefined".to_string());
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

/// Создаёт `Uint8Array` из среза байт (для передачи в JS API).
pub fn bytes_to_uint8array(bytes: &[u8]) -> js_sys::Uint8Array {
    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    arr.copy_from(bytes);
    return arr;
}

/// Hex-превью первых `max` байт — для отображения в UI.
///
/// Если байт больше `max`, в конце добавляется многоточие.
pub fn hex_preview(bytes: &[u8], max: usize) -> String {
    let n = bytes.len().min(max);
    let mut out = String::with_capacity(n * 3 + 8);
    for (i, b) in bytes.iter().take(n).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&crate::format_str::format_str(
            format_args!("{:02x}", b),
        ));
    }
    if bytes.len() > max {
        out.push_str(" …");
    }
    return out;
}