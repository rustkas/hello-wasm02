//! WASM-тесты главы 41: WebAuthn.
//!
//! Запуск:
//!   wasm-pack test --chrome --headless -- --test wasm_webauthn
//!
//! WebAuthn требует secure context (HTTPS или localhost) и реального
//! user gesture. Тесты, требующие диалога аутентификатора, помечены
//! #[ignore] — их нельзя выполнить в headless-браузере.

use wasm_bindgen_test::*;

use hello_wasm02::webauthn::{
    capability,
    encoding::{base64url_to_bytes, bytes_to_base64url, hex_preview},
    model::RpConfig,
};

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// 1. base64url — кодирование/декодирование
// ============================================================

#[wasm_bindgen_test]
fn wasm_b64url_hello_roundtrip() {
    let bytes = b"hello";
    let s = bytes_to_base64url(bytes);
    assert_eq!(s, "aGVsbG8");
    let back = base64url_to_bytes(&s).unwrap();
    assert_eq!(back, bytes);
}

#[wasm_bindgen_test]
fn wasm_b64url_no_padding() {
    // 1 байт → 2 символа base64url без '='.
    let s = bytes_to_base64url(b"f");
    assert!(!s.contains('='));
    assert_eq!(s, "Zg");
}

#[wasm_bindgen_test]
fn wasm_b64url_url_safe_chars() {
    // 0xFB 0xFF в стандартном base64 → "+/8=",
    // в base64url → "-_8".
    let bytes: [u8; 2] = [0xFB, 0xFF];
    let s = bytes_to_base64url(&bytes);
    assert!(!s.contains('+'));
    assert!(!s.contains('/'));
    assert_eq!(s, "-_8");
}

#[wasm_bindgen_test]
fn wasm_b64url_empty() {
    let s = bytes_to_base64url(&[]);
    assert_eq!(s, "");
    let back = base64url_to_bytes("").unwrap();
    assert!(back.is_empty());
}

#[wasm_bindgen_test]
fn wasm_b64url_decode_with_padding() {
    // Стандартная base64 со '=' тоже должна декодироваться.
    let back = base64url_to_bytes("aGVsbG8=").unwrap();
    assert_eq!(back, b"hello");
}

#[wasm_bindgen_test]
fn wasm_b64url_decode_invalid_length() {
    // Длина %4 == 1 — невозможно.
    let r = base64url_to_bytes("abcde");
    assert!(r.is_err());
}

// ============================================================
// 2. hex_preview
// ============================================================

#[wasm_bindgen_test]
fn wasm_hex_preview_short() {
    let bytes = [0xAB, 0xCD];
    let s = hex_preview(&bytes, 4);
    assert_eq!(s, "ab cd");
}

#[wasm_bindgen_test]
fn wasm_hex_preview_truncated() {
    let bytes = [0x00, 0xAB, 0xCD, 0xEF];
    let s = hex_preview(&bytes, 2);
    assert_eq!(s, "00 ab …");
}

#[wasm_bindgen_test]
fn wasm_hex_preview_empty() {
    let s = hex_preview(&[], 4);
    assert_eq!(s, "");
}

// ============================================================
// 3. Capability detection
// ============================================================

#[wasm_bindgen_test]
async fn wasm_webauthn_is_available() {
    // На localhost (secure context) WebAuthn должен быть доступен.
    assert!(
        capability::is_available(),
        "WebAuthn должен быть доступен на localhost",
    );
}

#[wasm_bindgen_test]
async fn wasm_platform_authenticator_query() {
    // Возвращает Ok(bool) — значение зависит от окружения.
    let result = capability::has_platform_authenticator().await;
    assert!(result.is_ok(), "isUVPAA должен вернуть Result");
}

#[wasm_bindgen_test]
async fn wasm_conditional_mediation_query() {
    let result = capability::is_conditional_mediation_available().await;
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
async fn wasm_capability_summary_contains_webauthn() {
    let summary = capability::capability_summary().await;
    assert!(!summary.is_empty());
    assert!(summary.contains("WebAuthn"));
}

// ============================================================
// 4. RpConfig
// ============================================================

#[wasm_bindgen_test]
fn wasm_rp_config_origin() {
    let cfg = RpConfig::new("example.com", "Example");
    assert_eq!(cfg.origin, "https://example.com");
    assert_eq!(cfg.rp_id, "example.com");
    assert_eq!(cfg.rp_name, "Example");
}

#[wasm_bindgen_test]
fn wasm_rp_config_localhost() {
    let cfg = RpConfig::new("localhost", "Local Dev");
    assert_eq!(cfg.origin, "https://localhost");
}

// ============================================================
// 5. Операции, требующие user gesture
// ============================================================

/// Реальная регистрация требует клика пользователя. В headless
/// Chrome диалог не появится — тест помечен #[ignore].
#[wasm_bindgen_test]
#[ignore]
async fn wasm_register_requires_user_gesture() {
    // Запускается вручную:
    //   wasm-pack test --chrome --headless -- --test wasm_webauthn --include-ignored
    // В headless всё равно не сработает — нужен обычный браузер.
}

/// Аналогично для аутентификации.
#[wasm_bindgen_test]
#[ignore]
async fn wasm_authenticate_requires_user_gesture() {
}

// ============================================================
// 6. Прямая проверка возвращаемых типов
// ============================================================

#[wasm_bindgen_test]
async fn wasm_credentials_container_is_object() {
    use hello_wasm02::webauthn::credentials_container;
    // На localhost container должен быть доступен.
    let result = credentials_container();
    assert!(result.is_ok(), "navigator.credentials должен быть доступен");
    let creds = result.unwrap();
    assert!(!creds.is_undefined());
    assert!(!creds.is_null());
}

// ============================================================
// 7. Проверка формата challenge
// ============================================================

#[wasm_bindgen_test]
fn wasm_challenge_base64url_length() {
    // 32 случайных байта → 43 символа base64url (без padding).
    let bytes = [0u8; 32];
    let s = bytes_to_base64url(&bytes);
    // 32 байта = 256 бит → ceil(32*8/6) = 43 символа.
    assert_eq!(s.len(), 43);
}