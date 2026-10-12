//! HMAC — Hash-based Message Authentication Code.
//!
//! `generateKey` → `sign` / `verify`.
//! HMAC использует симметричный ключ: один ключ и для подписи, и для проверки.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::CryptoError;
use super::util::{bytes_to_object, jsvalue_to_bytes};
use super::digest::HashAlg;

/// Генерирует HMAC-ключ.
pub async fn generate_key(
    hash: HashAlg,
    length_bits: u32,
    extractable: bool,
) -> Result<web_sys::CryptoKey, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("HMAC"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("hash"), &JsValue::from_str(hash.as_str()))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("length"), &JsValue::from_f64(length_bits as f64))
        .map_err(|e| CryptoError::from(e))?;

    let usages = js_sys::Array::new();
    usages.push(&JsValue::from_str("sign"));
    usages.push(&JsValue::from_str("verify"));

    let promise = subtle.generate_key_with_object(&alg, extractable, &usages)
        .map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;

    let key: web_sys::CryptoKey = result
        .dyn_into()
        .map_err(|_| CryptoError::new("HMAC key: unexpected type".to_string()))?;
    return Ok(key);
}

/// Подписывает `data` ключом `key` алгоритмом HMAC.
pub async fn sign(key: &web_sys::CryptoKey, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("HMAC"))
        .map_err(|e| CryptoError::from(e))?;

    let data_obj = bytes_to_object(data);
    let promise = subtle.sign_with_object_and_buffer_source(
        &alg,
        key,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}

/// Проверяет подпись `signature` для `data` ключом `key`.
pub async fn verify(
    key: &web_sys::CryptoKey,
    signature: &[u8],
    data: &[u8],
) -> Result<bool, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("HMAC"))
        .map_err(|e| CryptoError::from(e))?;

    let sig_obj = bytes_to_object(signature);
    let data_obj = bytes_to_object(data);
    let promise = subtle.verify_with_object_and_buffer_source_and_buffer_source(
        &alg,
        key,
        &sig_obj,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return result.as_bool().ok_or_else(|| {
        CryptoError::new("HMAC verify: expected boolean".to_string())
    });
}