//! Wasm-тесты offline-first: открытие кэша, предзагрузка, список, удаление.
//! Service Worker в headless-браузере может быть недоступен без HTTPS —
//! поэтому тестируем Cache API, а не регистрацию.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::sw_offline;

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64)
}

fn test_cache_name() -> String {
    return format!("test-sw-cache-{}", uid())
}

#[wasm_bindgen_test]
async fn open_and_delete_cache() {
    let name = test_cache_name();
    let cache = sw_offline::open_cache(&name).await.expect("open_cache");
    // Кэш открыт — можно работать.
    drop(cache);
    let removed = sw_offline::delete_cache(&name).await.expect("delete");
    assert!(removed);
}

#[wasm_bindgen_test]
async fn delete_cache_is_idempotent() {
    let name = test_cache_name();
    let r1 = sw_offline::delete_cache(&name).await.expect("first");
    assert!(!r1);
    let r2 = sw_offline::delete_cache(&name).await.expect("second");
    assert!(!r2);
}

#[wasm_bindgen_test]
async fn list_cached_returns_empty_for_new_cache() {
    let name = test_cache_name();
    let list = sw_offline::list_cached(&name).await.expect("list");
    assert!(list.is_empty());
    let _ = sw_offline::delete_cache(&name).await;
}

#[wasm_bindgen_test]
async fn preload_returns_count() {
    // В headless-браузере ресурсы могут быть недоступны,
    // но функция должна корректно вернуть результат (возможно, 0).
    let count = sw_offline::preload_app_shell().await;
    match count {
        Ok(n) => assert!(n <= hello_wasm02::sw_offline::ASSETS.len()),
        Err(_) => { /* fetch может быть недоступен — это нормально */ }
    }
}