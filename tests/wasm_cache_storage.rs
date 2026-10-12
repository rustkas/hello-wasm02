use wasm_bindgen_test::*;
use hello_wasm02::cache_storage::*;

wasm_bindgen_test_configure!(run_in_browser);

const CACHE_NAME: &str = "wasm-test-cache";

/// Ключи в Cache API — это валидные URL. Используем пути текущего
/// origin: `/test-key-1` автоматически превращается в
/// `http://127.0.0.1:PORT/test-key-1`.
const KEY_1: &str = "/test-key-1";
const KEY_MISSING: &str = "/no-such-key";

#[wasm_bindgen_test]
async fn test_put_get_roundtrip() {
    let parts = js_sys::Array::of1(&"hello".into());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/plain");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(
        &parts, &options,
    )
    .expect("Blob::new_with_str_sequence_and_options");

    put_blob(CACHE_NAME, KEY_1, &blob)
        .await
        .expect("put_blob");

    let read = get_blob(CACHE_NAME, KEY_1)
        .await
        .expect("get_blob");
    assert!(read.is_some(), "ожидался Blob");

    // Убираем за собой.
    let _ = delete_cache(CACHE_NAME).await;
}

#[wasm_bindgen_test]
async fn test_get_missing_returns_none() {
    let result = get_blob(CACHE_NAME, KEY_MISSING)
        .await
        .expect("get_blob");
    assert!(result.is_none(), "не должно быть записи");
}

#[wasm_bindgen_test]
async fn test_delete_idempotent() {
    // Удаляем несуществующее — не ошибка.
    let first = delete_blob(CACHE_NAME, KEY_MISSING)
        .await
        .expect("delete");
    assert!(!first, "первое удаление несуществующего — false");

    let second = delete_blob(CACHE_NAME, KEY_MISSING)
        .await
        .expect("delete");
    assert!(!second, "повторное удаление — тоже false");
}

#[wasm_bindgen_test]
async fn test_list_keys_returns_vec() {
    let keys = list_keys(CACHE_NAME).await.expect("list_keys");
    for k in &keys {
        assert!(!k.is_empty(), "ключ не пустой");
    }
}