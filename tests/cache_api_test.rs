//! Host-тесты модуля cache_api: константы и сериализация моделей.
//! Никаких JsValue/Cache здесь быть не должно — только чистая Rust-логика.

use hello_wasm02::cache_api::CACHE_NAME;
use hello_wasm02::cache_offline::Strategy;

#[test]
fn cache_name_is_stable() {
    assert_eq!(CACHE_NAME, "hello-wasm-offline-v1");
}

#[test]
fn strategy_enum_roundtrip() {
    let strategies = [
        Strategy::CacheFirst,
        Strategy::NetworkFirst,
        Strategy::StaleWhileRevalidate,
    ];
    for s in strategies {
        assert_eq!(s, s.clone());
    }
}

#[test]
fn strategy_variants_are_distinct() {
    assert_ne!(Strategy::CacheFirst, Strategy::NetworkFirst);
    assert_ne!(Strategy::NetworkFirst, Strategy::StaleWhileRevalidate);
    assert_ne!(Strategy::CacheFirst, Strategy::StaleWhileRevalidate);
}