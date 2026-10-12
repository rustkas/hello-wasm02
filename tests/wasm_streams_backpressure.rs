//! Тесты для секции 17.4: Backpressure и очереди.

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Начальный `desiredSize` равен `highWaterMark`.
#[wasm_bindgen_test]
async fn test_desired_size_signals() {
    let (stream, _buffer) = hello_wasm02::streams::writable_with_strategy(3).unwrap();

    let writer: web_sys::WritableStreamDefaultWriter = stream.get_writer().unwrap();

    // desired_size() возвращает Result<Option<f64>, JsValue>.
    let desired: Option<f64> = writer.desired_size().ok().flatten();
    assert_eq!(
        desired,
        Some(3.0),
        "начальный desiredSize должен быть равен highWaterMark"
    );
}

/// `CountQueuingStrategy` применяется к `ReadableStream`.
#[wasm_bindgen_test]
async fn test_count_strategy() {
    let strategy = hello_wasm02::streams::count_strategy(2).unwrap();

    let stream = hello_wasm02::streams::readable_with_strategy(
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        &strategy,
    )
    .unwrap();

    let result = hello_wasm02::streams::read_all_strings(&stream)
        .await
        .unwrap();

    assert_eq!(result.join(""), "abc");
}

/// Byte stream с `highWaterMark` работает.
///
/// Для byte stream спецификация запрещает передавать `size()`
/// в стратегии — размер каждого чанка вычисляется автоматически
/// как `byteLength`. Мы передаём только `{ highWaterMark: N }`.
#[wasm_bindgen_test]
async fn test_byte_length_strategy() {
    let stream = hello_wasm02::streams::readable_bytes_with_strategy(
        vec![b"abcd".to_vec(), b"efgh".to_vec()],
        16,
    )
    .unwrap();

    let result = hello_wasm02::streams::read_all(&stream).await.unwrap();

    assert_eq!(String::from_utf8_lossy(&result), "abcdefgh");
}

/// При медленном sink очередь заполняется — `desiredSize` становится <= 0.
#[wasm_bindgen_test]
async fn test_writer_ready_waits() {
    // Sink с задержкой 100 мс на write.
    let sink = js_sys::Object::new();

    let write_fn = wasm_bindgen::closure::Closure::<
        dyn FnMut(JsValue) -> js_sys::Promise,
    >::wrap(Box::new(|_chunk: JsValue| {
        js_sys::Promise::new(&mut |resolve, _reject| {
            let resolve_cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                resolve.call0(&JsValue::UNDEFINED).ok();
            });
            if let Some(window) = web_sys::window() {
                window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        resolve_cb.unchecked_ref(),
                        100,
                    )
                    .ok();
            }
        })
    }));

    let close_fn =
        wasm_bindgen::closure::Closure::<dyn FnMut()>::wrap(Box::new(|| {}));
    let abort_fn =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::wrap(Box::new(|_| {}));

    js_sys::Reflect::set(&sink, &"write".into(), write_fn.as_ref().unchecked_ref()).unwrap();
    js_sys::Reflect::set(&sink, &"close".into(), close_fn.as_ref().unchecked_ref()).unwrap();
    js_sys::Reflect::set(&sink, &"abort".into(), abort_fn.as_ref().unchecked_ref()).unwrap();
    write_fn.forget();
    close_fn.forget();
    abort_fn.forget();

    let strategy = hello_wasm02::streams::count_strategy(1).unwrap();

    let ctor_value = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("WritableStream"))
        .unwrap();
    let ctor: js_sys::Function = ctor_value.dyn_into().unwrap();

    let args = js_sys::Array::new();
    args.push(&sink);
    args.push(&strategy);

        // Function::construct отсутствует в js-sys 0.3.105.
    // Используем Reflect::construct — низкоуровневый вызов,
    // работающий во всех версиях.
    let stream_value = js_sys::Reflect::construct(ctor.as_ref(), &args).unwrap();
    let stream: web_sys::WritableStream = stream_value.unchecked_into();

    let writer: web_sys::WritableStreamDefaultWriter = stream.get_writer().unwrap();

    // Пишем три чанка без await — очередь накапливается.
    let _p1 = writer.write_with_chunk(&JsValue::from_str("a"));
    let _p2 = writer.write_with_chunk(&JsValue::from_str("b"));
    let _p3 = writer.write_with_chunk(&JsValue::from_str("c"));

    gloo_timers::future::TimeoutFuture::new(10).await;

    let desired: Option<f64> = writer.desired_size().ok().flatten();
    assert!(
        desired.unwrap_or(1.0) <= 0.0,
        "после трёх write без await desiredSize должен быть <= 0, получено {desired:?}"
    );
}
