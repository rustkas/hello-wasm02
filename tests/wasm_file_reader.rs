use wasm_bindgen_test::*;
use hello_wasm02::file_reader::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_read_as_text() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"Hello, WASM!".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let text = read_as_text(&blob).await.expect("read_as_text");
    assert_eq!(text, "Hello, WASM!");
}

#[wasm_bindgen_test]
async fn test_read_as_text_hello() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"Hello, WASM!".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let text = read_as_text(&blob).await.expect("read_as_text");
    assert_eq!(text, "Hello, WASM!");
}

#[wasm_bindgen_test]
async fn test_read_as_text_multiline() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"line1\nline2\nline3".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let text = read_as_text(&blob).await.expect("read_as_text");
    assert_eq!(text.lines().count(), 3);
    assert!(text.starts_with("line1"));
    assert!(text.ends_with("line3"));
}

#[wasm_bindgen_test]
async fn test_read_as_text_empty() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let text = read_as_text(&blob).await.expect("read_as_text");
    assert_eq!(text, "");
}