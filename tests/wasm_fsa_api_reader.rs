use wasm_bindgen_test::*;
use hello_wasm02::fsa_api_reader::*;

wasm_bindgen_test_configure!(run_in_browser);

fn make_file(content: &[u8], name: &str) -> web_sys::File {
    let u8arr = js_sys::Uint8Array::from(content);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&js_sys::Array::of1(&u8arr)).unwrap();
    web_sys::File::new_with_blob_sequence(&js_sys::Array::of1(&blob), name).unwrap()
}

#[wasm_bindgen_test]
async fn test_read_text_roundtrip() {
    let file = make_file("Hello, FSA!".as_bytes(), "hello.txt");
    let text = read_text(&file).await.unwrap();
    assert_eq!(text, "Hello, FSA!");
}

#[wasm_bindgen_test]
async fn test_read_bytes_roundtrip() {
    let data: Vec<u8> = (0..=255).collect();
    let file = make_file(&data, "bytes.bin");
    let bytes = read_bytes(&file).await.unwrap();
    assert_eq!(bytes, data);
}

#[wasm_bindgen_test]
async fn test_read_slice() {
    let data: Vec<u8> = (0..100).collect();
    let file = make_file(&data, "slice.bin");
    let slice = read_slice(&file, 10.0, 20.0).await.unwrap();
    assert_eq!(slice.len(), 10);
    assert_eq!(slice, &data[10..20]);
}

#[wasm_bindgen_test]
async fn test_read_stream_progress() {
    let data = vec![0xABu8; 128 * 1024];
    let file = make_file(&data, "stream.bin");
    let hits = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let h = hits.clone();
    let outcome = read_stream(&file, move |_| h.set(h.get() + 1), || false).await.unwrap();
    match outcome {
        ReadOutcome::Ok(b) => assert_eq!(b, data),
        _ => panic!("expected Ok"),
    }
    assert!(hits.get() >= 1);
}

#[wasm_bindgen_test]
async fn test_read_stream_cancel() {
    let data = vec![0u8; 256 * 1024];
    let file = make_file(&data, "cancel.bin");
    let outcome = read_stream(&file, |_| {}, || true).await.unwrap();
    assert!(matches!(outcome, ReadOutcome::Cancelled));
}