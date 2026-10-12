//! 49. Тесты File Converter.
//!
//! ## Что тестируется
//!
//! - `detect_format` — по расширению и содержимому.
//! - `choose_target` — правильная цель по умолчанию.
//! - JSON ↔ CSV: round-trip, экранирование, ошибки.
//! - Markdown → HTML: заголовки, inline-разметка, эскейпинг.
//! - TXT → JSON stats.
//! - `preview_output`, `replace_extension`, `format_size`.
//!
//! ## Что НЕ тестируется
//!
//! `read_file_chunked` требует реального `File` — headless-браузер
//! может его создать через `new File([...], name)`, но полноценная
//! демонстрация chunked-чтения видна в app.rs на большом файле.
//!
//! ## Отличие от `wasm_converter.rs`
//!
//! `wasm_converter.rs` тестирует image-пайплайн (`src/converter.rs`).
//! Здесь — текстовые конвертации, отдельный модуль.

use wasm_bindgen_test::*;

use hello_wasm02::file_convert::{
    choose_target, csv_to_json, detect_format, json_to_csv, md_to_html,
    preview_output, text_stats, text_to_json, Format,
};
use hello_wasm02::file_io::{format_size, replace_extension};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// detect_format
// =============================================================================

#[wasm_bindgen_test]
fn test_detect_by_extension() {
    assert_eq!(detect_format("data.json", b""), Format::Json);
    assert_eq!(detect_format("rows.csv", b""), Format::Csv);
    assert_eq!(detect_format("README.md", b""), Format::Markdown);
    assert_eq!(detect_format("page.html", b""), Format::Html);
    assert_eq!(detect_format("notes.txt", b""), Format::Text);
    assert_eq!(detect_format("DATA.JSON", b""), Format::Json);
}

#[wasm_bindgen_test]
fn test_detect_by_content_json() {
    assert_eq!(detect_format("noext", b"[1, 2, 3]"), Format::Json);
    assert_eq!(detect_format("noext", b"  {\"a\": 1}"), Format::Json);
}

#[wasm_bindgen_test]
fn test_detect_by_content_csv() {
    assert_eq!(detect_format("noext", b"a,b,c\n1,2,3"), Format::Csv);
}

#[wasm_bindgen_test]
fn test_detect_unknown() {
    assert_eq!(detect_format("file.bin", &[0xDE, 0xAD, 0xBE, 0xEF]), Format::Unknown);
}

// =============================================================================
// choose_target
// =============================================================================

#[wasm_bindgen_test]
fn test_choose_target() {
    assert_eq!(choose_target(Format::Json), Some(Format::Csv));
    assert_eq!(choose_target(Format::Csv), Some(Format::Json));
    assert_eq!(choose_target(Format::Markdown), Some(Format::Html));
    assert_eq!(choose_target(Format::Text), Some(Format::Json));
    assert_eq!(choose_target(Format::Html), None);
    assert_eq!(choose_target(Format::Unknown), None);
}

// =============================================================================
// JSON → CSV
// =============================================================================

#[wasm_bindgen_test]
fn test_json_to_csv_basic() {
    let input = br#"[{"a":1,"b":"x"},{"a":2,"b":"y"}]"#;
    let out = json_to_csv(input).expect("convert failed");
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "a,b\n1,x\n2,y\n");
}

#[wasm_bindgen_test]
fn test_json_to_csv_union_of_keys() {
    let input = br#"[{"a":1},{"b":2,"a":3}]"#;
    let out = json_to_csv(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "a,b\n1,\n3,2\n");
}

#[wasm_bindgen_test]
fn test_json_to_csv_escaping() {
    let input = br#"[{"msg":"hello, world"},{"msg":"say \"hi\""}]"#;
    let out = json_to_csv(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("\"hello, world\""));
    assert!(s.contains("\"say \"\"hi\"\"\""));
}

#[wasm_bindgen_test]
fn test_json_to_csv_invalid_json() {
    assert!(json_to_csv(b"{ not json").is_err());
}

// =============================================================================
// CSV → JSON
// =============================================================================

#[wasm_bindgen_test]
fn test_csv_to_json_basic() {
    let input = b"a,b\n1,x\n2,y\n";
    let out = csv_to_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["a"], "1");
    assert_eq!(arr[0]["b"], "x");
    assert_eq!(arr[1]["a"], "2");
}

#[wasm_bindgen_test]
fn test_csv_to_json_empty() {
    let out = csv_to_json(b"").unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "[]");
}

#[wasm_bindgen_test]
fn test_csv_json_roundtrip() {
    let original = br#"[{"name":"Alice","age":"30"},{"name":"Bob","age":"25"}]"#;
    let csv = json_to_csv(original).unwrap();
    let json = csv_to_json(&csv).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["name"], "Alice");
}

// =============================================================================
// Markdown → HTML
// =============================================================================

#[wasm_bindgen_test]
fn test_md_to_html_headers() {
    let input = b"# Title\n\n## Subtitle\n";
    let out = md_to_html(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("<h1>Title</h1>"));
    assert!(s.contains("<h2>Subtitle</h2>"));
}

#[wasm_bindgen_test]
fn test_md_to_html_paragraph() {
    let input = b"Just text\n";
    let out = md_to_html(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s.trim(), "<p>Just text</p>");
}

#[wasm_bindgen_test]
fn test_md_to_html_inline() {
    let input = b"Hello **world** and *universe*\n";
    let out = md_to_html(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("<strong>world</strong>"));
    assert!(s.contains("<em>universe</em>"));
}

#[wasm_bindgen_test]
fn test_md_to_html_code_and_link() {
    let input = b"Use `cargo build` and see [docs](https://example.com)\n";
    let out = md_to_html(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("<code>cargo build</code>"));
    assert!(s.contains("<a href=\"https://example.com\">docs</a>"));
}

#[wasm_bindgen_test]
fn test_md_to_html_escapes_specials() {
    let input = b"5 < 10 & 10 > 5\n";
    let out = md_to_html(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("5 &lt; 10 &amp; 10 &gt; 5"));
}

// =============================================================================
// TXT → JSON stats
// =============================================================================

#[wasm_bindgen_test]
fn test_text_stats() {
    let input = b"hello world\nsecond line\n";
    let s = text_stats(input).unwrap();
    assert_eq!(s.bytes, 24);
    assert_eq!(s.lines, 2);
    assert_eq!(s.words, 4);
}

#[wasm_bindgen_test]
fn test_text_to_json() {
    let out = text_to_json(b"one two\nthree").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["words"], 3);
    assert_eq!(v["lines"], 2);
}

// =============================================================================
// preview_output, replace_extension, format_size
// =============================================================================

#[wasm_bindgen_test]
fn test_preview_output_truncates() {
    let s = preview_output(b"abcdefghij", 5);
    assert!(s.starts_with("abcde"));
    assert!(s.contains("+5"));
}

#[wasm_bindgen_test]
fn test_preview_output_binary() {
    let s = preview_output(&[0xFF, 0xFE, 0xFD], 64);
    assert!(s.contains("<binary>"));
    assert!(s.contains("FF"));
}

#[wasm_bindgen_test]
fn test_replace_extension() {
    assert_eq!(replace_extension("data.json", "csv"), "data.csv");
    assert_eq!(replace_extension("no_ext", "txt"), "no_ext.txt");
    assert_eq!(replace_extension("a.b.c", "x"), "a.b.x");
}

#[wasm_bindgen_test]
fn test_format_size() {
    assert_eq!(format_size(500), "500 Б");
    assert_eq!(format_size(2048), "2.0 КБ");
    assert!(format_size(3 * 1024 * 1024).contains("МБ"));
}

#[wasm_bindgen_test]
fn test_json_to_csv_single_object() {
    // Одиночный объект → 1-строчный CSV.
    let input = br#"{"bytes":29825,"chars":24424,"lines":1061,"words":3292}"#;
    let out = json_to_csv(input).expect("single object");
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "bytes,chars,lines,words\n29825,24424,1061,3292\n");
}

#[wasm_bindgen_test]
fn test_json_to_csv_array_of_primitives() {
    let input = br#"[1, 2, 3, 4]"#;
    let out = json_to_csv(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "value\n1\n2\n3\n4\n");
}

#[wasm_bindgen_test]
fn test_json_to_csv_single_primitive() {
    let input = br#"42"#;
    let out = json_to_csv(input).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "value\n42\n");
}

#[wasm_bindgen_test]
fn test_json_to_csv_empty_and_null() {
    // Пустой массив и null — нечего конвертировать.
    assert!(json_to_csv(b"[]").is_err());
    assert!(json_to_csv(b"null").is_err());
}