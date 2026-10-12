//! Wasm-тесты практического проекта Главы 25 (Cache API).
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::cache_api;
use hello_wasm02::cache_offline::{self, Strategy};

wasm_bindgen_test_configure!(run_in_browser);

/// Уникальный суффикс, чтобы тесты не сталкивались в одном кэше
/// при последовательных запусках.
fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64)
}

fn test_cache_name() -> String {
    return format!("test-cache-{}", uid())
}

#[wasm_bindgen_test]
async fn open_and_delete_cache() {
    let name = test_cache_name();
    let cache = cache_api::open_cache(&name).await.expect("open_cache");
    // Кэш открыт — можно положить запись.
    cache_offline::put_text(&cache, "/hello", "world", "text/plain")
        .await
        .expect("put_text");
    let keys = cache_api::keys(&cache).await.expect("keys");
    assert!(keys.iter().any(|k| k.ends_with("/hello")));

    // Удаляем целиком.
    let removed = cache_api::delete_cache(&name).await.expect("delete_cache");
    assert!(removed);
}

#[wasm_bindgen_test]
async fn put_and_get_roundtrip() {
    let name = test_cache_name();
    let cache = cache_api::open_cache(&name).await.expect("open_cache");

    cache_offline::put_text(&cache, "/data", "{\"k\":1}", "application/json")
        .await
        .expect("put_text");

    let response = cache_api::get(&cache, "/data").await.expect("get");
    assert!(response.is_some());
    let resp = response.unwrap();
    let text = wasm_bindgen_futures::JsFuture::from(resp.text().unwrap())
        .await
        .unwrap()
        .as_string()
        .unwrap();
    assert_eq!(text, "{\"k\":1}");

    let _ = cache_api::delete_cache(&name).await;
}

#[wasm_bindgen_test]
async fn get_missing_returns_none() {
    let name = test_cache_name();
    let cache = cache_api::open_cache(&name).await.expect("open_cache");
    let result = cache_api::get(&cache, "/no-such").await.expect("get");
    assert!(result.is_none(), "не должно быть записи");
    let _ = cache_api::delete_cache(&name).await;
}

#[wasm_bindgen_test]
async fn remove_is_idempotent() {
    let name = test_cache_name();
    let cache = cache_api::open_cache(&name).await.expect("open_cache");
    cache_offline::put_text(&cache, "/del", "x", "text/plain").await.unwrap();

    let first = cache_api::remove(&cache, "/del").await.expect("first");
    assert!(first);
    let second = cache_api::remove(&cache, "/del").await.expect("second");
    assert!(!second); // идемпотентность: второй раз нечего удалять

    let _ = cache_api::delete_cache(&name).await;
}

#[wasm_bindgen_test]
async fn cache_first_returns_cached() {
    let name = test_cache_name();
    let cache = cache_api::open_cache(&name).await.expect("open_cache");
    cache_offline::put_text(&cache, "/cached", "from cache", "text/plain").await.unwrap();

    let response = cache_offline::fetch_with_strategy("/cached", &cache, Strategy::CacheFirst)
        .await
        .expect("cache_first");
    let text = wasm_bindgen_futures::JsFuture::from(response.text().unwrap())
        .await
        .unwrap()
        .as_string()
        .unwrap();
    assert_eq!(text, "from cache");

    let _ = cache_api::delete_cache(&name).await;
}

#[wasm_bindgen_test]
async fn network_first_falls_back_to_cache() {
    let name = test_cache_name();
    let cache = cache_api::open_cache(&name).await.expect("open_cache");
    // Кладём в кэш, но URL не существует в сети.
    cache_offline::put_text(&cache, "https://invalid.invalid/x", "offline", "text/plain")
        .await
        .unwrap();

    let response = cache_offline::fetch_with_strategy(
        "https://invalid.invalid/x",
        &cache,
        Strategy::NetworkFirst,
    )
    .await
    .expect("network_first fallback");
    let text = wasm_bindgen_futures::JsFuture::from(response.text().unwrap())
        .await
        .unwrap()
        .as_string()
        .unwrap();
    assert_eq!(text, "offline");

    let _ = cache_api::delete_cache(&name).await;
}

#[wasm_bindgen_test]
async fn delete_cache_is_idempotent() {
    let name = test_cache_name();
    // Кэша ещё нет — delete не должен падать.
    let r1 = cache_api::delete_cache(&name).await.expect("first");
    assert!(!r1);
    let r2 = cache_api::delete_cache(&name).await.expect("second");
    assert!(!r2);
}