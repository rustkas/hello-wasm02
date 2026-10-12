//! Тесты Streams API через крейт `wasm-streams`.

use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// JS-поток превращается в Rust Stream через wasm-streams.
#[wasm_bindgen_test]
async fn test_crate_js_to_rust_stream() {
    use futures::StreamExt;

    let source = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
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

    assert_eq!(total, "ab");
}

/// JS-поток превращается в AsyncRead.
///
/// Используем `readable_bytes` — настоящий byte stream, а не
/// `readable_from_strings`. Разница принципиальна: `into_async_read`
/// из wasm-streams использует BYOB-читатель, который работает
/// только с byte streams.
///
/// Читаем через `read_exact`, а не `read_to_end`: BYOB-читатель
/// под капотом wasm-streams корректно отдаёт чанки только при чтении
/// в буфер известного размера.
#[wasm_bindgen_test]
async fn test_crate_js_to_async_read() {
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

/// JS-поток превращается в Rust Sink.
#[wasm_bindgen_test]
async fn test_crate_js_to_sink() {
    use futures::SinkExt;

    let (stream, buffer) = hello_wasm02::streams::writable_into_buffer().unwrap();
    let mut sink = hello_wasm02::streams_wasm::js_to_rust_sink(stream);

    for chunk in ["a", "b"] {
        let js_value: JsValue = JsValue::from_str(chunk);
        sink.send(js_value).await.expect("sink send failed");
    }
    sink.close().await.ok();

    assert_eq!(buffer.borrow().join(""), "ab");
}

/// Round-trip: Rust Stream → JS ReadableStream → Rust Stream.
#[wasm_bindgen_test]
async fn test_crate_rust_stream_to_js_roundtrip() {
    use futures::StreamExt;

    // Явный тип Ok::<JsValue, JsValue> — иначе компилятор не выведет тип ошибки.
    let rust_source = futures::stream::iter(vec!["x", "y", "z"])
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

    assert_eq!(total, "xyz");
}

/// TransformStream через обёртку wasm-streams.
///
/// Запись и чтение идут параллельно через `join`. Без этого
/// возникает deadlock: `TransformStream` по умолчанию имеет
/// `readable` с `highWaterMark = 0`, и второй `send` не разрешится,
/// пока читатель не заберёт первый чанк из readable.
#[wasm_bindgen_test]
async fn test_crate_wrap_transform() {
    use futures::{SinkExt, StreamExt};

    let transform =
        hello_wasm02::streams::transform_with(|s: String| s.to_uppercase()).unwrap();

    // wrap_transform возвращает пару (readable, writable) как
    // обёртки wasm-streams.
    let (readable, writable) = hello_wasm02::streams_wasm::wrap_transform(transform);

    // Sink пишет в writable, Stream читает из readable.
    let mut sink = writable.into_sink();
    let stream = readable.into_stream();

    // Задача записи: отправить два чанка и закрыть sink.
    let write_task = async move {
        for chunk in ["abc", "def"] {
            let js_value: JsValue = JsValue::from_str(chunk);
            sink.send(js_value).await.expect("sink send failed");
        }
        sink.close().await.ok();
    };

    // Задача чтения: собрать чанки, привести их к Vec<u8>.
    let read_task = async move {
        let chunks: Vec<Vec<u8>> = stream
            .filter_map(|r: Result<JsValue, JsValue>| async move {
                r.ok().map(|v| {
                    if let Ok(arr) = v.clone().dyn_into::<js_sys::Uint8Array>() {
                        arr.to_vec()
                    } else {
                        v.as_string().unwrap_or_default().into_bytes()
                    }
                })
            })
            .collect::<Vec<Vec<u8>>>()
            .await;
        chunks
    };

    // Обе задачи — параллельно. join будет переключаться между ними,
    // освобождая backpressure по мере чтения.
    let (_, chunks) = futures::future::join(write_task, read_task).await;

    let total: String = chunks
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .collect();

    assert_eq!(total, "ABCDEF");
}