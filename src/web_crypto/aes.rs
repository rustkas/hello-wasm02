//! AES-GCM — симметричное шифрование с аутентификацией.
//!
//! `generateKey` → `encrypt` / `decrypt`.
//! GCM добавляет authentication tag: изменение шифротекста
//! приводит к ошибке дешифрования.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::CryptoError;
use super::util::{bytes_to_object, jsvalue_to_bytes};

pub const AES_GCM_IV_LEN: usize = 12; // 96 бит — рекомендация NIST

/// Генерирует AES-GCM ключ.
pub async fn generate_key(
    length_bits: u32,
    extractable: bool,
) -> Result<web_sys::CryptoKey, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("AES-GCM"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("length"), &JsValue::from_f64(length_bits as f64))
        .map_err(|e| CryptoError::from(e))?;

    let usages = js_sys::Array::new();
    usages.push(&JsValue::from_str("encrypt"));
    usages.push(&JsValue::from_str("decrypt"));

    let promise = subtle.generate_key_with_object(&alg, extractable, &usages)
        .map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;

    let key: web_sys::CryptoKey = result
        .dyn_into()
        .map_err(|_| CryptoError::new("AES key: unexpected type".to_string()))?;
    return Ok(key);
}

/// Шифрует `plaintext` ключом `key` со случайным IV.
pub async fn encrypt(
    key: &web_sys::CryptoKey,
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
    let iv = super::random::random_bytes(AES_GCM_IV_LEN)?;
    let ciphertext = encrypt_with_iv(key, plaintext, &iv).await?;
    return Ok((ciphertext, iv));
}

/// Шифрует с явным IV (для тестов с детерминированным IV).
pub async fn encrypt_with_iv(
    key: &web_sys::CryptoKey,
    plaintext: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("AES-GCM"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("iv"), &bytes_to_object(iv).into())
        .map_err(|e| CryptoError::from(e))?;

    let data_obj = bytes_to_object(plaintext);
    let promise = subtle.encrypt_with_object_and_buffer_source(
        &alg,
        key,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}

/// Дешифрует `ciphertext` ключом `key` с IV `iv`.
pub async fn decrypt(
    key: &web_sys::CryptoKey,
    ciphertext: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("AES-GCM"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("iv"), &bytes_to_object(iv).into())
        .map_err(|e| CryptoError::from(e))?;

    let data_obj = bytes_to_object(ciphertext);
    let promise = subtle.decrypt_with_object_and_buffer_source(
        &alg,
        key,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}