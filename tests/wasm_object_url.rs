use wasm_bindgen_test::*;
use hello_wasm02::object_url::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_create_object_url_prefix() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"hello".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let url = create_object_url(&blob).expect("create_object_url");
    assert!(url.starts_with("blob:"), "URL должен начинаться с 'blob:', получено: {url}");

    // Освобождаем, чтобы не текла память в тестовом окружении.
    revoke_object_url(&url).ok();
}

#[wasm_bindgen_test]
fn test_create_and_revoke() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"data".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let url = create_object_url(&blob).expect("create_object_url");
    assert!(!url.is_empty());

    // Освобождение не должно возвращать ошибку.
    revoke_object_url(&url).expect("revoke_object_url");
}

#[wasm_bindgen_test]
fn test_two_urls_differ() {
    // Один и тот же Blob, два вызова create_object_url —
    // должны получиться разные URL (каждый вызов создаёт новый).
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"same".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let url1 = create_object_url(&blob).expect("create #1");
    let url2 = create_object_url(&blob).expect("create #2");

    assert_ne!(url1, url2, "Два вызова create_object_url должны давать разные URL");

    revoke_object_url(&url1).ok();
    revoke_object_url(&url2).ok();
}