//! Host-тесты модуля sw_core: конфигурация и модель статуса.
//! Никаких JsValue/Service Worker здесь быть не должно.

use hello_wasm02::sw_core::SW_PATH;
use hello_wasm02::sw_offline::{ASSETS, CACHE_APP_SHELL, CACHE_RUNTIME};

#[test]
fn sw_path_is_stable() {
    assert_eq!(SW_PATH, "/sw.js");
}

#[test]
fn cache_names_are_stable() {
    assert_eq!(CACHE_APP_SHELL, "app-shell-v1");
    assert_eq!(CACHE_RUNTIME, "runtime-v1");
}

#[test]
fn assets_manifest_not_empty() {
    assert!(!ASSETS.is_empty());
    assert!(ASSETS.iter().any(|a| a.ends_with(".wasm")));
    assert!(ASSETS.iter().any(|a| a.ends_with(".js")));
}