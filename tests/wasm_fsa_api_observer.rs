//! Тесты `src/fsa_api_observer.rs`.
//!
//! **Что можно проверить в headless Chrome.**
//! * `is_supported()` — feature detection (возвращает bool).
//! * Создание observer'а — не падает, если API есть.
//! * `disconnect()` идемпотентен — два вызова подряд — ОК.
//!
//! **Что проверить нельзя.**
//! * Реальное срабатывание observer'а на изменения. В headless
//!   Chrome нет внешнего источника изменений (пользователь не
//!   редактирует файлы из проводника ОС во время теста).
//!   Полная проверка — только вручную через демо.
//!
//! Все тесты мягко пропускаются, если `FileSystemObserver`
//! не поддерживается браузером.

use wasm_bindgen_test::*;

use hello_wasm02::fsa_api_observer::{is_supported, observe_dir, ChangeKind};
use hello_wasm02::fsa_api_opfs::opfs_root;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Feature detection
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_is_supported_returns_bool() {
    // Не паникует в любом окружении. Если API нет — false.
    let ok = is_supported();
    let msg: String = format!("FileSystemObserver supported: {ok}");
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&msg));
}

// ---------------------------------------------------------------------------
// Создание observer'а
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_observe_dir_creates_observer() {
    if !is_supported() {
        return; // мягкий skip: браузер не поддерживает
    }

    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return, // мягкий skip: OPFS недоступен
    };

    // Создаём observer на корне OPFS.
    let handle = observe_dir(&root, |_records| {}).expect("observe_dir");
    // Сразу отписываемся — в тесте не нужен постоянный наблюдатель.
    handle.disconnect();
}

#[wasm_bindgen_test]
async fn test_disconnect_is_idempotent() {
    if !is_supported() {
        return;
    }

    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };

    let handle = observe_dir(&root, |_| {}).expect("observe_dir");

    // Три disconnect() подряд — все Ok (правило проекта).
    handle.disconnect();
    handle.disconnect();
    handle.disconnect();
}

// ---------------------------------------------------------------------------
// Доменная модель
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_change_kind_from_str() {
    assert_eq!(ChangeKind::from_str("appeared"), ChangeKind::Appeared);
    assert_eq!(ChangeKind::from_str("disappeared"), ChangeKind::Disappeared);
    assert_eq!(ChangeKind::from_str("modified"), ChangeKind::Modified);
    assert_eq!(ChangeKind::from_str("moved"), ChangeKind::Moved);
    // Неизвестное значение → Unknown, не паника.
    assert_eq!(ChangeKind::from_str("garbage"), ChangeKind::Unknown);
    assert_eq!(ChangeKind::from_str(""), ChangeKind::Unknown);
}

#[wasm_bindgen_test]
fn test_change_kind_labels() {
    // Проверяем, что ярлыки непустые и не паникуют.
    assert!(!ChangeKind::Appeared.label_ru().is_empty());
    assert!(!ChangeKind::Disappeared.label_ru().is_empty());
    assert!(!ChangeKind::Modified.label_ru().is_empty());
    assert!(!ChangeKind::Moved.label_ru().is_empty());
    assert!(!ChangeKind::Unknown.label_ru().is_empty());
}