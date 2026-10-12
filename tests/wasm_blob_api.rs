//! Тесты `src/blob_api.rs`.
//!
//! Проверяем: создание Blob из байтов, корректный MIME, формат
//! object URL, идемпотентность revoke, отсутствие паники при
//! скачивании.

use wasm_bindgen_test::*;

use hello_wasm02::blob_api::{
    blob_mime, blob_size, create_blob, create_object_url, download_url,
    revoke_object_url, shorten_url,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// create_blob
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_create_blob_empty() {
    let blob = create_blob(&[], "application/octet-stream").expect("blob");
    assert_eq!(blob_size(&blob), 0.0);
    assert_eq!(blob_mime(&blob), "application/octet-stream");
}

#[wasm_bindgen_test]
fn test_create_blob_with_bytes() {
    let data = b"hello, blob!";
    let blob = create_blob(data, "text/plain").expect("blob");
    assert_eq!(blob_size(&blob), data.len() as f64);
    assert_eq!(blob_mime(&blob), "text/plain");
}

#[wasm_bindgen_test]
fn test_create_blob_with_utf8_mime() {
    let data = "привет мир".as_bytes();
    let blob = create_blob(data, "text/plain;charset=utf-8").expect("blob");
    assert_eq!(blob_size(&blob), data.len() as f64);
    assert_eq!(blob_mime(&blob), "text/plain;charset=utf-8");
}

// ---------------------------------------------------------------------------
// object URL
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_create_object_url_format() {
    let blob = create_blob(b"test", "text/plain").expect("blob");
    let url = create_object_url(&blob).expect("url");
    assert!(url.starts_with("blob:"), "URL должен начинаться с blob: — {url}");
    revoke_object_url(&url);
}

#[wasm_bindgen_test]
fn test_revoke_is_idempotent() {
    let blob = create_blob(b"x", "text/plain").expect("blob");
    let url = create_object_url(&blob).expect("url");
    // Три вызова подряд — все Ok, без паники.
    revoke_object_url(&url);
    revoke_object_url(&url);
    revoke_object_url(&url);
}

// ---------------------------------------------------------------------------
// download
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_download_url_does_not_panic() {
    let blob = create_blob(b"test", "text/plain").expect("blob");
    let url = create_object_url(&blob).expect("url");
    // В headless Chrome фактического скачивания не будет, но
    // ошибок быть не должно. Это проверка «не паникует».
    download_url(&url, "test.txt").expect("download");
    revoke_object_url(&url);
}

// ---------------------------------------------------------------------------
// shorten_url
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_shorten_url_short_unchanged() {
    let short = "blob:abc";
    assert_eq!(shorten_url(short), short);
}

#[wasm_bindgen_test]
fn test_shorten_url_long_truncated() {
    let long = "blob:http://localhost:5173/a1b2c3d4-e5f6-7890-abcd-ef1234567890";
    let out = shorten_url(long);
    assert!(out.len() < long.len());
    assert!(out.starts_with('…'));
}