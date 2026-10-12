//! Глава 38: Web Crypto API — браузерная криптография.
//!
//! Все модули оборачивают `web_sys::SubtleCrypto` в удобный API,
//! возвращающий `Result<T, CryptoError>` вместо разрозненных `JsValue`.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

pub mod digest;
pub mod hmac;
pub mod aes;
pub mod rsa;
pub mod ecdsa;
pub mod random;
pub mod keys;
pub mod util;

use wasm_bindgen::JsValue;
use crate::format_str::format_str;

/// Универсальная ошибка криптоопераций.
///
/// Оборачивает `JsValue` из JS-промисов в человекочитаемую строку.
#[derive(Debug, Clone)]
pub struct CryptoError {
    pub message: String,
}

impl CryptoError {
    pub fn new(msg: String) -> Self {
        return Self { message: msg };
    }
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.message);
    }
}

impl From<JsValue> for CryptoError {
    fn from(v: JsValue) -> Self {
        let msg = v
            .as_string()
            .unwrap_or_else(|| {
                let debug = format_str(format_args!("{v:?}"));
                debug
            });
        return Self { message: msg };
    }
}

impl From<String> for CryptoError {
    fn from(s: String) -> Self {
        return Self { message: s };
    }
}

/// Возвращает `window.crypto.subtle`.
///
/// `Crypto` доступен через `Window.crypto`, `SubtleCrypto` — через
/// `Crypto.subtle`. Если браузер не поддерживает Web Crypto (например,
/// страница открыта по HTTP), возвращаем ошибку.
pub fn subtle() -> Result<web_sys::SubtleCrypto, CryptoError> {
    let window = web_sys::window()
        .ok_or_else(|| CryptoError::new("no window".to_string()))?;
    let crypto = window.crypto()
        .map_err(|_| CryptoError::new("window.crypto unavailable".to_string()))?;
    let subtle = crypto.subtle();
    return Ok(subtle);
}

/// Возвращает `window.crypto` (для `getRandomValues`).
pub fn crypto_obj() -> Result<web_sys::Crypto, CryptoError> {
    let window = web_sys::window()
        .ok_or_else(|| CryptoError::new("no window".to_string()))?;
    return window.crypto().map_err(|e| CryptoError::from(e));
}