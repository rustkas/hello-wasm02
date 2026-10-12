//! Тесты ручной реализации Streams API.

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// ReadableStream из массива строк читается целиком.
#[wasm_bindgen_test]
async fn test_manual_readable_from_strings() {
    let stream = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
    ])
    .unwrap();

    let bytes = hello_wasm02::streams::read_all(&stream).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&bytes), "ab");
}

/// WritableStream собирает записанные чанки в буфер.
///
/// Здесь читаемой стороны нет — backpressure не участвует.
#[wasm_bindgen_test]
async fn test_manual_writable_into_buffer() {
    let (stream, buffer) = hello_wasm02::streams::writable_into_buffer().unwrap();

    let writer: web_sys::WritableStreamDefaultWriter = stream
        .get_writer()
        .expect("get_writer failed");

    for chunk in ["x", "y"] {
        let js_value: JsValue = JsValue::from_str(chunk);
        let _ = wasm_bindgen_futures::JsFuture::from(
            writer.write_with_chunk(&js_value),
        )
        .await;
    }
    // close() тоже возвращает Promise — его нужно дождаться,
    // иначе writer останется в подвешенном состоянии.
    let _ = wasm_bindgen_futures::JsFuture::from(writer.close()).await;

    assert_eq!(buffer.borrow().join(""), "xy");
}

/// TransformStream переводит строки в верхний регистр.
///
/// Запись и чтение идут **параллельно** через `join`. Без этого
/// возникает deadlock: `TransformStream` по умолчанию имеет
/// `readable` с `highWaterMark = 0`, и второй `write` не разрешится,
/// пока читатель не заберёт первый чанк из readable.
#[wasm_bindgen_test]
async fn test_manual_transform_upper() {
    use futures::future::join;

    let transform =
        hello_wasm02::streams::transform_with(|s: String| s.to_uppercase()).unwrap();
    let readable = transform.readable();
    let writable = transform.writable();

    let writer: web_sys::WritableStreamDefaultWriter = writable
        .get_writer()
        .expect("get_writer failed");

    let write_task = async move {
        for chunk in ["ab", "cd"] {
            let js_value: JsValue = JsValue::from_str(chunk);
            let _ = wasm_bindgen_futures::JsFuture::from(
                writer.write_with_chunk(&js_value),
            )
            .await;
        }
        let _ = wasm_bindgen_futures::JsFuture::from(writer.close()).await;
    };

    let read_task = hello_wasm02::streams::read_all(&readable);

    let (_, read_result) = join(write_task, read_task).await;
    let bytes = read_result.unwrap();

    assert_eq!(String::from_utf8_lossy(&bytes), "ABCD");
}

/// pipeThrough соединяет источник и TransformStream.
#[wasm_bindgen_test]
async fn test_manual_pipe_through() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
    ])
    .unwrap();

    let transform =
        hello_wasm02::streams::transform_with(|s: String| format!("[{s}]")).unwrap();

    let out = hello_wasm02::streams::call_pipe_through(&source, &transform)
        .expect("pipeThrough failed");

    let bytes = hello_wasm02::streams::read_all(&out).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&bytes), "[a][b]");
}

/// JS-поток превращается в Rust Stream через mpsc.
#[wasm_bindgen_test]
async fn test_manual_js_to_rust_stream() {
    use futures::StreamExt;

    let source = hello_wasm02::streams::readable_from_strings(vec![
        "1".to_string(),
        "2".to_string(),
    ])
    .unwrap();

    let rust_stream = hello_wasm02::streams::js_to_rust_stream(source);

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

    assert_eq!(total, "12");
}