//! Раздутие base64: localStorage хранит только строки, бинарные данные
//! приходится кодировать. Считаем, во сколько раз это дороже.
//!
//! Автономный модуль — не зависит от других модулей проекта.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use wasm_bindgen::prelude::*;

/// Псевдослучайные байты через LCG. Детерминированы по seed,
/// что удобно для тестов и повторных прогонов.
pub fn random_bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.push((state >> 33) as u8);
    }
    out
}

/// Кодирует байты в base64-строку.
pub fn encode_base64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Декодирует base64. Возвращает ошибку при невалидной строке.
pub fn decode_base64(s: &str) -> Result<Vec<u8>, String> {
    STANDARD.decode(s).map_err(|e| format!("base64 decode: {e}"))
}

/// Отчёт о раздутии.
#[derive(Debug, Clone, PartialEq)]
pub struct BloatReport {
    /// Исходный размер бинарных данных.
    pub raw_bytes: usize,
    /// Длина base64-строки в байтах (ASCII).
    pub base64_chars: usize,
    /// Размер base64-строки в байтах UTF-16, как её считает localStorage.
    pub utf16_bytes: usize,
    /// Во сколько раз UTF-16 представление больше исходных байтов.
    pub ratio: f64,
}

/// Анализирует раздутие: сколько занимает `raw` в бинарном виде
/// vs сколько занимает его base64-представление в localStorage.
pub fn analyze_bloat(raw: &[u8]) -> BloatReport {
    let encoded = STANDARD.encode(raw);
    let base64_chars = encoded.len();
    let utf16_bytes = base64_chars * 2;
    let ratio = if raw.is_empty() {
        0.0
    } else {
        utf16_bytes as f64 / raw.len() as f64
    };
    BloatReport {
        raw_bytes: raw.len(),
        base64_chars,
        utf16_bytes,
        ratio,
    }
}

/// Сохраняет байты в localStorage как base64.
/// Возвращает реальный размер (в UTF-16 байтах), который заняла запись.
pub fn store_bytes_as_base64(key: &str, bytes: &[u8]) -> Result<usize, JsValue> {
    let encoded = STANDARD.encode(bytes);
    crate::ls_probe::set_string(key, &encoded)?;
    Ok(encoded.encode_utf16().count() * 2)
}

/// Читает байты из localStorage, декодируя base64.
/// None — если ключа нет.
pub fn load_bytes_from_base64(key: &str) -> Result<Option<Vec<u8>>, JsValue> {
    let raw = match crate::ls_probe::get_string(key)? {
        Some(v) => v,
        None => return Ok(None),
    };
    STANDARD
        .decode(&raw)
        .map(Some)
        .map_err(|e| JsValue::from_str(&format!("base64 decode: {e}")))
}