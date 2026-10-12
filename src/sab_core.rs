//! Работа с ArrayBuffer, TypedArray и memory views.
//! Автономный модуль — не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;

/// Проверяет cross-origin isolation (нужно для SharedArrayBuffer).
///
/// `web-sys 0.3.105` не экспортирует метод на Window —
/// читаем свойство глобального объекта через Reflect.
pub fn cross_origin_isolated() -> bool {
    let global = js_sys::global();
    js_sys::Reflect::get(&global, &JsValue::from_str("crossOriginIsolated"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Создаёт Uint8Array из Rust-среза через копирование.
/// `Vec<u8>` → `Uint8Array` (2 копии: Rust → WASM heap → JS heap).
pub fn bytes_to_copy(data: &[u8]) -> js_sys::Uint8Array {
    js_sys::Uint8Array::from(data)
}

/// Создаёт Float32Array из Rust-среза через копирование.
pub fn f32_to_copy(data: &[f32]) -> js_sys::Float32Array {
    js_sys::Float32Array::from(data)
}

/// Читает JS typed array в Rust `Vec<u8>` (одна копия).
pub fn copy_from_uint8(view: &js_sys::Uint8Array) -> Vec<u8> {
    view.to_vec()
}

/// Читает JS typed array в Rust `Vec<f32>` (одна копия).
pub fn copy_from_f32(view: &js_sys::Float32Array) -> Vec<f32> {
    view.to_vec()
}

/// Информация о среде: cross-origin isolated и доступность SAB.
#[derive(Debug, Clone)]
pub struct EnvInfo {
    pub cross_origin_isolated: bool,
    pub shared_array_buffer_available: bool,
}

pub fn env_info() -> EnvInfo {
    let isolated = cross_origin_isolated();
    EnvInfo {
        cross_origin_isolated: isolated,
        shared_array_buffer_available: isolated,
    }
}

/// Создаёт SharedArrayBuffer, если среда изолирована.
/// Иначе возвращает обычный ArrayBuffer через Uint8Array.
pub fn make_shared_or_plain(size: usize) -> Result<js_sys::Uint8Array, JsValue> {
    if cross_origin_isolated() {
        let sab = js_sys::SharedArrayBuffer::new(size as u32);
        Ok(js_sys::Uint8Array::new(&sab))
    } else {
        let ab = js_sys::ArrayBuffer::new(size as u32);
        Ok(js_sys::Uint8Array::new(&ab))
    }
}