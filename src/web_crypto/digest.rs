//! SHA-256 / SHA-384 / SHA-512 — криптографические хеши.
//!
//! `SubtleCrypto.digest(algorithm, data)` → `Promise<ArrayBuffer>`.
//! Возвращает ровно `hash_len` байт.
//!
//! В web-sys у `SubtleCrypto` нет метода с именем `digest` —
//! есть `digest_with_str_and_buffer_source` (алгоритм как строка),
//! возвращающий `Result<Promise, JsValue>`: обёртка web-sys может
//! кинуть синхронное исключение, поэтому `Result` нужно развернуть
//! до `JsFuture::from`.

use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use super::CryptoError;
use super::util::{bytes_to_object, jsvalue_to_bytes, hex_encode};

pub const SHA256_LEN: usize = 32;
pub const SHA384_LEN: usize = 48;
pub const SHA512_LEN: usize = 64;

/// Алгоритм хеширования.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlg {
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlg {
    pub fn as_str(self) -> &'static str {
        return match self {
            HashAlg::Sha256 => "SHA-256",
            HashAlg::Sha384 => "SHA-384",
            HashAlg::Sha512 => "SHA-512",
        };
    }
    pub fn digest_len(self) -> usize {
        return match self {
            HashAlg::Sha256 => SHA256_LEN,
            HashAlg::Sha384 => SHA384_LEN,
            HashAlg::Sha512 => SHA512_LEN,
        };
    }
}

/// Считает хеш от `data`.
pub async fn digest(alg: HashAlg, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let subtle = super::subtle()?;
    let data_obj = bytes_to_object(data);
    // web-sys возвращает Result<Promise, JsValue> — разворачиваем.
    let promise: js_sys::Promise = subtle
        .digest_with_str_and_buffer_source(alg.as_str(), &data_obj)
        .map_err(|e| CryptoError::from(e))?;
    let result: JsValue = JsFuture::from(promise)
        .await
        .map_err(|e| CryptoError::from(e))?;
    return jsvalue_to_bytes(&result).map_err(|e| CryptoError::new(e));
}

/// Удобная обёртка: hex-строка хеша.
pub async fn digest_hex(alg: HashAlg, data: &[u8]) -> Result<String, CryptoError> {
    let bytes = digest(alg, data).await?;
    return Ok(hex_encode(&bytes));
}