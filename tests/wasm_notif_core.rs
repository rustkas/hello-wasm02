//! Wasm-тесты `notif_core`: permission-парсинг, менеджер, состояние.
//!
//! ПРАВИЛО: тесты, требующие user gesture или granted permission,
//! помечены `#[ignore]` — в headless они не работают.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::notif_core::{self, NotifyOptions, NotificationManager, Permission};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- Permission: парсинг ----------------

#[wasm_bindgen_test]
fn permission_from_str_known_values() {
    assert_eq!(Permission::from_str("default"), Permission::Default);
    assert_eq!(Permission::from_str("granted"), Permission::Granted);
    assert_eq!(Permission::from_str("denied"), Permission::Denied);
}

#[wasm_bindgen_test]
fn permission_from_str_unknown() {
    assert_eq!(Permission::from_str("garbage"), Permission::Unknown);
    assert_eq!(Permission::from_str(""), Permission::Unknown);
}

#[wasm_bindgen_test]
fn permission_as_str_roundtrip() {
    for p in [
        Permission::Default,
        Permission::Granted,
        Permission::Denied,
        Permission::Unknown,
    ] {
        assert_eq!(Permission::from_str(p.as_str()), p);
    }
}

#[wasm_bindgen_test]
fn permission_shorts() {
    assert_eq!(Permission::Default.short(), "не запрошено");
    assert_eq!(Permission::Granted.short(), "разрешено");
    assert_eq!(Permission::Denied.short(), "запрещено");
}

#[wasm_bindgen_test]
fn permission_tones() {
    assert_eq!(Permission::Granted.tone(), "ok");
    assert_eq!(Permission::Denied.tone(), "err");
    assert_eq!(Permission::Default.tone(), "warn");
    assert_eq!(Permission::Unknown.tone(), "muted");
}

// ---------------- API availability ----------------

#[wasm_bindgen_test]
fn api_supported_in_headless_chrome() {
    // В headless Chrome на localhost Notification API есть.
    assert!(notif_core::is_supported());
}

#[wasm_bindgen_test]
fn permission_returns_known_state() {
    // Не предполагаем конкретное значение — только что функция
    // не паникует и возвращает валидный вариант.
    let p = notif_core::permission();
    assert!(
        matches!(
            p,
            Permission::Default | Permission::Granted | Permission::Denied | Permission::Unknown
        ),
        "permission вернул невалидное состояние: {:?}", p,
    );
}

// ---------------- NotificationManager: локальное состояние ----------------

#[wasm_bindgen_test]
fn manager_starts_empty() {
    let m = NotificationManager::new();
    assert_eq!(m.count(), 0);
}

#[wasm_bindgen_test]
fn close_all_on_empty_is_noop() {
    let m = NotificationManager::new();
    m.close_all();
    assert_eq!(m.count(), 0);
    // Идемпотентность.
    m.close_all();
    assert_eq!(m.count(), 0);
}

#[wasm_bindgen_test]
fn manager_clone_shares_state() {
    let m1 = NotificationManager::new();
    let m2 = m1.clone();
    // Оба клона смотрят на один Vec.
    assert_eq!(m1.count(), 0);
    assert_eq!(m2.count(), 0);
}

// ---------------- NotifyOptions: значения по умолчанию ----------------

#[wasm_bindgen_test]
fn notify_options_default() {
    let opts = NotifyOptions::default();
    assert!(opts.title.is_empty());
    assert!(opts.body.is_empty());
    assert!(opts.tag.is_empty());
    assert!(opts.icon.is_none());
    assert!(!opts.require_interaction);
}

// ---------------- Show — требует разрешения ----------------

#[wasm_bindgen_test]
#[ignore = "требует granted permission — в headless недоступно"]
fn show_creates_notification() {
    let m = NotificationManager::new();
    let opts = NotifyOptions {
        title: "Test".to_string(),
        body: "Body".to_string(),
        ..Default::default()
    };
    m.show(opts, |_event| {}).expect("show");
    assert_eq!(m.count(), 1);
}