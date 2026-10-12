//! ECDSA — подпись на эллиптических кривых.
//! ECDH — обмен ключами (deriveKey / deriveBits).
//!
//! ECDSA: `generateKey` → `(publicKey, privateKey)` → `sign` / `verify`.
//! ECDH: два участника генерируют пары, обмениваются публичными,
//!       затем `deriveKey` даёт общий симметричный ключ.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::CryptoError;
use super::util::{bytes_to_object, jsvalue_to_bytes};

pub const P256: &str = "P-256";
pub const P384: &str = "P-384";
pub const P521: &str = "P-521";

/// Пара ключей ECDSA.
pub struct EcdsaKeyPair {
    pub public_key: web_sys::CryptoKey,
    pub private_key: web_sys::CryptoKey,
}

/// Генерирует пару ключей ECDSA.
pub async fn generate_key_pair(
    named_curve: &str,
) -> Result<EcdsaKeyPair, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("ECDSA"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("namedCurve"), &JsValue::from_str(named_curve))
        .map_err(|e| CryptoError::from(e))?;

    let usages = js_sys::Array::new();
    usages.push(&JsValue::from_str("sign"));
    usages.push(&JsValue::from_str("verify"));

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
        .map_err(|_| CryptoError::new("ECDSA publicKey: unexpected type".to_string()))?;
    let private_key: web_sys::CryptoKey = private
        .dyn_into()
        .map_err(|_| CryptoError::new("ECDSA privateKey: unexpected type".to_string()))?;

    return Ok(EcdsaKeyPair { public_key, private_key });
}

/// Подписывает `data` приватным ключом ECDSA.
pub async fn sign(
    private_key: &web_sys::CryptoKey,
    data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("ECDSA"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("hash"), &JsValue::from_str("SHA-256"))
        .map_err(|e| CryptoError::from(e))?;

    let data_obj = bytes_to_object(data);
    let promise = subtle.sign_with_object_and_buffer_source(
        &alg,
        private_key,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}

/// Проверяет подпись `signature` для `data` публичным ключом ECDSA.
pub async fn verify(
    public_key: &web_sys::CryptoKey,
    signature: &[u8],
    data: &[u8],
) -> Result<bool, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("ECDSA"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("hash"), &JsValue::from_str("SHA-256"))
        .map_err(|e| CryptoError::from(e))?;

    let sig_obj = bytes_to_object(signature);
    let data_obj = bytes_to_object(data);
    let promise = subtle.verify_with_object_and_buffer_source_and_buffer_source(
        &alg,
        public_key,
        &sig_obj,
        &data_obj,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return result.as_bool().ok_or_else(|| {
        CryptoError::new("ECDSA verify: expected boolean".to_string())
    });
}

// ============================================================
//                    ECDH
// ============================================================

/// Пара ключей ECDH.
pub struct EcdhKeyPair {
    pub public_key: web_sys::CryptoKey,
    pub private_key: web_sys::CryptoKey,
}

/// Генерирует пару ключей ECDH.
///
/// Web Crypto не позволяет пустой `keyUsages` для `generateKey`:
/// приватный ключ должен получить хотя бы одну роль. Для ECDH это
/// `deriveKey` / `deriveBits`. Публичный ключ всё равно получит `[]` —
/// это неявно делает движок.
pub async fn generate_key_pair_ecdh(
    named_curve: &str,
) -> Result<EcdhKeyPair, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("ECDH"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("namedCurve"), &JsValue::from_str(named_curve))
        .map_err(|e| CryptoError::from(e))?;

    // keyUsages НЕ может быть пустым для generateKey — нужна хотя бы
    // одна роль. Для ECDH это deriveKey или deriveBits.
    let usages = js_sys::Array::new();
    usages.push(&JsValue::from_str("deriveKey"));

    let promise = subtle.generate_key_with_object(&alg, false, &usages)
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
        .map_err(|_| CryptoError::new("ECDH publicKey: unexpected type".to_string()))?;
    let private_key: web_sys::CryptoKey = private
        .dyn_into()
        .map_err(|_| CryptoError::new("ECDH privateKey: unexpected type".to_string()))?;

    return Ok(EcdhKeyPair { public_key, private_key });
}

/// Выводит общий AES-GCM ключ из приватного ключа и публичного ключа собеседника.
pub async fn derive_aes_key(
    my_private: &web_sys::CryptoKey,
    their_public: &web_sys::CryptoKey,
) -> Result<web_sys::CryptoKey, CryptoError> {
    let subtle = super::subtle()?;

    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &JsValue::from_str("name"), &JsValue::from_str("ECDH"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&alg, &JsValue::from_str("public"), their_public)
        .map_err(|e| CryptoError::from(e))?;

    let derived_alg = js_sys::Object::new();
    js_sys::Reflect::set(&derived_alg, &JsValue::from_str("name"), &JsValue::from_str("AES-GCM"))
        .map_err(|e| CryptoError::from(e))?;
    js_sys::Reflect::set(&derived_alg, &JsValue::from_str("length"), &JsValue::from_f64(256.0))
        .map_err(|e| CryptoError::from(e))?;

    let usages = js_sys::Array::new();
    usages.push(&JsValue::from_str("encrypt"));
    usages.push(&JsValue::from_str("decrypt"));

    let promise = subtle.derive_key_with_object_and_object(
        &alg,
        my_private,
        &derived_alg,
        false,
        &usages,
    ).map_err(|e| CryptoError::from(e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;

    let key: web_sys::CryptoKey = result
        .dyn_into()
        .map_err(|_| CryptoError::new("ECDH derived key: unexpected type".to_string()))?;
    return Ok(key);
}