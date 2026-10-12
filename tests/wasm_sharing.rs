use wasm_bindgen_test::*;
use hello_wasm02::sharing::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Проверяет, что `is_supported()` возвращает `bool` без паники.
///
/// На мобильных браузерах — `true`, на десктопном Firefox — `false`.
/// Тест всегда проходит — фиксируем, что функция не падает.
#[wasm_bindgen_test]
fn test_is_supported_returns_bool() {
    let supported = is_supported();
    web_sys::console::log_1(
        &format!("Web Share supported: {supported}").into(),
    );
}

/// Проверяет, что `can_share_files` возвращает `bool` без паники.
///
/// На пустом массиве — `false` (нечего шарить).
/// На одном файле — зависит от браузера.
#[wasm_bindgen_test]
fn test_can_share_empty_returns_false() {
    let result = can_share_files(&[]);
    assert!(!result, "пустой список файлов не может быть расшарен");
}

/// Проверяет, что `can_share_files` не паникует на валидном файле.
#[wasm_bindgen_test]
fn test_can_share_file_returns_bool() {
    let parts = js_sys::Array::of1(&"test content".into());
    let file = web_sys::File::new_with_str_sequence(&parts, "test.txt")
        .expect("File::new_with_str_sequence");
    let result = can_share_files(&[file]);
    // Результат зависит от браузера — главное, не паникует.
    web_sys::console::log_1(
    &format!("canShare(test.txt): {result}").into(),
);
}

/// Без user gesture вызов `share_files` вернёт `Err` с
/// `SecurityError` или `NotAllowedError`. Тест проверяет, что
/// функция возвращает `Result`, а не паникует.
#[wasm_bindgen_test]
async fn test_share_files_returns_result() {
    let parts = js_sys::Array::of1(&"test content".into());
    let file = web_sys::File::new_with_str_sequence(&parts, "test.txt")
        .expect("File::new_with_str_sequence");

    let result = share_files(&[file], "Test", "Sharing test").await;
    match result {
        Ok(()) => {
            // Если браузер разрешил — пользователь поделился.
        }
        Err(e) => {
            // Ожидаемо в headless и без gesture: SecurityError.
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(
                &format!("share_files error: {s}").into(),
            );
        }
    }
}

/// Проверяет, что `is_abort_error` распознаёт отмену.
#[wasm_bindgen_test]
fn test_is_abort_error_recognizes_abort() {
    // Симулируем ошибку отмены через строку.
    let e = wasm_bindgen::JsValue::from_str("AbortError");
    assert!(is_abort_error(&e), "должно распознать AbortError");
}

#[wasm_bindgen_test]
fn test_is_abort_error_rejects_other() {
    let e = wasm_bindgen::JsValue::from_str("Some other error");
    assert!(!is_abort_error(&e), "не должно распознавать обычную ошибку");
}