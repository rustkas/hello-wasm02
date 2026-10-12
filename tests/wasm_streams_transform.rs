//! Тесты для секции 17.3: TransformStream, pipeThrough, pipeTo.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Базовое преобразование: upper case.
#[wasm_bindgen_test]
async fn test_transform_upper() {
    let transform =
        hello_wasm02::streams::transform_with(|s: String| s.to_uppercase()).unwrap();

    let result = hello_wasm02::streams::process_transform(
        transform,
        vec!["ab".to_string(), "cd".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(result.join(""), "ABCD");
}

/// Ошибка в transform доходит до читателя.
#[wasm_bindgen_test]
async fn test_transform_error_propagates() {
    let transform = hello_wasm02::streams::transform_with_error(|chunk: String| {
        if chunk.contains("fail") {
            Err("test error".to_string())
        } else {
            Ok(chunk)
        }
    })
    .unwrap();

    let result = hello_wasm02::streams::process_transform(
        transform,
        vec!["ok".to_string(), "fail".to_string()],
    )
    .await;

    assert!(result.is_err(), "ошибка в transform должна проброситься");
}

/// Без flush последний чанк теряется; с flush — нет.
#[wasm_bindgen_test]
async fn test_transform_flush() {
    // Без flush: остаток после последнего \n теряется.
    let no_flush = hello_wasm02::streams::line_splitter_no_flush().unwrap();
    let result_no_flush = hello_wasm02::streams::process_transform(
        no_flush,
        vec!["a\nb\nc".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(
        result_no_flush.join(""),
        "a\nb\n",
        "без flush последняя строка теряется"
    );

    // С flush: остаток отдаётся.
    let with_flush = hello_wasm02::streams::line_splitter().unwrap();
    let result_with_flush = hello_wasm02::streams::process_transform(
        with_flush,
        vec!["a\nb\nc".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(
        result_with_flush.join(""),
        "a\nb\nc",
        "с flush последняя строка сохраняется"
    );
}

/// Параллельная запись и чтение не дают deadlock.
///
/// Если бы мы писали все чанки, потом читали, второй write завис бы.
/// process_transform использует join — тест завершается без таймаута.
#[wasm_bindgen_test]
async fn test_transform_parallel() {
    let transform =
        hello_wasm02::streams::transform_with(|s: String| format!("[{s}]")).unwrap();

    let input = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
        "e".to_string(),
    ];

    let result = hello_wasm02::streams::process_transform(transform, input)
        .await
        .unwrap();

    assert_eq!(result.join(""), "[a][b][c][d][e]");
}

/// pipeThrough соединяет источник и TransformStream.
#[wasm_bindgen_test]
async fn test_transform_pipe_through() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
    ])
    .unwrap();

    let transform =
        hello_wasm02::streams::transform_with(|s: String| format!("<{s}>")).unwrap();

    let piped = hello_wasm02::streams::call_pipe_through(&source, &transform).unwrap();
    let result = hello_wasm02::streams::read_all_strings(&piped).await.unwrap();

    assert_eq!(result.join(""), "<a><b>");
}

/// Line splitter режет вход произвольной нарезки на строки.
#[wasm_bindgen_test]
async fn test_line_splitter() {
    let transform = hello_wasm02::streams::line_splitter().unwrap();

    // Нарезка в середине слов.
    let input = vec![
        "hello\nwor".to_string(),
        "ld\nfoo\n".to_string(),
        "bar".to_string(),
    ];

    let result = hello_wasm02::streams::process_transform(transform, input)
        .await
        .unwrap();

    assert_eq!(result.join(""), "hello\nworld\nfoo\nbar");
}

/// pipeTo: обычный случай переносит все данные и закрывает приёмник.
#[wasm_bindgen_test]
async fn test_pipe_to_basic() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "x".to_string(),
        "y".to_string(),
        "z".to_string(),
    ])
    .unwrap();

    let buffer: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let buffer_c = buffer.clone();
    let close_called: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let close_c = close_called.clone();

    let sink = hello_wasm02::streams::writable_with_callbacks(
        move |chunk| buffer_c.borrow_mut().push(chunk),
        move || *close_c.borrow_mut() = true,
        |_| {},
    )
    .unwrap();

    hello_wasm02::streams::pipe_to(&source, &sink, false, false, false)
        .await
        .unwrap();

    assert_eq!(buffer.borrow().join(""), "xyz");
    assert!(*close_called.borrow(), "close должен быть вызван");
}

/// pipeTo с preventClose оставляет приёмник открытым.
#[wasm_bindgen_test]
async fn test_pipe_to_prevent_close() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
    ])
    .unwrap();

    let buffer: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let buffer_c = buffer.clone();
    let close_called: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let close_c = close_called.clone();

    let sink = hello_wasm02::streams::writable_with_callbacks(
        move |chunk| buffer_c.borrow_mut().push(chunk),
        move || *close_c.borrow_mut() = true,
        |_| {},
    )
    .unwrap();

    hello_wasm02::streams::pipe_to(&source, &sink, true, false, false)
        .await
        .unwrap();

    assert_eq!(buffer.borrow().join(""), "ab");
    assert!(
        !*close_called.borrow(),
        "с preventClose приёмник не должен закрываться"
    );

    // Приёмник остался открытым — можем дописать.
    let writer: web_sys::WritableStreamDefaultWriter = sink.get_writer().unwrap();
    let _ = wasm_bindgen_futures::JsFuture::from(
        writer.write_with_chunk(&JsValue::from_str("c")),
    )
    .await;
    let _ = wasm_bindgen_futures::JsFuture::from(writer.close()).await;

    assert_eq!(buffer.borrow().join(""), "abc");
}