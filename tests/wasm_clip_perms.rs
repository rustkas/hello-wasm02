//! Wasm-тесты `clip_perms`: PermState, доступность, query.
//!
//! ПРАВИЛО: Permissions API может быть недоступен в конкретном браузере.
//! Тесты не предполагают конкретного ответа `query` — только что
//! функция не паникует и возвращает валидный `PermState`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::clip_perms::{self, PermState};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- PermState ----------------

#[wasm_bindgen_test]
fn perm_state_from_str() {
    assert_eq!(PermState::from_str("granted"), PermState::Granted);
    assert_eq!(PermState::from_str("prompt"), PermState::Prompt);
    assert_eq!(PermState::from_str("denied"), PermState::Denied);
    assert_eq!(PermState::from_str("garbage"), PermState::Unknown);
}

#[wasm_bindgen_test]
fn perm_state_as_str_roundtrip() {
    for s in [PermState::Granted, PermState::Prompt, PermState::Denied, PermState::Unknown] {
        assert_eq!(PermState::from_str(s.as_str()), s);
    }
}

#[wasm_bindgen_test]
fn perm_state_shorts() {
    assert_eq!(PermState::Granted.short(), "разрешено");
    assert_eq!(PermState::Prompt.short(), "будет запрошено");
    assert_eq!(PermState::Denied.short(), "запрещено");
}

// ---------------- доступность ----------------

#[wasm_bindgen_test]
fn perms_api_is_available() {
    // В headless Chrome Permissions API есть.
    assert!(clip_perms::is_available());
}

// ---------------- query ----------------

#[wasm_bindgen_test]
async fn query_clipboard_write_returns_state() {
    let state = clip_perms::query("clipboard-write").await.unwrap();
    // Конкретное значение зависит от окружения — проверяем,
    // что это один из валидных вариантов.
    assert!(
        matches!(
            state,
            PermState::Granted | PermState::Prompt | PermState::Denied | PermState::Unknown
        ),
        "query вернул невалидное состояние: {:?}",
        state,
    );
}

#[wasm_bindgen_test]
async fn query_both_returns_tuple() {
    let (read, write) = clip_perms::query_both().await;
    // Просто проверяем, что обе величины валидны.
    let _ = read.as_str();
    let _ = write.as_str();
}

#[wasm_bindgen_test]
async fn probe_returns_info() {
    let info = clip_perms::probe().await;
    assert!(info.api_available, "clipboard API должен быть доступен");
    // Permissions API доступен в headless Chrome.
    assert!(info.perms_api_available);
}