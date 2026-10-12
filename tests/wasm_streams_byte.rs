//! Тесты для секции 17.6: Byte streams и BYOB.

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Byte stream можно читать default reader'ом.
#[wasm_bindgen_test]
async fn test_byte_stream_default_reader() {
    let stream = hello_wasm02::streams::readable_bytes_with_byob_support(vec![
        b"hello".to_vec(),
        b" world".to_vec(),
    ])
    .unwrap();

    let result = hello_wasm02::streams::read_all(&stream).await.unwrap();

    assert_eq!(String::from_utf8_lossy(&result), "hello world");
}

/// Byte stream корректно читается BYOB-читателем.
#[wasm_bindgen_test]
async fn test_byte_stream_byob_reader() {
    let stream = hello_wasm02::streams::readable_bytes_with_byob_support(vec![
        b"hello".to_vec(),
        b" world".to_vec(),
    ])
    .unwrap();

    let result = hello_wasm02::streams::read_byob(&stream, 1024)
        .await
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&result), "hello world");
}

/// BYOB с маленьким буфером: чанк больше буфера — читается частями.
#[wasm_bindgen_test]
async fn test_byob_partial_fill() {
    // Один чанк 10 байт, буфер 4 байта — читается как 4+4+2.
    let stream = hello_wasm02::streams::readable_bytes_with_byob_support(vec![
        b"0123456789".to_vec(),
    ])
    .unwrap();

    let result = hello_wasm02::streams::read_byob(&stream, 4)
        .await
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&result), "0123456789");
}

/// BYOB корректно сигналит EOF на пустом потоке.
#[wasm_bindgen_test]
async fn test_byob_eof() {
    let stream = hello_wasm02::streams::readable_bytes_with_byob_support(vec![]).unwrap();

    let result = hello_wasm02::streams::read_byob(&stream, 1024)
        .await
        .unwrap();

    assert!(result.is_empty(), "пустой поток должен вернуть пустой Vec");
}