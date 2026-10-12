use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use hello_wasm02::fsa_save::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Без user gesture `save_text` вернёт `Err`. Тест проверяет,
/// что функция возвращает `Result`, а не паникует.
#[wasm_bindgen_test]
async fn test_save_text_returns_result() {
    let result = save_text("Hello, FSA!", "test-fsa.txt").await;
    match result {
        Ok(Some(name)) => {
            assert_eq!(name, "test-fsa.txt");
        }
        Ok(None) => {
            // Пользователь отменил диалог.
        }
        Err(e) => {
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(
                &format!("save_text error: {s}").into(),
            );
        }
    }
}

/// Проверяет бинарную версию сохранения.
#[wasm_bindgen_test]
async fn test_save_bytes_returns_result() {
    let bytes: Vec<u8> = (0..=255).collect();
    let result = save_bytes(
        &bytes,
        "application/octet-stream",
        "test.bin",
    )
    .await;
    match result {
        Ok(Some(name)) => {
            assert_eq!(name, "test.bin");
        }
        Ok(None) => { /* отменено */ }
        Err(e) => {
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(
                &format!("save_bytes error: {s}").into(),
            );
        }
    }
}

/// Проверяет, что `save_blob` корректно принимает Blob.
#[wasm_bindgen_test]
async fn test_save_blob_returns_result() {
    let array = js_sys::Array::of1(&JsValue::from_str("blob content"));
    let blob = web_sys::Blob::new_with_str_sequence(&array)
    .expect("Blob::new_with_str_sequence");
    let result = save_blob(&blob, "test-blob.txt").await;
    match result {
        Ok(Some(name)) => {
            assert_eq!(name, "test-blob.txt");
        }
        Ok(None) => { /* отменено */ }
        Err(e) => {
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(
                &format!("save_blob error: {s}").into(),
            );
        }
    }
}