//! WASM-тесты главы 38: Web Crypto API.
//!
//! Запуск:
//!   wasm-pack test --chrome --headless -- --test wasm_web_crypto
//!
//! Web Crypto API доступен только в secure context (https:// или localhost).
//! Все тесты — асинхронные, выполняются в реальном браузере.

use wasm_bindgen_test::*;

use hello_wasm02::web_crypto::{
    aes, digest, ecdsa, hmac, keys, random, rsa,
    digest::HashAlg,
};

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// 1. DIGEST — SHA-256 / SHA-384 / SHA-512
// ============================================================

#[wasm_bindgen_test]
async fn wasm_sha256_empty() {
    let hex = digest::digest_hex(HashAlg::Sha256, b"").await.unwrap();
    assert_eq!(
        hex,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[wasm_bindgen_test]
async fn wasm_sha256_abc() {
    let hex = digest::digest_hex(HashAlg::Sha256, b"abc").await.unwrap();
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[wasm_bindgen_test]
async fn wasm_sha256_len() {
    let bytes = digest::digest(HashAlg::Sha256, b"hello").await.unwrap();
    assert_eq!(bytes.len(), digest::SHA256_LEN);
}

#[wasm_bindgen_test]
async fn wasm_sha384_len() {
    let bytes = digest::digest(HashAlg::Sha384, b"hello").await.unwrap();
    assert_eq!(bytes.len(), digest::SHA384_LEN);
}

#[wasm_bindgen_test]
async fn wasm_sha512_len() {
    let bytes = digest::digest(HashAlg::Sha512, b"hello").await.unwrap();
    assert_eq!(bytes.len(), digest::SHA512_LEN);
}

// ============================================================
// 2. HMAC — sign / verify
// ============================================================

#[wasm_bindgen_test]
async fn wasm_hmac_sign_verify_roundtrip() {
    let key = hmac::generate_key(HashAlg::Sha256, 256, true).await.unwrap();
    let data = b"message to authenticate";
    let sig = hmac::sign(&key, data).await.unwrap();
    assert_eq!(sig.len(), 32);
    let ok = hmac::verify(&key, &sig, data).await.unwrap();
    assert!(ok);
}

#[wasm_bindgen_test]
async fn wasm_hmac_verify_wrong_data() {
    let key = hmac::generate_key(HashAlg::Sha256, 256, true).await.unwrap();
    let sig = hmac::sign(&key, b"correct").await.unwrap();
    let ok = hmac::verify(&key, &sig, b"tampered").await.unwrap();
    assert!(!ok);
}

// ============================================================
// 3. AES-GCM — encrypt / decrypt
// ============================================================

#[wasm_bindgen_test]
async fn wasm_aes_gcm_roundtrip() {
    let key = aes::generate_key(256, true).await.unwrap();
    let plaintext = b"secret message";
    let (ciphertext, iv) = aes::encrypt(&key, plaintext).await.unwrap();
    assert_ne!(ciphertext, plaintext);
    let decrypted = aes::decrypt(&key, &ciphertext, &iv).await.unwrap();
    assert_eq!(decrypted, plaintext);
}

#[wasm_bindgen_test]
async fn wasm_aes_gcm_tampered_fails() {
    let key = aes::generate_key(256, true).await.unwrap();
    let (mut ciphertext, iv) = aes::encrypt(&key, b"data").await.unwrap();
    ciphertext[0] ^= 0xFF;
    let result = aes::decrypt(&key, &ciphertext, &iv).await;
    assert!(result.is_err());
}

// ============================================================
// 4. RSA-OAEP — encrypt / decrypt
// ============================================================

#[wasm_bindgen_test]
async fn wasm_rsa_oaep_roundtrip() {
    let kp = rsa::generate_key_pair(rsa::RSA_2048, HashAlg::Sha256)
        .await
        .unwrap();
    let plaintext = b"hello rsa";
    let ciphertext = rsa::encrypt(&kp.public_key, plaintext).await.unwrap();
    assert_ne!(ciphertext, plaintext);
    let decrypted = rsa::decrypt(&kp.private_key, &ciphertext).await.unwrap();
    assert_eq!(decrypted, plaintext);
}

// ============================================================
// 5. ECDSA — sign / verify
// ============================================================

#[wasm_bindgen_test]
async fn wasm_ecdsa_sign_verify_roundtrip() {
    let kp = ecdsa::generate_key_pair(ecdsa::P256).await.unwrap();
    let data = b"signed data";
    let sig = ecdsa::sign(&kp.private_key, data).await.unwrap();
    let ok = ecdsa::verify(&kp.public_key, &sig, data).await.unwrap();
    assert!(ok);
}

#[wasm_bindgen_test]
async fn wasm_ecdsa_verify_wrong_data() {
    let kp = ecdsa::generate_key_pair(ecdsa::P256).await.unwrap();
    let sig = ecdsa::sign(&kp.private_key, b"original").await.unwrap();
    let ok = ecdsa::verify(&kp.public_key, &sig, b"tampered").await.unwrap();
    assert!(!ok);
}

// ============================================================
// 6. ECDH — derive shared AES key
// ============================================================

#[wasm_bindgen_test]
async fn wasm_ecdh_shared_key_matches() {
    let alice = ecdsa::generate_key_pair_ecdh(ecdsa::P256).await.unwrap();
    let bob = ecdsa::generate_key_pair_ecdh(ecdsa::P256).await.unwrap();

    let alice_key = ecdsa::derive_aes_key(&alice.private_key, &bob.public_key)
        .await
        .unwrap();
    let bob_key = ecdsa::derive_aes_key(&bob.private_key, &alice.public_key)
        .await
        .unwrap();

    let plaintext = b"shared secret message";
    let (ciphertext, iv) = aes::encrypt(&alice_key, plaintext).await.unwrap();
    let decrypted = aes::decrypt(&bob_key, &ciphertext, &iv).await.unwrap();
    assert_eq!(decrypted, plaintext);
}

// ============================================================
// 7. RANDOM — getRandomValues, UUID
// ============================================================

#[wasm_bindgen_test]
async fn wasm_random_bytes_len() {
    let bytes = random::random_bytes(32).unwrap();
    assert_eq!(bytes.len(), 32);
}

#[wasm_bindgen_test]
async fn wasm_random_bytes_different() {
    let a = random::random_bytes(32).unwrap();
    let b = random::random_bytes(32).unwrap();
    assert_ne!(a, b);
}

#[wasm_bindgen_test]
async fn wasm_random_uuid_format() {
    let uuid = random::random_uuid().unwrap();
    assert_eq!(uuid.len(), 36);
    assert_eq!(uuid.chars().filter(|c| *c == '-').count(), 4);
}

// ============================================================
// 8. importKey / exportKey
// ============================================================

#[wasm_bindgen_test]
async fn wasm_export_aes_raw() {
    let key = aes::generate_key(256, true).await.unwrap();
    let raw = keys::export_raw(&key).await.unwrap();
    assert_eq!(raw.len(), 32);
}

#[wasm_bindgen_test]
async fn wasm_export_hmac_jwk() {
    let key = hmac::generate_key(HashAlg::Sha256, 256, true).await.unwrap();
    let jwk = keys::export_jwk(&key).await.unwrap();
    assert!(jwk.contains("\"k\""));
    assert!(jwk.contains("\"alg\""));
}