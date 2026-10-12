#![cfg(target_arch = "wasm32")]

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen_test::*;

use hello_wasm02::blob_processor::{
    choose_chunk_size, make_json_blob, process_blob, read_blob_bytes, read_slice_bytes,
    render_histogram_png, Metadata, CANCELLED,
};

wasm_bindgen_test_configure!(run_in_browser);

fn blob_from_bytes(bytes: &[u8], mime: &str) -> web_sys::Blob {
    use js_sys::{Array, Uint8Array};
    use web_sys::{Blob, BlobPropertyBag};
    let u8arr = Uint8Array::from(bytes);
    let parts = Array::new();
    parts.push(u8arr.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type(mime);
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag).unwrap()
}

/// Флаг отмены, который никогда не срабатывает.
fn no_cancel() -> Rc<Cell<bool>> {
    Rc::new(Cell::new(false))
}

#[wasm_bindgen_test]
fn choose_chunk_size_is_bounded() {
    // Маленький файл → минимум.
    assert_eq!(choose_chunk_size(1024), 32 * 1024);
    // Пустой → минимум (для определённости).
    assert_eq!(choose_chunk_size(0), 32 * 1024);
    // 4 МБ → 32 КБ (ровно целевое число чанков).
    assert_eq!(choose_chunk_size(4 * 1024 * 1024), 32 * 1024);
    // 64 МБ → 512 КБ.
    assert_eq!(choose_chunk_size(64 * 1024 * 1024), 512 * 1024);
    // 1 ГБ → 8 МБ (верхняя граница; 1 ГБ влезает в usize даже на wasm32).
    let gb1: usize = 1024 * 1024 * 1024;
    assert_eq!(choose_chunk_size(gb1), 8 * 1024 * 1024);
}
#[wasm_bindgen_test]
async fn read_blob_returns_same_bytes() {
    let bytes = b"hello, blob!";
    let blob = blob_from_bytes(bytes, "text/plain");
    let roundtrip = read_blob_bytes(&blob).await.expect("read");
    assert_eq!(roundtrip, bytes);
}

#[wasm_bindgen_test]
async fn read_slice_extracts_subrange() {
    let blob = blob_from_bytes(b"0123456789", "text/plain");
    let mid = read_slice_bytes(&blob, 3.0, 7.0).await.expect("slice");
    assert_eq!(mid, b"3456");
}

#[wasm_bindgen_test]
async fn process_empty_blob_is_safe() {
    let blob = blob_from_bytes(&[], "application/octet-stream");
    // 4 аргумента: blob, chunk_size, cancel, on_progress
    let meta = process_blob(&blob, 1024, no_cancel(), |_, _, _| {})
        .await
        .expect("empty");
    assert_eq!(meta.total_bytes, 0);
    assert_eq!(meta.chunks, 0);
    assert_eq!(meta.unique_bytes, 0);
    assert_eq!(meta.min_byte, 0);
}

#[wasm_bindgen_test]
async fn process_counts_bytes_correctly() {
    let bytes: Vec<u8> = (0u16..1024).map(|i| (i & 0xFF) as u8).collect();
    let blob = blob_from_bytes(&bytes, "application/octet-stream");

    let meta = process_blob(&blob, 128, no_cancel(), |_, _, _| {})
        .await
        .expect("process");
    assert_eq!(meta.total_bytes, 1024);
    assert_eq!(meta.chunks, 8);
    assert_eq!(meta.sum, bytes.iter().map(|&b| b as u64).sum::<u64>());
    assert_eq!(meta.min_byte, 0);
    assert_eq!(meta.max_byte, 255);
    assert!(meta.unique_bytes > 0 && meta.unique_bytes <= 256);
    assert_eq!(meta.histogram[0], 4);
    assert_eq!(meta.histogram[255], 4);
}

#[wasm_bindgen_test]
async fn process_reports_progress() {
    let bytes = vec![0u8; 64 * 1024];
    let blob = blob_from_bytes(&bytes, "application/octet-stream");

    let mut calls: Vec<(usize, usize)> = Vec::new();
    process_blob(&blob, 4096, no_cancel(), |done, total, _hist| {
        calls.push((done, total));
    })
    .await
    .expect("process");

    assert!(calls.len() >= 2, "expected start + end, got {}", calls.len());
    assert_eq!(calls.first(), Some(&(0, 64 * 1024)));
    assert_eq!(calls.last(), Some(&(64 * 1024, 64 * 1024)));
    for (done, total) in &calls {
        assert!(*done <= *total);
        assert_eq!(*total, 64 * 1024);
    }
}

#[wasm_bindgen_test]
async fn process_provides_histogram_in_callback() {
    let bytes = vec![0u8; 512];
    let blob = blob_from_bytes(&bytes, "application/octet-stream");

    let mut final_hist = [0u32; 256];
    process_blob(&blob, 256, no_cancel(), |_, _, hist| {
        final_hist = *hist;
    })
    .await
    .expect("process");

    // Все байты нулевые → histogram[0] == 512.
    assert_eq!(final_hist[0], 512);
}

#[wasm_bindgen_test]
async fn cancel_stops_processing() {
    let bytes = vec![0u8; 1_000_000];
    let blob = blob_from_bytes(&bytes, "application/octet-stream");

    let cancel = Rc::new(Cell::new(false));
    let cancel_for_cb = cancel.clone();
    let mut tick = 0u32;

    let result = process_blob(&blob, 4096, cancel, move |_, _, _| {
        tick += 1;
        if tick == 1 {
            cancel_for_cb.set(true);
        }
    })
    .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().as_string().unwrap_or_default();
    assert_eq!(msg, CANCELLED);
}

#[wasm_bindgen_test]
async fn render_histogram_returns_png_blob() {
    let mut hist = [0u32; 256];
    hist[0] = 10;
    hist[128] = 5;
    hist[255] = 7;
    let blob = render_histogram_png(&hist, 400, 200).await.expect("png");
    assert_eq!(blob.type_(), "image/png");
    assert!(blob.size() > 0.0);
}

#[wasm_bindgen_test]
async fn json_blob_has_expected_content_type() {
    let meta = Metadata::default();
    let blob = make_json_blob(&meta).expect("json blob");
    assert_eq!(blob.type_(), "application/json");
    assert!(blob.size() > 0.0);
}

#[wasm_bindgen_test]
fn metadata_json_is_parseable_shape() {
    let mut meta = Metadata::default();
    meta.total_bytes = 42;
    meta.chunks = 1;
    meta.chunk_size = 64;
    meta.sha256 = String::from("deadbeefcafebabe");
    meta.histogram[0] = 42;

    let text = meta.to_json();
    assert!(text.contains("\"total_bytes\": 42"));
    assert!(text.contains("\"chunks\": 1"));
    assert!(text.contains("\"sha256\": \"deadbeefcafebabe\""));
    assert!(text.contains("\"histogram\": ["));
}