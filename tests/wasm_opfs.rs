use wasm_bindgen_test::*;
use hello_wasm02::opfs::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_write_read_roundtrip() {
    let name = format!("test-{}.bin", js_sys::Date::now() as u64);
    let bytes: Vec<u8> = (0..=255).collect();

    write_file(&name, &bytes).await.expect("write_file");

    let read = read_file(&name).await.expect("read_file");
    assert_eq!(read, bytes);

    // Проверяем размер.
    let size = file_size(&name).await.expect("file_size");
    assert_eq!(size as usize, bytes.len());

    // Убираем.
    let _ = remove_file(&name).await;
}

#[wasm_bindgen_test]
async fn test_read_missing_fails() {
    let result = read_file("no-such-file.bin").await;
    assert!(result.is_err(), "ожидалась ошибка для отсутствующего файла");
}

#[wasm_bindgen_test]
async fn test_estimate_returns_pair() {
    let (usage, quota) = estimate().await.expect("estimate");
    assert!(quota > 0.0, "квота должна быть положительной");
    assert!(usage >= 0.0, "usage не может быть отрицательным");
}