use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use hello_wasm02::fsa_open::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Проверяет, что `is_supported()` возвращает `bool` без паники.
///
/// В Chrome/Edge — `true`, в Firefox/Safari — `false`. Это
/// единственный тест, который не требует user gesture и работает
/// в любом браузере.
#[wasm_bindgen_test]
fn test_is_supported_returns_bool() {
    let supported = is_supported();
    // Логируем результат — полезно при отладке в браузере.
    web_sys::console::log_1(
        &format!("FSA supported: {supported}").into(),
    );
    // Тест всегда проходит — фиксируем, что функция не паникует.
}

/// Без user gesture вызов `pick_one_file` вернёт `Err` с
/// `SecurityError`. Тест проверяет, что функция возвращает
/// `Result`, а не паникует.
#[wasm_bindgen_test]
async fn test_pick_one_file_returns_result() {
    let result = pick_one_file().await;
    match result {
        Ok(Some(file)) => {
            // Если браузер разрешил вызов — проверяем File.
            assert!(!file.name().is_empty(), "имя файла пустое");
        }
        Ok(None) => {
            // Пользователь отменил диалог.
        }
        Err(e) => {
            // Ожидаемо в headless-режиме: SecurityError.
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(
                &format!("pick_one_file error: {s}").into(),
            );
        }
    }
}

/// Проверяет, что `pick_one_file_with_filter` не паникует
/// при отсутствии user gesture.
#[wasm_bindgen_test]
async fn test_pick_one_file_with_filter_returns_result() {
    let accept = js_sys::Array::of1(&JsValue::from_str(".txt"));
    let result = pick_one_file_with_filter(&accept.into()).await;
    match result {
        Ok(_) => { /* допустимо */ }
        Err(e) => {
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(
                &format!("pick_one_file_with_filter error: {s}").into(),
            );
        }
    }
}