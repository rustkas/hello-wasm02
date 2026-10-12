//! Base64 — кодирование и декодирование через crate `base64`.
//! Модуль автономен: работает на любой платформе, не только в браузере.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Кодирует байты в base64 (стандартный алфавит с padding `=`).
///
/// Соответствует тому, что выдаёт браузерная `btoa` для ASCII-строк.
pub fn encode(bytes: &[u8]) -> String {
    return STANDARD.encode(bytes);
}

/// Декодирует base64-строку в байты.
///
/// Возвращает `Err(String)` с описанием проблемы, если строка
/// не является валидным base64.
pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    return STANDARD.decode(s).map_err(|e| e.to_string());
}