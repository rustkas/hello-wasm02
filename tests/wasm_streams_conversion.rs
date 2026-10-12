//! Тесты для секции 17.5: конвертация потоков между JS и Rust.

use futures::StreamExt;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// JS → Rust Stream через wasm-streams.
///
/// Проверяем базовый сценарий с `filter_map` + `collect`.
/// Для AsyncRead этот способ не подходит (см. следующий тест).
#[wasm_bindgen_test]
async fn test_js_to_rust_stream_wasm_streams() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
    ])
    .unwrap();

    let rust_stream = hello_wasm02::streams_wasm::js_to_rust_stream(source);

    let chunks: Vec<Vec<u8>> = rust_stream
        .filter_map(|r: Result<Vec<u8>, hello_wasm02::fetch::FetchError>| async move {
            r.ok()
        })
        .collect::<Vec<Vec<u8>>>()
        .await;

    let total: String = chunks
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .collect();

    assert_eq!(total, "abc");
}

/// JS → Rust Stream через mpsc.
///
/// Альтернативная реализация: тот же результат, но через
/// собственный mpsc-канал. Используется, когда нужна надёжная
/// связка с AsyncRead.
#[wasm_bindgen_test]
async fn test_js_to_rust_stream_mpsc() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "x".to_string(),
        "y".to_string(),
        "z".to_string(),
    ])
    .unwrap();

    let rust_stream = hello_wasm02::streams_wasm::js_to_rust_stream_mpsc(source);

    let chunks: Vec<Vec<u8>> = rust_stream
        .filter_map(|r: Result<Vec<u8>, hello_wasm02::fetch::FetchError>| async move {
            r.ok()
        })
        .collect::<Vec<Vec<u8>>>()
        .await;

    let total: String = chunks
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .collect();

    assert_eq!(total, "xyz");
}

/// Rust Stream → JS ReadableStream.
///
/// Создаём поток в Rust, превращаем в JS-поток, читаем его
/// через наш же JS-поток reader.
#[wasm_bindgen_test]
async fn test_rust_stream_to_js() {
    let rust_source = futures::stream::iter(vec!["one", "two", "three"])
        .map(|s: &str| Ok::<JsValue, JsValue>(JsValue::from_str(s)));

    let js_stream = hello_wasm02::streams_wasm::rust_stream_to_js(rust_source);

    let chunks = hello_wasm02::streams::read_all_strings(&js_stream)
        .await
        .unwrap();

    assert_eq!(chunks.join(""), "onetwothree");
}

/// JS WritableStream → Rust Sink.
///
/// Создаём JS-приёмник, оборачиваем в Rust Sink, отправляем
/// туда данные через `SinkExt::send`.
#[wasm_bindgen_test]
async fn test_js_to_rust_sink() {
    use futures::SinkExt;

    let (stream, buffer) = hello_wasm02::streams::writable_into_buffer().unwrap();

    let mut sink = hello_wasm02::streams_wasm::js_to_rust_sink(stream);

    for chunk in ["alpha", "beta", "gamma"] {
        let js_value: JsValue = JsValue::from_str(chunk);
        sink.send(js_value).await.unwrap();
    }

    sink.close().await.unwrap();

    assert_eq!(buffer.borrow().join(""), "alphabetagamma");
}

/// JS ReadableStream → Rust AsyncRead.
///
/// Используем `read_exact`, потому что `wasm-streams::into_stream`
/// не всегда присылает корректный EOF. Наш mpsc-адаптер тоже не
/// идеален для «чтения до конца», но `read_exact` с известной длиной
/// работает надёжно.
#[wasm_bindgen_test]
async fn test_js_to_async_read() {
    use futures::io::AsyncReadExt;

    let source = hello_wasm02::streams::readable_bytes(vec![
        b"hello".to_vec(),
        b" world".to_vec(),
    ])
    .unwrap();

    let mut reader = hello_wasm02::streams_wasm::js_to_async_read(source).unwrap();

    let mut buffer = vec![0u8; 11];
    reader.read_exact(&mut buffer).await.unwrap();

    assert_eq!(&buffer, b"hello world");
}

/// Round-trip: Rust → JS → Rust.
///
/// Данные, сгенерированные в Rust, проходят через JS-поток
/// и возвращаются в Rust без изменений.
#[wasm_bindgen_test]
async fn test_round_trip() {
    let rust_source = futures::stream::iter(vec!["one", "two", "three"])
        .map(|s: &str| Ok::<JsValue, JsValue>(JsValue::from_str(s)));

    let js_stream = hello_wasm02::streams_wasm::rust_stream_to_js(rust_source);

    let back = hello_wasm02::streams_wasm::js_to_rust_stream(js_stream);

    let chunks: Vec<Vec<u8>> = back
        .filter_map(|r: Result<Vec<u8>, hello_wasm02::fetch::FetchError>| async move {
            r.ok()
        })
        .collect::<Vec<Vec<u8>>>()
        .await;

    let total: String = chunks
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .collect();

    assert_eq!(total, "onetwothree");
}