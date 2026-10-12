//! Тесты обёрток ввода-вывода: `src/converter_ui.rs`.
//! Всё, что требует user gesture, помечается `#[ignore]` или мягко
//! проверяется на «не паникует».

use hello_wasm02::{converter, converter_ui};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn forget_save_handle_idempotent() {
    converter_ui::forget_save_handle();
    converter_ui::forget_save_handle();
    converter_ui::forget_save_handle();
}

#[wasm_bindgen_test]
fn upload_id_url_safe_via_converter() {
    // Косвенная проверка: id, который попадёт в upload_to_server,
    // не должен содержать символов вне [A-Za-z0-9_-].
    for _ in 0..8 {
        let id = converter::generate_upload_id();
        for ch in id.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
                "unexpected char in upload_id: {:?}",
                ch
            );
        }
    }
}

/// Проверяем, что FSA-путь без user gesture не паникует: либо Err,
/// либо fallback (`<a download>`) успешно срабатывает.
#[wasm_bindgen_test]
async fn save_file_chunked_without_gesture_is_safe() {
    let bytes = vec![0u8; 1024];
    let res = converter_ui::save_file_chunked(
        &bytes,
        "test.grayscale.png",
        |_| {},
        || false,
    )
    .await;

    match res {
        Ok(_) => { /* fallback <a download> */ }
        Err(e) => {
            let name = js_sys::Reflect::get(&e, &"name".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            assert!(
                name.is_empty()
                    || name == "SecurityError"
                    || name == "AbortError"
                    || name == "NotAllowedError",
                "unexpected error name: {}",
                name
            );
        }
    }
}

/// Отмена save при первом же вызове is_cancelled: должен вернуться
/// `SaveOutcome::Cancelled` или fallback-результат (в зависимости
/// от того, есть ли FSA в браузере).
#[wasm_bindgen_test]
async fn save_file_chunked_cancel_immediately_is_safe() {
    let bytes = vec![0u8; 64 * 1024];
    let res = converter_ui::save_file_chunked(
        &bytes,
        "cancel.png",
        |_| {},
        || true,
    )
    .await;

    match res {
        Ok(converter_ui::SaveOutcome::Cancelled) => {}
        Ok(converter_ui::SaveOutcome::FallbackDownload) => {}
        Ok(converter_ui::SaveOutcome::Fsa) => {
            panic!("FSA path should not complete when cancelled immediately");
        }
        Err(_) => { /* SecurityError без жеста — допустимо */ }
    }
}

/// Диалог выбора файла в headless недоступен. Помечаем `#[ignore]`.
#[wasm_bindgen_test]
#[ignore]
async fn pick_image_file_requires_gesture() {
    let res = converter_ui::pick_image_file().await;
    assert!(res.is_err());
}

/// `upload_to_server` без запущенного auth-server должен вернуть
/// ошибку сети, не паникуя. Интеграционный smoke-тест.
#[wasm_bindgen_test]
async fn upload_to_dead_server_returns_err() {
    let bytes = vec![0u8; 32];
    let res = converter_ui::upload_to_server(
        &bytes,
        "test-upload-id",
        "x.png",
        "image/png",
        "http://127.0.0.1:1", // заведомо мёртвый порт
        |_| {},
    )
    .await;
    assert!(res.is_err(), "upload to dead server must err");
}

/// `delete_upload` к мёртвому серверу — тоже Err, без паники.
#[wasm_bindgen_test]
async fn delete_from_dead_server_returns_err() {
    let res = converter_ui::delete_upload("http://127.0.0.1:1", "any-id").await;
    assert!(res.is_err(), "delete from dead server must err");
}