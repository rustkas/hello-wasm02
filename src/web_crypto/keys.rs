//! importKey / exportKey — импорт и экспорт ключей.
//!
//! Форматы:
//! - `"raw"`   — сырые байты (HMAC, AES).
//! - `"jwk"`   — JSON Web Key (все типы, включая асимметричные).
//! - `"spki"`  — публичный ключ (SubjectPublicKeyInfo).
//! - `"pkcs8"` — приватный ключ (PKCS#8).

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::CryptoError;
use super::util::{bytes_to_object, jsvalue_to_bytes};

/// Формат ключа.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyFormat {
    Raw,
    Jwk,
    Spki,
    Pkcs8,
}

impl KeyFormat {
    pub fn as_str(self) -> &'static str {
        return match self {
            KeyFormat::Raw => "raw",
            KeyFormat::Jwk => "jwk",
            KeyFormat::Spki => "spki",
            KeyFormat::Pkcs8 => "pkcs8",
        };
    }
}

/// Импортирует ключ из сырых байт (`raw`) или из JWK-объекта.
pub async fn import_raw(
    format: KeyFormat,
    data: &[u8],
    algorithm: &js_sys::Object,
    extractable: bool,
    usages: &[&str],
) -> Result<web_sys::CryptoKey, CryptoError> {
    let subtle = super::subtle()?;
    let data_obj = bytes_to_object(data);
    let usages_js = js_sys::Array::new();
    for u in usages {
        usages_js.push(&JsValue::from_str(u));
    }
    let promise = subtle.import_key_with_object(
        format.as_str(),
        &data_obj,
        algorithm,
        extractable,
        &usages_js,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    let key: web_sys::CryptoKey = result
        .dyn_into()
        .map_err(|_| CryptoError::new("importKey: unexpected type".to_string()))?;
    return Ok(key);
}

/// Экспортирует ключ в сырые байты (`raw`).
pub async fn export_raw(key: &web_sys::CryptoKey) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;
    let promise = subtle.export_key("raw", key)
        .map_err(|e| CryptoError::from(e))?;
    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}

/// Экспортирует ключ в формате JWK (возвращает JSON-строку).
pub async fn export_jwk(key: &web_sys::CryptoKey) -> Result<String, CryptoError> {
    let subtle = super::subtle()?;
    let promise = subtle.export_key("jwk", key)
        .map_err(|e| CryptoError::from(e))?;
    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    let json = js_sys::JSON::stringify(&result)
        .map_err(|e| CryptoError::from(e))?;
    return json
        .as_string()
        .ok_or_else(|| CryptoError::new("JWK export: expected string".to_string()));
}

/// Строит algorithm-объект `{ name: "AES-GCM" }` — нужен для importKey
/// AES-ключа из raw/JWK.
pub fn aes_gcm_alg() -> js_sys::Object {
    let alg = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &alg,
        &JsValue::from_str("name"),
        &JsValue::from_str("AES-GCM"),
    );
    return alg;
}