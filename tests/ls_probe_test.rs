//! Wasm-тесты модуля ls_probe: доступность, CRUD, размер, детекция квоты.
//!
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use hello_wasm02::ls_probe;

wasm_bindgen_test_configure!(run_in_browser);

/// Уникальный суффикс для ключей, чтобы тесты не пересекались
/// при последовательных запусках в одном origin.
fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

// ---------- utf16_bytes: чистая функция ----------

#[wasm_bindgen_test]
fn utf16_bytes_ascii() {
    // Каждый ASCII-символ — 1 code unit × 2 байта.
    assert_eq!(ls_probe::utf16_bytes("abc"), 6);
}

#[wasm_bindgen_test]
fn utf16_bytes_empty_is_zero() {
    assert_eq!(ls_probe::utf16_bytes(""), 0);
}

#[wasm_bindgen_test]
fn utf16_bytes_cyrillic() {
    // Каждая кириллическая буква — 1 code unit × 2 байта.
    // «тест» = 4 буквы × 2 = 8.
    assert_eq!(ls_probe::utf16_bytes("тест"), 8);
}

#[wasm_bindgen_test]
fn utf16_bytes_emoji_is_surrogate_pair() {
    // U+1F600 — суррогатная пара → 2 code units → 4 байта.
    assert_eq!(ls_probe::utf16_bytes("😀"), 4);
}

// ---------- is_quota_error ----------

#[wasm_bindgen_test]
fn is_quota_error_on_plain_string_false() {
    let v = JsValue::from_str("not an error");
    assert!(!ls_probe::is_quota_error(&v));
}

#[wasm_bindgen_test]
fn is_quota_error_on_synthetic_object_true() {
    // Собираем объект { name: "QuotaExceededError" } — детектор
    // должен сработать через Reflect.
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("name"),
        &JsValue::from_str("QuotaExceededError"),
    )
    .unwrap();
    assert!(ls_probe::is_quota_error(&obj.into()));
}

#[wasm_bindgen_test]
fn is_quota_error_on_other_name_false() {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("name"),
        &JsValue::from_str("SecurityError"),
    )
    .unwrap();
    assert!(!ls_probe::is_quota_error(&obj.into()));
}

// ---------- CRUD ----------

#[wasm_bindgen_test]
fn is_available_in_normal_context() {
    assert!(ls_probe::is_available());
}

#[wasm_bindgen_test]
fn set_get_roundtrip() {
    let key = format!("probe-{}", uid());
    ls_probe::set_string(&key, "hello").unwrap();
    let v = ls_probe::get_string(&key).unwrap();
    assert_eq!(v, Some("hello".to_string()));
    ls_probe::remove(&key).unwrap();
}

#[wasm_bindgen_test]
fn get_missing_returns_none() {
    let key = format!("missing-{}", uid());
    assert!(ls_probe::get_string(&key).unwrap().is_none());
}

#[wasm_bindgen_test]
fn remove_is_idempotent() {
    // Правило проекта: удаление идемпотентно.
    // Оба вызова должны завершиться успешно.
    let key = format!("rm-{}", uid());
    ls_probe::remove(&key).unwrap();
    ls_probe::remove(&key).unwrap();
}

#[wasm_bindgen_test]
fn keys_contains_our_key() {
    let key = format!("keys-{}", uid());
    ls_probe::set_string(&key, "x").unwrap();
    let ks = ls_probe::keys().unwrap();
    assert!(ks.iter().any(|k| k == &key));
    ls_probe::remove(&key).unwrap();
}

#[wasm_bindgen_test]
fn item_count_grows_with_new_key() {
    let before = ls_probe::item_count().unwrap();
    let key = format!("cnt-{}", uid());
    ls_probe::set_string(&key, "x").unwrap();
    let after = ls_probe::item_count().unwrap();
    assert!(after >= before + 1);
    ls_probe::remove(&key).unwrap();
}

#[wasm_bindgen_test]
fn approx_bytes_used_grows_with_value() {
    let key = format!("sz-{}", uid());
    let before = ls_probe::approx_bytes_used().unwrap();
    ls_probe::set_string(&key, &"x".repeat(1000)).unwrap();
    let after = ls_probe::approx_bytes_used().unwrap();
    // 1000 ASCII-символов × 2 байта UTF-16 + ключ.
    assert!(after >= before + 2000);
    ls_probe::remove(&key).unwrap();
}

#[wasm_bindgen_test]
fn probe_reports_available() {
    let info = ls_probe::probe();
    assert!(info.available);
    // В нормальном контексте доступ есть; count и bytes — согласованы.
    assert!(info.key_count <= ls_probe::item_count().unwrap());
}