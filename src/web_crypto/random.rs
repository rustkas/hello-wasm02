//! Криптографически стойкие случайные значения.
//!
//! `Crypto.getRandomValues()` — единственный метод Web Crypto,
//! доступный в небезопасном контексте (HTTP).
//! `randomUUID()` — генерация UUID v4.
//!
//! `Crypto::random_uuid()` в web-sys возвращает `String` напрямую
//! (метод синхронный и не может провалиться в нормальных условиях).

use super::CryptoError;

/// Заполняет `len` байт криптостойкой случайностью.
pub fn random_bytes(len: usize) -> Result<Vec<u8>, CryptoError> {
    let crypto = super::crypto_obj()?;
    let mut out = vec![0u8; len];
    // getRandomValues принимает `&mut [u8]` — передаём явный срез.
    let slice: &mut [u8] = out.as_mut_slice();
    crypto
        .get_random_values_with_u8_array(slice)
        .map_err(|e| CryptoError::from(e))?;
    return Ok(out);
}

/// Возвращает случайный UUID v4 в виде строки.
pub fn random_uuid() -> Result<String, CryptoError> {
    let crypto = super::crypto_obj()?;
    return Ok(crypto.random_uuid());
}