//! Тесты для секции 17.1: ReadableStream как источник данных.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Поток из массива читается целиком.
#[wasm_bindgen_test]
async fn test_read_from_array() {
    let stream = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
    ])
    .unwrap();

    let bytes = hello_wasm02::streams::read_all(&stream).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&bytes), "abc");
}

/// Генератор отдаёт ожидаемую последовательность.
#[wasm_bindgen_test]
async fn test_read_from_generator() {
    let counter: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
    let counter_clone = counter.clone();

    let stream = hello_wasm02::streams::readable_from_generator(move || {
        let mut c = counter_clone.borrow_mut();
        if *c > 5 {
            None
        } else {
            let v = *c;
            *c += 1;
            Some(format!("{v}"))
        }
    })
    .unwrap();

    let bytes = hello_wasm02::streams::read_all(&stream).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&bytes), "12345");
}

/// Асинхронный pull отдаёт данные с задержкой, но все.
#[wasm_bindgen_test]
async fn test_read_from_async_pull() {
    let counter: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
    let counter_clone = counter.clone();

    let stream = hello_wasm02::streams::readable_async(move || {
        let counter = counter_clone.clone();
        async move {
            gloo_timers::future::TimeoutFuture::new(50).await;
            let mut c = counter.borrow_mut();
            if *c > 3 {
                None
            } else {
                let v = *c;
                *c += 1;
                Some(format!("{v}"))
            }
        }
    })
    .unwrap();

    let start = js_sys::Date::now();
    let bytes = hello_wasm02::streams::read_all(&stream).await.unwrap();
    let elapsed = js_sys::Date::now() - start;

    assert_eq!(String::from_utf8_lossy(&bytes), "123");
    // Три чанка по 50 мс каждый — минимум 150 мс.
    assert!(elapsed >= 150.0, "прошло {elapsed} мс, ожидалось ≥150");
}

/// Поток locked, пока у него есть reader.
#[wasm_bindgen_test]
async fn test_stream_locked() {
    let stream = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
    ])
    .unwrap();

    assert!(!hello_wasm02::streams::stream_is_locked(&stream));

    let reader: web_sys::ReadableStreamDefaultReader =
        stream.get_reader().dyn_into().unwrap();

    assert!(hello_wasm02::streams::stream_is_locked(&stream));

    reader.release_lock();
    assert!(!hello_wasm02::streams::stream_is_locked(&stream));
}

/// reader.cancel() прекращает чтение, следующий read возвращает done.
#[wasm_bindgen_test]
async fn test_reader_cancel() {
    let stream = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
    ])
    .unwrap();

    let reader: web_sys::ReadableStreamDefaultReader =
        stream.get_reader().dyn_into().unwrap();

    // Отменяем.
    let _ = wasm_bindgen_futures::JsFuture::from(reader.cancel()).await;

    // После cancel следующий read возвращает done.
    let result = wasm_bindgen_futures::JsFuture::from(reader.read()).await.unwrap();
    let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
        .unwrap()
        .as_bool()
        .unwrap();
    assert!(done, "после cancel поток должен быть закрыт");

    reader.release_lock();
}

/// tee() — оба рукава получают все чанки.
#[wasm_bindgen_test]
async fn test_tee_both_branches_receive_data() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "x".to_string(),
        "y".to_string(),
        "z".to_string(),
    ])
    .unwrap();

    let (branch_a, branch_b) = hello_wasm02::streams::tee_stream(&source).unwrap();

    let (res_a, res_b) = futures::future::join(
        hello_wasm02::streams::read_all(&branch_a),
        hello_wasm02::streams::read_all(&branch_b),
    )
    .await;

    let text_a = String::from_utf8_lossy(&res_a.unwrap()).to_string();
    let text_b = String::from_utf8_lossy(&res_b.unwrap()).to_string();

    assert_eq!(text_a, "xyz");
    assert_eq!(text_b, "xyz");
}

/// tee() — исходный поток становится locked.
#[wasm_bindgen_test]
async fn test_tee_source_locked() {
    let source = hello_wasm02::streams::readable_from_strings(vec![
        "a".to_string(),
    ])
    .unwrap();

    assert!(!hello_wasm02::streams::stream_is_locked(&source));

    let _ = hello_wasm02::streams::tee_stream(&source).unwrap();

    assert!(
        hello_wasm02::streams::stream_is_locked(&source),
        "после tee() исходный поток должен быть locked"
    );

    // get_reader() на locked-потоке должен вернуть объект,
    // но попытка создать второго reader'а — бросить TypeError.
    // Проверим через JS-вызов напрямую.
    let get_reader = js_sys::Reflect::get(&source, &JsValue::from_str("getReader"))
        .unwrap()
        .dyn_into::<js_sys::Function>()
        .unwrap();
    let result = get_reader.call0(&source);
    assert!(result.is_err(), "второй getReader() должен упасть");
}