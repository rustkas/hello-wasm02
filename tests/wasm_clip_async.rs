//! Wasm-тесты `clip_async`: Blob для текста, ClipboardItem из
//! Promise<Blob>, поддержка MIME, поведение с задержкой.
//!
//! ПРАВИЛО: wasm-тесты называются с префиксом `wasm_` в имени файла.
//! ПРАВИЛО: `format!` в tail-позиции — через явный `return`.
//! ПРАВИЛО: headless-тесты не могут эмулировать user gesture —
//! операции записи в буфер помечены `#[ignore]`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

use hello_wasm02::clip_async;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- доступность ----------------

#[wasm_bindgen_test]
fn api_available_in_headless() {
    assert!(
        clip_async::is_available(),
        "navigator.clipboard должен быть доступен в headless Chrome на localhost",
    );
}

#[wasm_bindgen_test]
fn supports_text_plain() {
    assert!(
        clip_async::supports_mime("text/plain"),
        "text/plain должен поддерживаться",
    );
}

#[wasm_bindgen_test]
fn supports_text_html() {
    assert!(
        clip_async::supports_mime("text/html"),
        "text/html должен поддерживаться",
    );
}

// ---------------- text blob ----------------

#[wasm_bindgen_test]
fn text_blob_has_correct_mime() {
    let blob = clip_async::make_text_blob("hello", "text/plain").expect("blob");
    assert_eq!(blob.type_(), "text/plain");
    assert_eq!(blob.size() as u32, 5);
}

#[wasm_bindgen_test]
fn text_blob_empty_string() {
    let blob = clip_async::make_text_blob("", "text/plain").expect("blob");
    assert_eq!(blob.size() as u32, 0);
}

// ---------------- make_item ----------------

#[wasm_bindgen_test]
fn make_item_from_blob_returns_object() {
    let blob = clip_async::make_text_blob("hi", "text/plain").expect("blob");
    let item = clip_async::make_item(&[("text/plain".to_string(), blob.into())])
        .expect("item");
    assert!(item.is_object(), "ClipboardItem должен быть объектом");

    // У ClipboardItem есть поле `types` — массив MIME.
    let types = js_sys::Reflect::get(&item, &JsValue::from_str("types"))
        .expect("types");
    let arr = types.dyn_into::<js_sys::Array>().expect("types — Array");
    assert_eq!(arr.length(), 1);
    assert_eq!(arr.get(0).as_string().as_deref(), Some("text/plain"));
}

#[wasm_bindgen_test]
fn make_item_multi_format_single_item() {
    let text_blob = clip_async::make_text_blob("hi", "text/plain").expect("text blob");
    let html_blob = clip_async::make_text_blob("<b>hi</b>", "text/html").expect("html blob");
    let item = clip_async::make_item(&[
        ("text/plain".to_string(), text_blob.into()),
        ("text/html".to_string(), html_blob.into()),
    ])
    .expect("item");

    let types = js_sys::Reflect::get(&item, &JsValue::from_str("types"))
        .expect("types");
    let arr = types.dyn_into::<js_sys::Array>().expect("types — Array");
    assert_eq!(arr.length(), 2, "item должен содержать 2 MIME-типа");
}

// ---------------- png promise ----------------

#[wasm_bindgen_test]
async fn png_promise_resolves_with_blob() {
    // Быстрая задержка — тест не должен ждать секунды.
    let promise = clip_async::make_png_promise(64, 50, |_sz| {})
        .expect("png promise");

    let blob_val = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("await png promise");
    let blob = blob_val.dyn_into::<web_sys::Blob>().expect("Blob");
    assert_eq!(blob.type_(), "image/png");
    assert!(blob.size() > 0.0, "PNG должен быть непустым");
}

#[wasm_bindgen_test]
async fn png_promise_callback_reports_size() {
    // Проверяем, что on_ready получает размер.
    let observed: std::rc::Rc<std::cell::RefCell<Option<u64>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let obs = observed.clone();

    let promise = clip_async::make_png_promise(64, 50, move |sz| {
        *obs.borrow_mut() = Some(sz);
    })
    .expect("png promise");

    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("await");

    assert!(
        observed.borrow().is_some(),
        "on_ready должен был получить размер",
    );
    assert!(
        observed.borrow().unwrap() > 0,
        "размер должен быть больше нуля",
    );
}

// ---------------- запись в буфер (требует разрешения) ----------------

#[wasm_bindgen_test]
#[ignore = "требует clipboard-write permission в headless"]
async fn write_single_text_item_works() {
    let blob = clip_async::make_text_blob("test write", "text/plain").expect("blob");
    let item = clip_async::make_item(&[("text/plain".to_string(), blob.into())])
        .expect("item");
    clip_async::write_items(&[item])
        .await
        .expect("write_items");
}

#[wasm_bindgen_test]
#[ignore = "требует clipboard-write permission в headless"]
async fn write_two_items_works() {
    let text_blob = clip_async::make_text_blob("test", "text/plain").expect("text");
    let text_item = clip_async::make_item(&[("text/plain".to_string(), text_blob.into())])
        .expect("text item");
    let png_promise = clip_async::make_png_promise(64, 50, |_| {}).expect("png promise");
    let png_item = clip_async::make_item(&[("image/png".to_string(), png_promise.into())])
        .expect("png item");
    clip_async::write_items(&[text_item, png_item])
        .await
        .expect("write_items");
}