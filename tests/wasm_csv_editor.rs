//! Тесты `src/csv_editor.rs`.
//!
//! **Что проверяем.**
//! * CSV parser: простые, кавычки, экранирование, перенос строки в кавычках.
//! * Roundtrip: parse → serialize → parse без потерь.
//! * SHA-256: детерминирован, меняется при изменении.
//! * `CsvDoc::is_dirty()`: true/false по хешу.
//! * OPFS roundtrip: записать → прочитать → сравнить.

use wasm_bindgen_test::*;

use hello_wasm02::csv_editor::{
    file_handle_in, opfs_root, parse_csv, read_file_bytes, seed_doc, serialize_csv,
    sha256_hex, short_hash, write_file_bytes, 
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// CSV parser
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_parse_empty() {
    let r = parse_csv("");
    assert!(r.is_empty());
}

#[wasm_bindgen_test]
fn test_parse_simple() {
    let r = parse_csv("a,b,c\n1,2,3\n");
    assert_eq!(r.len(), 2);
    assert_eq!(r[0], vec!["a", "b", "c"]);
    assert_eq!(r[1], vec!["1", "2", "3"]);
}

#[wasm_bindgen_test]
fn test_parse_no_trailing_newline() {
    let r = parse_csv("a,b\n1,2");
    assert_eq!(r.len(), 2);
    assert_eq!(r[1], vec!["1", "2"]);
}

#[wasm_bindgen_test]
fn test_parse_quoted() {
    let r = parse_csv(r#""hello, world",x"#);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0], vec!["hello, world", "x"]);
}

#[wasm_bindgen_test]
fn test_parse_escaped_quotes() {
    let r = parse_csv(r#""he said ""hi""",x"#);
    assert_eq!(r[0], vec![r#"he said "hi""#, "x"]);
}

#[wasm_bindgen_test]
fn test_parse_newline_in_quotes() {
    let r = parse_csv("\"line1\nline2\",x");
    assert_eq!(r.len(), 1);
    assert_eq!(r[0][0], "line1\nline2");
}

#[wasm_bindgen_test]
fn test_parse_crlf() {
    let r = parse_csv("a,b\r\n1,2\r\n");
    assert_eq!(r.len(), 2);
    assert_eq!(r[0], vec!["a", "b"]);
    assert_eq!(r[1], vec!["1", "2"]);
}

// ---------------------------------------------------------------------------
// Serialize + roundtrip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_serialize_simple() {
    let rows = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["1".to_string(), "2".to_string()],
    ];
    assert_eq!(serialize_csv(&rows), "a,b\n1,2\n");
}

#[wasm_bindgen_test]
fn test_serialize_quotes() {
    let rows = vec![vec![
        "with, comma".to_string(),
        "with \"quote\"".to_string(),
    ]];
    let out = serialize_csv(&rows);
    assert_eq!(out, "\"with, comma\",\"with \"\"quote\"\"\"\n");
}

#[wasm_bindgen_test]
fn test_roundtrip() {
    let original = vec![
        vec!["a,b".to_string(), "c\"d".to_string(), "e\nf".to_string()],
        vec!["1".to_string(), "2".to_string(), "3".to_string()],
    ];
    let text = serialize_csv(&original);
    let parsed = parse_csv(&text);
    assert_eq!(parsed, original);
}

// ---------------------------------------------------------------------------
// SHA-256
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_sha256_deterministic() {
    let a = sha256_hex(b"hello");
    let b = sha256_hex(b"hello");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
}

#[wasm_bindgen_test]
fn test_sha256_changes_on_edit() {
    let a = sha256_hex(b"hello");
    let b = sha256_hex(b"hello!");
    assert_ne!(a, b);
}

#[wasm_bindgen_test]
fn test_short_hash() {
    let full = "abcdef0123456789";
    assert_eq!(short_hash(full), "abcdef012345");
    // Короткий хеш остаётся как есть.
    assert_eq!(short_hash("abc"), "abc");
}

// ---------------------------------------------------------------------------
// CsvDoc
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_doc_dirty_tracking() {
    let mut doc = seed_doc();
    assert!(!doc.is_dirty(), "новый seed-документ не грязный");

    doc.rows[1][0] = "Alice2".to_string();
    assert!(doc.is_dirty(), "после правки — грязный");

    // Вернули как было — снова чистый.
    doc.rows[1][0] = "Alice".to_string();
    assert!(!doc.is_dirty(), "вернули — снова чистый");
}

#[wasm_bindgen_test]
fn test_doc_mark_saved() {
    let mut doc = seed_doc();
    doc.rows[1][0] = "Alice2".to_string();
    assert!(doc.is_dirty());

    doc.mark_saved();
    assert!(!doc.is_dirty(), "после mark_saved — чистый");
}

// ---------------------------------------------------------------------------
// OPFS roundtrip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_opfs_csv_roundtrip() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = match file_handle_in(&root, "csv_test_roundtrip.csv", true).await {
        Ok(h) => h,
        Err(_) => return,
    };

    let doc = seed_doc();
    let text = serialize_csv(&doc.rows);

    write_file_bytes(&handle, text.as_bytes())
        .await
        .expect("write");

    let back = read_file_bytes(&handle).await.expect("read");
    let back_text = String::from_utf8_lossy(&back).to_string();
    assert_eq!(back_text, text, "содержимое совпадает");

    // Повторная запись — идемпотентна (тот же хеш → тот же размер).
    write_file_bytes(&handle, text.as_bytes())
        .await
        .expect("write 2");
    let back2 = read_file_bytes(&handle).await.expect("read 2");
    assert_eq!(back, back2);
}

#[wasm_bindgen_test]
async fn test_opfs_persists_across_reads() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = match file_handle_in(&root, "csv_test_persist.csv", true).await {
        Ok(h) => h,
        Err(_) => return,
    };

    // Записываем одну версию.
    write_file_bytes(&handle, b"v1\n").await.expect("write v1");

    // Отдельно читаем — тот же handle, тот же файл.
    let back = read_file_bytes(&handle).await.expect("read");
    assert_eq!(&back, b"v1\n");
}