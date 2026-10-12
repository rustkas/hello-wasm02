//! Тесты для секции 17.2: WritableStream как приёмник данных.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Все чанки дошли до sink в правильном порядке.
#[wasm_bindgen_test]
async fn test_write_multiple_chunks() {
    let buffer: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let buffer_c = buffer.clone();

    let stream = hello_wasm02::streams::writable_with_callbacks(
        move |chunk| buffer_c.borrow_mut().push(chunk),
        || {},
        |_| {},
    )
    .unwrap();

    hello_wasm02::streams::write_all(
        &stream,
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
    )
    .await
    .unwrap();

    let collected = buffer.borrow().join("");
    assert_eq!(collected, "abc");
}

/// close-колбэк вызывается при writer.close().
#[wasm_bindgen_test]
async fn test_close_calls_close_callback() {
    let close_called: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let close_called_c = close_called.clone();

    let stream = hello_wasm02::streams::writable_with_callbacks(
        |_chunk| {},
        move || *close_called_c.borrow_mut() = true,
        |_| {},
    )
    .unwrap();

    hello_wasm02::streams::write_all(&stream, vec!["x".to_string()])
        .await
        .unwrap();

    assert!(
        *close_called.borrow(),
        "close callback должен быть вызван после write_all"
    );
}

/// Запись после close отклоняется с ошибкой.
#[wasm_bindgen_test]
async fn test_write_after_close_fails() {
    let stream =
        hello_wasm02::streams::writable_with_callbacks(|_chunk| {}, || {}, |_| {}).unwrap();

    let writer: web_sys::WritableStreamDefaultWriter = stream.get_writer().unwrap();

    // Первый write успешен.
    let _ =
        wasm_bindgen_futures::JsFuture::from(writer.write_with_chunk(&JsValue::from_str("first")))
            .await;

    // Close.
    let _ = wasm_bindgen_futures::JsFuture::from(writer.close()).await;

    // Второй write должен упасть.
    let result =
        wasm_bindgen_futures::JsFuture::from(writer.write_with_chunk(&JsValue::from_str("second")))
            .await;

    assert!(result.is_err(), "write после close должен вернуть ошибку");
}

/// abort-колбэк вызывается с переданной причиной.
#[wasm_bindgen_test]
async fn test_abort_calls_abort_callback() {
    let abort_reason: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let abort_reason_c = abort_reason.clone();

    let stream = hello_wasm02::streams::writable_with_callbacks(
        |_chunk| {},
        || {},
        move |reason| *abort_reason_c.borrow_mut() = reason,
    )
    .unwrap();

    let writer: web_sys::WritableStreamDefaultWriter = stream.get_writer().unwrap();

    let _ = wasm_bindgen_futures::JsFuture::from(
        writer.abort_with_reason(&JsValue::from_str("test abort")),
    )
    .await;

    let reason = abort_reason.borrow().clone();
    assert_eq!(reason.as_deref(), Some("test abort"));
}

/// Ошибка в sink.write прокидывается наружу.
///
/// Используем `js_sys::eval`, чтобы создать sink, чей write бросает
/// исключение. Через Rust-замыкание бросить JS-исключение из `write`
/// нельзя — Closure имеет фиксированную сигнатуру без Result.
#[wasm_bindgen_test]
async fn test_error_in_write_propagates() {
    // js_sys::eval возвращает JsValue, а new_with_underlying_sink
    // ожидает &js_sys::Object. Приводим тип через dyn_into.
    let sink: js_sys::Object = js_sys::eval(
        r#"({
        write(chunk) {
            if (String(chunk).includes('fail')) {
                throw new Error('sink write failed');
            }
        },
        close() {},
        abort() {},
    })"#,
    )
    .unwrap()
    .dyn_into()
    .expect("eval result is not an Object");

    let stream = web_sys::WritableStream::new_with_underlying_sink(&sink).unwrap();

    let writer: web_sys::WritableStreamDefaultWriter = stream.get_writer().unwrap();

    // Первый write — успех.
    let _ = wasm_bindgen_futures::JsFuture::from(writer.write_with_chunk(&JsValue::from_str("ok")))
        .await;

    // Второй write содержит "fail" — должен упасть.
    let result = wasm_bindgen_futures::JsFuture::from(
        writer.write_with_chunk(&JsValue::from_str("this will fail")),
    )
    .await;

    assert!(result.is_err(), "write с 'fail' должен вернуть ошибку");

    // Извлекаем сообщение.
    if let Err(e) = result {
        if let Some(err) = e.dyn_ref::<js_sys::Error>() {
            let msg = String::from(err.message());
            assert!(
                msg.contains("sink write failed"),
                "ожидалось сообщение 'sink write failed', получено: {msg}"
            );
        }
    }
}
