//! Тесты `src/blob_container.rs`.
//!
//! * Чистые функции (`hex_dump`, `crc32`) — host-тесты, не
//!   требуют браузера.
//! * JS-функции (`from_text`, `read_text`, `slice`, `is_file`) —
//!   WASM-тесты в headless Chrome.

use wasm_bindgen_test::*;

use hello_wasm02::blob_container::{
    blob_mime, blob_size, crc32, from_bytes, from_text, hex_dump, is_file,
    read_bytes, read_first_chunk, read_text, slice,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Чистая логика: hex_dump
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_hex_dump_empty() {
    let s = hex_dump(&[], 256);
    assert!(s.is_empty());
}

#[wasm_bindgen_test]
fn test_hex_dump_hello() {
    let s = hex_dump(b"Hello", 256);
    // Одна строка + перевод строки.
    assert!(s.starts_with("00000000  48 65 6c 6c 6f"));
    assert!(s.contains("|Hello"));
}

#[wasm_bindgen_test]
fn test_hex_dump_truncated() {
    let bytes: Vec<u8> = (0..100).collect();
    let s = hex_dump(&bytes, 32);
    // 32 байта = 2 строки по 16.
    assert!(s.contains("00000000"));
    assert!(s.contains("00000010"));
    assert!(!s.contains("00000020"));
    assert!(s.contains("скрыто"));
}

#[wasm_bindgen_test]
fn test_hex_dump_non_printable() {
    let bytes = b"\x00\x01\x02\x03 hello";
    let s = hex_dump(bytes, 256);
    // Непечатаемые — точки.
    assert!(s.contains("|.... hello"));
}

// ---------------------------------------------------------------------------
// Чистая логика: crc32
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_crc32_empty() {
    assert_eq!(crc32(&[]), 0);
}

#[wasm_bindgen_test]
fn test_crc32_hello() {
    // Известное значение CRC32("hello") = 0x3610A686.
    assert_eq!(crc32(b"hello"), 0x3610_A686);
}

#[wasm_bindgen_test]
fn test_crc32_deterministic() {
    let a = crc32(b"the quick brown fox");
    let b = crc32(b"the quick brown fox");
    assert_eq!(a, b);
}

#[wasm_bindgen_test]
fn test_crc32_changes_on_edit() {
    let a = crc32(b"hello");
    let b = crc32(b"hello!");
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------------
// from_text / from_bytes / чтение
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_from_text_roundtrip() {
    let text = "Hello, Blob!";
    let blob = from_text(text, "text/plain;charset=utf-8").expect("blob");
    assert_eq!(blob_size(&blob), text.len() as f64);
    assert_eq!(blob_mime(&blob), "text/plain;charset=utf-8");

    let back = read_text(&blob).await.expect("read");
    assert_eq!(back, text);
}

#[wasm_bindgen_test]
async fn test_from_bytes_roundtrip() {
    let data: Vec<u8> = (0..=255).collect();
    let blob = from_bytes(&data, "application/octet-stream").expect("blob");
    assert_eq!(blob_size(&blob), 256.0);

    let back = read_bytes(&blob).await.expect("read");
    assert_eq!(back, data);
}

#[wasm_bindgen_test]
async fn test_read_first_chunk() {
    let data: Vec<u8> = (0..1024).map(|i| (i & 0xFF) as u8).collect();
    let blob = from_bytes(&data, "application/octet-stream").expect("blob");

    let chunk = read_first_chunk(&blob, 4096).await.expect("stream");
    // Blob.stream() может отдать весь Blob одним chunk или часть.
    assert!(!chunk.is_empty(), "chunk не пустой");
    assert!(chunk.len() <= data.len(), "chunk не больше исходного");
    // Байты совпадают с началом.
    assert_eq!(&chunk[..], &data[..chunk.len()]);
}

// ---------------------------------------------------------------------------
// slice
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_slice_roundtrip() {
    let data: Vec<u8> = (0..1024).map(|i| (i & 0xFF) as u8).collect();
    let blob = from_bytes(&data, "application/octet-stream").expect("blob");

    let part = slice(&blob, 256.0, 512.0).expect("slice");
    assert_eq!(blob_size(&part), 256.0);

    let bytes = read_bytes(&part).await.expect("read");
    assert_eq!(bytes.len(), 256);
    assert_eq!(bytes, data[256..512]);
}

#[wasm_bindgen_test]
async fn test_slice_sum_equals_total() {
    let data: Vec<u8> = (0..1024).map(|i| (i & 0xFF) as u8).collect();
    let blob = from_bytes(&data, "application/octet-stream").expect("blob");

    let total = blob_size(&blob);
    let mut sum = 0.0;
    let mut offset = 0.0;
    while offset < total {
        let end = (offset + 256.0).min(total);
        let part = slice(&blob, offset, end).expect("slice");
        sum += blob_size(&part);
        offset = end;
    }
    assert_eq!(sum, total, "сумма размеров кусков = размер родителя");
}

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_is_file_on_plain_blob() {
    let blob = from_text("hi", "text/plain").expect("blob");
    assert!(!is_file(&blob), "обычный Blob — не File");
}

#[wasm_bindgen_test]
fn test_is_file_on_undefined() {
    assert!(!is_file(&wasm_bindgen::JsValue::UNDEFINED));
    assert!(!is_file(&wasm_bindgen::JsValue::NULL));
}