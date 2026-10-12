//! Wasm-тесты `clip_core`: создание Blob, классификация ошибок,
//! доступность API. Реальные операции с буфером обмена тестируются
//! только там, где headless Chrome позволяет (см. комментарии).
//!
//! ПРАВИЛО: headless-тесты не могут эмулировать user gesture.
//! `clipboard.writeText` в headless может пройти, если разрешение
//! выдано через `PermissionsHelper.setPermission`. Тесты, требующие
//! интерактивного разрешения, помечены `#[ignore]`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use hello_wasm02::clip_core::{self, ClipError};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- классификация ошибок ----------------

#[wasm_bindgen_test]
fn classify_not_allowed() {
    let e = web_sys::DomException::new_with_message_and_name(
        "denied",
        "NotAllowedError",
    )
    .expect("make DomException");
    let js: JsValue = e.into();
    assert_eq!(clip_core::classify_clip_error(&js), ClipError::NotAllowed);
}

#[wasm_bindgen_test]
fn classify_security() {
    let e = web_sys::DomException::new_with_message_and_name(
        "insecure",
        "SecurityError",
    )
    .unwrap();
    let js: JsValue = e.into();
    assert_eq!(clip_core::classify_clip_error(&js), ClipError::Security);
}

#[wasm_bindgen_test]
fn classify_unknown() {
    let e = JsValue::from_str("strange");
    let kind = clip_core::classify_clip_error(&e);
    assert_eq!(kind, ClipError::Other("strange".to_string()));
}

// ---------------- Blob ----------------

#[wasm_bindgen_test]
fn text_blob_has_correct_mime() {
    let blob = clip_core::text_blob("hello", "text/plain").expect("text_blob");
    assert_eq!(blob.type_(), "text/plain");
}

#[wasm_bindgen_test]
fn text_blob_has_correct_size() {
    let blob = clip_core::text_blob("hello", "text/plain").expect("text_blob");
    assert_eq!(blob.size() as u32, 5);
}

#[wasm_bindgen_test]
fn bytes_blob_has_correct_mime() {
    let bytes: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];
    let blob = clip_core::bytes_blob(&bytes, "image/png").expect("bytes_blob");
    assert_eq!(blob.type_(), "image/png");
    assert_eq!(blob.size() as u32, 4);
}

#[wasm_bindgen_test]
async fn blob_to_text_roundtrip() {
    let blob = clip_core::text_blob("привет мир", "text/plain").unwrap();
    let text = clip_core::blob_to_text(&blob).await.unwrap();
    assert_eq!(text, "привет мир");
}

#[wasm_bindgen_test]
async fn blob_to_bytes_roundtrip() {
    let bytes: Vec<u8> = (0..32u8).collect();
    let blob = clip_core::bytes_blob(&bytes, "application/octet-stream").unwrap();
    let back = clip_core::blob_to_bytes(&blob).await.unwrap();
    assert_eq!(back, bytes);
}

// ---------------- доступность ----------------

#[wasm_bindgen_test]
fn clipboard_is_available_in_headless() {
    // В headless Chrome на localhost `navigator.clipboard` доступен.
    assert!(clip_core::is_available(), "clipboard недоступен в headless");
}

// ---------------- writeText / readText ----------------
//
// В headless Chrome разрешения выдаются через PermissionsHelper
// в webdriver.json. Если не выданы — writeText может упасть
// с NotAllowedError. Тест помечен ignore и запускается вручную.

#[wasm_bindgen_test]
#[ignore = "требует clipboard-write permission в headless"]
async fn write_read_text_roundtrip() {
    clip_core::write_text("test-clipboard-123").await.expect("writeText");
    let back = clip_core::read_text().await.expect("readText");
    assert_eq!(back, "test-clipboard-123");
}