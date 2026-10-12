use wasm_bindgen_test::*;
use hello_wasm02::blob_slice::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Создаёт Blob из последовательности байт.
fn make_blob(bytes: &[u8]) -> web_sys::Blob {
    let uint8 = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&uint8.buffer());
    return web_sys::Blob::new_with_u8_array_sequence(&parts)
        .expect("Blob::new_with_u8_array_sequence");
}

#[wasm_bindgen_test]
fn test_slice_blob_size() {
    let blob = make_blob(&[0u8; 100]);
    let slice = slice_blob(&blob, 10.0, 30.0).expect("slice_blob");
    assert_eq!(slice.size(), 20.0);
}

#[wasm_bindgen_test]
fn test_slice_beyond_end_clamps() {
    // end больше размера — срез идёт до конца.
    let blob = make_blob(&[0u8; 50]);
    let slice = slice_blob(&blob, 40.0, 1000.0).expect("slice_blob");
    assert_eq!(slice.size(), 10.0);
}

#[wasm_bindgen_test]
fn test_slice_start_beyond_size_is_empty() {
    // start >= size — пустой Blob.
    let blob = make_blob(&[0u8; 50]);
    let slice = slice_blob(&blob, 100.0, 200.0).expect("slice_blob");
    assert_eq!(slice.size(), 0.0);
}

#[wasm_bindgen_test]
async fn test_read_first_n_exact() {
    let bytes: Vec<u8> = (0..100).collect();
    let blob = make_blob(&bytes);

    let read = read_first_n(&blob, 32).await.expect("read_first_n");
    assert_eq!(read.len(), 32);
    assert_eq!(read[0], 0);
    assert_eq!(read[31], 31);
}

#[wasm_bindgen_test]
async fn test_read_first_n_truncates() {
    let bytes: Vec<u8> = (0..10).collect();
    let blob = make_blob(&bytes);

    let read = read_first_n(&blob, 100).await.expect("read_first_n");
    assert_eq!(read.len(), 10);
}

#[wasm_bindgen_test]
async fn test_read_slice_middle() {
    let bytes: Vec<u8> = (0..100).collect();
    let blob = make_blob(&bytes);

    let read = read_slice(&blob, 20.0, 25.0).await.expect("read_slice");
    assert_eq!(read, vec![20u8, 21, 22, 23, 24]);
}

#[wasm_bindgen_test]
fn test_chunk_count() {
    assert_eq!(chunk_count(1000.0, 100.0), 10);
    assert_eq!(chunk_count(1050.0, 100.0), 11);
    assert_eq!(chunk_count(0.0, 100.0), 0);
    assert_eq!(chunk_count(100.0, 0.0), 0);
    assert_eq!(chunk_count(100.0, -1.0), 0);
}

#[wasm_bindgen_test]
fn test_split_into_chunks_exact() {
    let blob = make_blob(&[0u8; 300]);
    let chunks = split_into_chunks(&blob, 100.0);
    assert_eq!(chunks.len(), 3);
    for chunk in &chunks {
        assert_eq!(chunk.size(), 100.0);
    }
}

#[wasm_bindgen_test]
fn test_split_into_chunks_with_tail() {
    let blob = make_blob(&[0u8; 250]);
    let chunks = split_into_chunks(&blob, 100.0);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].size(), 100.0);
    assert_eq!(chunks[1].size(), 100.0);
    assert_eq!(chunks[2].size(), 50.0);
}

#[wasm_bindgen_test]
fn test_split_into_chunks_invalid_size() {
    let blob = make_blob(&[0u8; 100]);
    let chunks = split_into_chunks(&blob, 0.0);
    assert!(chunks.is_empty());
}

#[wasm_bindgen_test]
async fn test_concat_roundtrip() {
    let original: Vec<u8> = (0..100u8).collect();
    let blob = make_blob(&original);
    let chunks = split_into_chunks(&blob, 30.0);
    assert_eq!(chunks.len(), 4);

    let recombined = concat_blobs(&chunks, "application/octet-stream")
        .expect("concat_blobs");
    assert_eq!(recombined.size(), 100.0);

    // Читаем результат и сравниваем побайтово.
    let bytes = read_first_n(&recombined, 100).await.expect("read");
    assert_eq!(bytes, original);
}

#[wasm_bindgen_test]
async fn test_chunks_preserve_content() {
    // Проверяем, что содержимое чанков не портится.
    let original: Vec<u8> = (0..60u8).collect();
    let blob = make_blob(&original);
    let chunks = split_into_chunks(&blob, 20.0);

    let c0 = read_first_n(&chunks[0], 20).await.expect("c0");
    let c1 = read_first_n(&chunks[1], 20).await.expect("c1");
    let c2 = read_first_n(&chunks[2], 20).await.expect("c2");

    assert_eq!(c0, (0..20u8).collect::<Vec<_>>());
    assert_eq!(c1, (20..40u8).collect::<Vec<_>>());
    assert_eq!(c2, (40..60u8).collect::<Vec<_>>());
}