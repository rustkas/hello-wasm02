//! Тесты Streams в Service Worker.
//!
//! **Запускать вручную:** `wasm-pack test --chrome --test wasm_sw_streams`.
//! Требуется, чтобы SW был зарегистрирован и активирован.
//!
//! В headless-Chrome SW регистрируется, но не успевает активироваться
//! до начала тестов. `wasm-bindgen-test` не умеет ждать активации SW.
//! Поэтому тесты помечены `#[ignore]`.

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// SW генерирует поток — клиент читает его через `get_stream`.
#[wasm_bindgen_test]
#[ignore = "requires active Service Worker; run without --headless"]
async fn test_sw_stream_response() {
    let url = "/api/logs/stream";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let bytes = hello_wasm02::streams::read_all(&stream).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);

    // 10 строк NDJSON.
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 10);

    // Каждая строка — валидный JSON.
    for line in lines {
        let json: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(json["seq"].is_number());
        assert!(json["level"].is_string());
    }
}

/// SW модифицирует чанки — клиент видит префикс `[SW]`.
#[wasm_bindgen_test]
#[ignore = "requires active Service Worker; run without --headless"]
async fn test_sw_stream_modification() {
    let url = "/api/proxy-stream/stream/3";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let bytes = hello_wasm02::streams::read_all(&stream).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);

    // Все непустые строки начинаются с префикса, добавленного SW.
    for line in text.lines() {
        if !line.is_empty() {
            assert!(
                line.starts_with("[SW]"),
                "строка должна иметь префикс [SW], получено: {line}"
            );
        }
    }
}

/// Заголовок `X-Generated-By` доказывает, что ответ от SW, а не от сервера.
#[wasm_bindgen_test]
#[ignore = "requires active Service Worker; run without --headless"]
async fn test_sw_marks_response() {
    let url = "/api/logs/stream";
    let resp = hello_wasm02::fetch::send_request_classified(url)
        .await
        .unwrap();
    let header = hello_wasm02::fetch::get_response_header(&resp, "x-generated-by");
    assert_eq!(header.as_deref(), Some("ServiceWorker"));
}

/// Инкрементальное чтение: колбэк вызывается на каждый чанк.
#[wasm_bindgen_test]
#[ignore = "requires active Service Worker; run without --headless"]
async fn test_sw_stream_incremental() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let url = "/api/logs/stream";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();

    let chunks_count = Rc::new(RefCell::new(0usize));
    let counter = chunks_count.clone();

    let total = hello_wasm02::fetch::read_stream_with_callback(&stream, move |_, _| {
        *counter.borrow_mut() += 1;
    })
    .await
    .unwrap();

    assert!(total > 0);
    // SW отдаёт 10 строк — минимум 10 чанков.
    assert!(
        *chunks_count.borrow() >= 10,
        "ожидалось ≥10 чанков, получено {}",
        *chunks_count.borrow()
    );
}