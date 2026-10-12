//! Wasm-тесты `paste_evt`: извлечение содержимого из DataTransfer,
//! разбор `text/uri-list`, обработка файлов, поведение на пустом буфере.
//!
//! ПРАВИЛО: wasm-тесты называются с префиксом `wasm_` в имени файла.
//! ПРАВИЛО: `format!` в tail-позиции — через `format_str(format_args!(...))`
//! или явный `return`.
//! ПРАВИЛО: каждое имя базы содержит уникальный суффикс — но здесь
//! базы нет, работаем только с `DataTransfer` в памяти.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use hello_wasm02::paste_evt;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- helpers ----------------

fn make_dt(entries: &[(&str, &str)]) -> web_sys::DataTransfer {
    let dt = web_sys::DataTransfer::new().expect("new DataTransfer");
    for (mime, value) in entries {
        dt.set_data(mime, value).expect("set_data");
    }
    return dt;
}

fn make_paste_event(entries: &[(&str, &str)]) -> web_sys::Event {
    let ev = web_sys::Event::new("paste").expect("new Event");
    let dt = make_dt(entries);
    js_sys::Reflect::set(&ev, &JsValue::from_str("clipboardData"), &dt)
        .expect("set clipboardData");
    ev
}

// ---------------- parse_uri_list (чистая функция) ----------------

#[wasm_bindgen_test]
fn parse_uri_list_single() {
    let v = paste_evt::parse_uri_list("https://example.com/");
    assert_eq!(v, vec!["https://example.com/"]);
}

#[wasm_bindgen_test]
fn parse_uri_list_multiple_crlf() {
    let v = paste_evt::parse_uri_list("https://a/\r\nhttps://b/\r\nhttps://c/");
    assert_eq!(v, vec!["https://a/", "https://b/", "https://c/"]);
}

#[wasm_bindgen_test]
fn parse_uri_list_skips_comments_and_blanks() {
    let raw = "#comment\n\nhttps://a/\n#another\nhttps://b/\n";
    let v = paste_evt::parse_uri_list(raw);
    assert_eq!(v, vec!["https://a/", "https://b/"]);
}

#[wasm_bindgen_test]
fn parse_uri_list_empty() {
    let v = paste_evt::parse_uri_list("");
    assert!(v.is_empty());
}

// ---------------- extract: text/plain ----------------

#[wasm_bindgen_test]
fn extract_plain_only() {
    let dt = make_dt(&[("text/plain", "hello world")]);
    let p = paste_evt::extract_from_data_transfer(&dt);
    assert_eq!(p.plain, "hello world");
    assert!(p.html.is_none());
    assert!(p.uris.is_empty());
    assert!(p.files.is_empty());   // ← исправлено
    assert!(p.types.iter().any(|t| t == "text/plain"));
}

#[wasm_bindgen_test]
fn extract_plain_and_html() {
    let dt = make_dt(&[
        ("text/plain", "hello"),
        ("text/html", "<b>hello</b>"),
    ]);
    let p = paste_evt::extract_from_data_transfer(&dt);
    assert_eq!(p.plain, "hello");
    assert_eq!(p.html.as_deref(), Some("<b>hello</b>"));
}

#[wasm_bindgen_test]
fn extract_empty_dt() {
    let dt = web_sys::DataTransfer::new().expect("new");
    let p = paste_evt::extract_from_data_transfer(&dt);
    assert!(p.is_empty(), "пустой DataTransfer → is_empty");
    assert_eq!(p.summary(), "пусто");
}

// ---------------- extract: text/uri-list ----------------

#[wasm_bindgen_test]
fn extract_uri_list() {
    let dt = make_dt(&[(
        "text/uri-list",
        "https://a/\r\nhttps://b/",
    )]);
    let p = paste_evt::extract_from_data_transfer(&dt);
    assert_eq!(p.uris, vec!["https://a/", "https://b/"]);
}

// ---------------- extract через событие ----------------

#[wasm_bindgen_test]
fn extract_from_paste_event_works() {
    let ev = make_paste_event(&[
        ("text/plain", "from paste"),
        ("text/html", "<i>from paste</i>"),
    ]);
    let p = paste_evt::extract_from_event(&ev).expect("extract");
    assert_eq!(p.plain, "from paste");
    assert_eq!(p.html.as_deref(), Some("<i>from paste</i>"));
}

#[wasm_bindgen_test]
fn extract_from_event_without_clipboard_data_returns_none() {
    let ev = web_sys::Event::new("paste").expect("event");
    let p = paste_evt::extract_from_event(&ev);
    assert!(p.is_none(), "без clipboardData — None");
}

// ---------------- summary ----------------

#[wasm_bindgen_test]
fn summary_plain_and_html() {
    let dt = make_dt(&[
        ("text/plain", "abc"),
        ("text/html", "<p>abc</p>"),
    ]);
    let p = paste_evt::extract_from_data_transfer(&dt);
    let s = p.summary();
    assert!(s.contains("plain: 3"), "summary: {s}");
    assert!(s.contains("html:"), "summary: {s}");
}

#[wasm_bindgen_test]
fn summary_uri_only() {
    let dt = make_dt(&[("text/uri-list", "https://a/")]);
    let p = paste_evt::extract_from_data_transfer(&dt);
    let s = p.summary();
    assert!(s.contains("uri: 1"), "summary: {s}");
}

// ---------------- is_empty ----------------

#[wasm_bindgen_test]
fn is_empty_false_for_plain() {
    let dt = make_dt(&[("text/plain", "x")]);
    let p = paste_evt::extract_from_data_transfer(&dt);
    assert!(!p.is_empty());
}

#[wasm_bindgen_test]
fn is_empty_true_for_empty_html_string() {
    // Пустая строка как html не должна считаться содержимым.
    let dt = web_sys::DataTransfer::new().expect("new");
    // set_data с пустой строкой иногда не сохраняет формат — используем
    // только text/plain пустой.
    let _ = dt.set_data("text/html", "");
    let p = paste_evt::extract_from_data_transfer(&dt);
    assert!(p.is_empty());
}