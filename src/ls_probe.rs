//! Работа с localStorage: CRUD, детекция квоты, приблизительный размер.
//! Автономный модуль — не зависит от других модулей проекта.
//!
//! ПРАВИЛО: ошибки `localStorage` пробрасываются как `JsValue` **без обёртки
//! в строку**. `JsValue::from_str(&format!("{e:?}"))` уничтожает свойства
//! `name` и `message` исходного `DOMException`, из-за чего `is_quota_error`
//! перестаёт распознавать `QuotaExceededError`.

use wasm_bindgen::prelude::*;
use web_sys::{DomException, Storage};

/// Возвращает localStorage. Ошибка — если окна нет или storage отключён.
pub fn storage() -> Result<Storage, JsValue> {
    let win = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let opt = win.local_storage()?;
    opt.ok_or_else(|| JsValue::from_str("localStorage недоступен"))
}

pub fn is_available() -> bool {
    storage().is_ok()
}

/// Записывает строку. Ошибку пробрасывает как есть.
pub fn set_string(key: &str, value: &str) -> Result<(), JsValue> {
    storage()?.set_item(key, value)
}

pub fn get_string(key: &str) -> Result<Option<String>, JsValue> {
    storage()?.get_item(key)
}

pub fn remove(key: &str) -> Result<(), JsValue> {
    storage()?.remove_item(key)
}

pub fn clear_all() -> Result<(), JsValue> {
    storage()?.clear()
}

pub fn keys() -> Result<Vec<String>, JsValue> {
    let s = storage()?;
    let len = s.length()?;
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        if let Some(k) = s.key(i)? {
            out.push(k);
        }
    }
    Ok(out)
}

pub fn item_count() -> Result<u32, JsValue> {
    storage()?.length()
}

pub fn approx_bytes_used() -> Result<usize, JsValue> {
    let s = storage()?;
    let len = s.length()?;
    let mut total = 0usize;
    for i in 0..len {
        if let Some(k) = s.key(i)? {
            total += utf16_bytes(&k);
            if let Some(v) = s.get_item(&k)? {
                total += utf16_bytes(&v);
            }
        }
    }
    Ok(total)
}

pub fn utf16_bytes(s: &str) -> usize {
    s.encode_utf16().count() * 2
}

/// true, если ошибка — `QuotaExceededError`.
///
/// Работает только если ошибка — оригинальный `JsValue` (DOMException).
/// Если `set_string` её обернул в строку, детектор не сработает — см. правило.
pub fn is_quota_error(e: &JsValue) -> bool {
    if let Some(ex) = e.dyn_ref::<DomException>() {
        if ex.name() == "QuotaExceededError" {
            return true;
        }
    }
    js_sys::Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .map(|s| s == "QuotaExceededError")
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct StorageInfo {
    pub available: bool,
    pub key_count: u32,
    pub approx_bytes: usize,
}

pub fn probe() -> StorageInfo {
    if !is_available() {
        return StorageInfo {
            available: false,
            key_count: 0,
            approx_bytes: 0,
        };
    }
    StorageInfo {
        available: true,
        key_count: item_count().unwrap_or(0),
        approx_bytes: approx_bytes_used().unwrap_or(0),
    }
}