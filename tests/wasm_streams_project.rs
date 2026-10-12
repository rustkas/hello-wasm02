//! Тесты для секции 17.7: стриминговый JSON-парсер.
//!
//! Проверяем LineReader и цепочку построчного парсинга на
//! in-memory источнике (`futures::io::Cursor`). Это позволяет
//! тестировать логику без сети и без браузерного fetch.

use serde::Deserialize;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Одна запись для тестов.
#[derive(Deserialize, Debug)]
struct Record {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    message: String,
}

/// Читаем три строки через LineReader.
#[wasm_bindgen_test]
async fn test_line_reader_basic() {
    use futures::io::Cursor;

    let input = b"line one\nline two\nline three\n";
    let cursor = Cursor::new(input.to_vec());

    let mut reader = hello_wasm02::streams::LineReader::new(cursor);

    assert_eq!(reader.next_line().await.unwrap().as_deref(), Some("line one"));
    assert_eq!(reader.next_line().await.unwrap().as_deref(), Some("line two"));
    assert_eq!(
        reader.next_line().await.unwrap().as_deref(),
        Some("line three")
    );
    assert_eq!(reader.next_line().await.unwrap(), None);
}

/// Последняя строка без завершающего \n не теряется.
#[wasm_bindgen_test]
async fn test_line_reader_no_trailing_newline() {
    use futures::io::Cursor;

    let input = b"first\nsecond\nthird without newline";
    let cursor = Cursor::new(input.to_vec());

    let mut reader = hello_wasm02::streams::LineReader::new(cursor);

    assert_eq!(reader.next_line().await.unwrap().as_deref(), Some("first"));
    assert_eq!(reader.next_line().await.unwrap().as_deref(), Some("second"));
    assert_eq!(
        reader.next_line().await.unwrap().as_deref(),
        Some("third without newline")
    );
    assert_eq!(reader.next_line().await.unwrap(), None);
}

/// Читаем NDJSON через цепочку и парсим каждый объект.
#[wasm_bindgen_test]
async fn test_ndjson_parse() {
    use futures::io::Cursor;

    let ndjson = concat!(
        "{\"id\":1,\"message\":\"first\"}\n",
        "{\"id\":2,\"message\":\"second\"}\n",
        "{\"id\":3,\"message\":\"third\"}\n",
    );
    let cursor = Cursor::new(ndjson.as_bytes().to_vec());

    let mut reader = hello_wasm02::streams::LineReader::new(cursor);

    let mut records: Vec<Record> = Vec::new();
    while let Some(line) = reader.next_line().await.unwrap() {
        let record: Record = serde_json::from_str(&line).unwrap();
        records.push(record);
    }

    assert_eq!(records.len(), 3);
    assert_eq!(records[0].id, 1);
    assert_eq!(records[0].message, "first");
    assert_eq!(records[1].id, 2);
    assert_eq!(records[2].id, 3);
}

/// Callback вызывается на каждый чанк при чтении с прогрессом.
#[wasm_bindgen_test]
async fn test_progress_callback_called() {
    use futures::io::Cursor;
    use std::cell::RefCell;
    use std::rc::Rc;

    let data = b"0123456789ABCDEFGHIJ".to_vec();
    let total = data.len();
    let cursor = Cursor::new(data);

    let sizes: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let sizes_c = sizes.clone();

    let result = hello_wasm02::streams::read_all_async_with_progress(
        cursor,
        move |n| {
            sizes_c.borrow_mut().push(n);
        },
    )
    .await
    .unwrap();

    assert_eq!(result.len(), total);

    // Колбэк вызывался хотя бы один раз.
    let readings = sizes.borrow().clone();
    assert!(
        !readings.is_empty(),
        "callback должен быть вызван хотя бы один раз"
    );

    // Последнее значение — общий размер.
    assert_eq!(*readings.last().unwrap(), total);

    // Значения монотонно возрастают.
    for i in 1..readings.len() {
        assert!(
            readings[i] >= readings[i - 1],
            "значения должны неубывать: {:?}",
            readings
        );
    }
}