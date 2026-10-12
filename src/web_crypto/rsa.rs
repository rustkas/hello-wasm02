//! RSA-OAEP — асимметричное шифрование.
//!
//! `generateKey` → `(publicKey, privateKey)`.
//! Шифруем публичным, дешифруем приватным.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::CryptoError;
use super::util::{bytes_to_object, jsvalue_to_bytes};
use super::digest::HashAlg;

pub const RSA_2048: u32 = 2048;
pub const RSA_4096: u32 = 4096;

/// Стандартная публичная экспонента F4 = 65537 = [0x01, 0x00, 0x01].
const PUBLIC_EXPONENT_F4: [u8; 3] = [0x01, 0x00, 0x01];

/// Пара ключей RSA.
pub struct RsaKeyPair {
    pub public_key: web_sys::CryptoKey,
    pub private_key: web_sys::CryptoKey,
}

/// Генерирует пару ключей RSA-OAEP.
///
/// `modulus_length` — 2048 или 4096 бит.
/// `hash` — хеш для OAEP-дополнения.
pub async fn generate_key_pair(
    modulus_length: u32,
    hash: HashAlg,
) -> Result<RsaKeyPair, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("RSA-OAEP"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("modulusLength"), &JsValue::from_f64(modulus_length as f64))
        .map_err(|e| CryptoError::from(e))?;

    // publicExponent — обязательное поле. Формат: Uint8Array.
    let exponent = js_sys::Uint8Array::new_with_length(PUBLIC_EXPONENT_F4.len() as u32);
    exponent.copy_from(&PUBLIC_EXPONENT_F4);
    js_sys::Reflect::set(
        &alg,
        &JsValue::from_str("publicExponent"),
        &JsValue::from(exponent),
    ).map_err(|e| CryptoError::from(e))?;

    let hash_obj = js_sys::Object::new();
    js_sys::Reflect::set(&hash_obj, &JsValue::from_str("name"), &JsValue::from_str(hash.as_str()))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("hash"), &JsValue::from(hash_obj))
        .map_err(|e| CryptoError::from(e))?;

    let usages = js_sys::Array::new();
    usages.push(&JsValue::from_str("encrypt"));
    usages.push(&JsValue::from_str("decrypt"));

    let promise = subtle.generate_key_with_object(&alg, true, &usages)
        .map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;

    let public = js_sys::Reflect::get(&result, &JsValue::from_str("publicKey"))
        .map_err(|e| CryptoError::from(e))?;
    let private = js_sys::Reflect::get(&result, &JsValue::from_str("privateKey"))
        .map_err(|e| CryptoError::from(e))?;

    let public_key: web_sys::CryptoKey = public
        .dyn_into()
        .map_err(|_| CryptoError::new("RSA publicKey: unexpected type".to_string()))?;
    let private_key: web_sys::CryptoKey = private
        .dyn_into()
        .map_err(|_| CryptoError::new("RSA privateKey: unexpected type".to_string()))?;

    return Ok(RsaKeyPair { public_key, private_key });
}

/// Шифрует `plaintext` публичным ключом RSA-OAEP.
pub async fn encrypt(
    public_key: &web_sys::CryptoKey,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("RSA-OAEP"))
        .map_err(|e| CryptoError::from(e))?;

    let data_obj = bytes_to_object(plaintext);
    let promise = subtle.encrypt_with_object_and_buffer_source(
        &alg,
        public_key,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}

/// Дешифрует `ciphertext` приватным ключом RSA-OAEP.
pub async fn decrypt(
    private_key: &web_sys::CryptoKey,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("RSA-OAEP"))
        .map_err(|e| CryptoError::from(e))?;

    let data_obj = bytes_to_object(ciphertext);
    let promise = subtle.decrypt_with_object_and_buffer_source(
        &alg,
        private_key,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}