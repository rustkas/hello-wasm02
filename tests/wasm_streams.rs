use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// get_stream возвращает поток.
#[wasm_bindgen_test]
async fn test_get_stream_returns_stream() {
    let url = "http://127.0.0.1:3000/api/stream/3";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    // Проверяем, что поток не заблокирован и готов к чтению.
    assert!(!stream.locked());
}

/// read_stream_to_vec читает весь поток.
#[wasm_bindgen_test]
async fn test_read_stream_to_vec() {
    let url = "http://127.0.0.1:3000/api/stream/3";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let bytes = hello_wasm02::fetch::read_stream_to_vec(&stream)
        .await
        .unwrap();
    // 3 чанка по 1024 байта.
    assert_eq!(bytes.len(), 3 * 1024);
}

/// read_stream_chunks возвращает непустой список.
#[wasm_bindgen_test]
async fn test_read_stream_chunks_non_empty() {
    let url = "http://127.0.0.1:3000/api/stream/4";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let chunks = hello_wasm02::fetch::read_stream_chunks(&stream)
        .await
        .unwrap();
    assert_eq!(chunks.len(), 4);
    for (i, chunk) in chunks.iter().enumerate() {
        assert!(!chunk.is_empty(), "чанк {i} пуст");
    }
}

/// Сумма размеров чанков равна общему размеру.
#[wasm_bindgen_test]
async fn test_read_stream_chunks_sum() {
    let url = "http://127.0.0.1:3000/api/stream/5";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let chunks = hello_wasm02::fetch::read_stream_chunks(&stream)
        .await
        .unwrap();
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    assert_eq!(total, 5 * 1024);
}

/// Чтение потока с текстовым содержимым.
#[wasm_bindgen_test]
async fn test_read_stream_handles_text() {
    // JSONPlaceholder отдаёт текст, не поток чанков.
    // Проверяем, что поток можно прочитать и получить осмысленные байты.
    let url = "https://jsonplaceholder.typicode.com/posts/1";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let bytes = hello_wasm02::fetch::read_stream_to_vec(&stream)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("userId"));
}

/// Колбэк вызывается на каждый чанк.
#[wasm_bindgen_test]
async fn test_stream_progress_callback_called() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let url = "http://127.0.0.1:3000/api/stream/3";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();

    let counter = Rc::new(RefCell::new(0usize));
    let counter_clone = counter.clone();

    let total = hello_wasm02::fetch::read_stream_with_callback(&stream, move |_total, _chunk_size| {
        *counter_clone.borrow_mut() += 1;
    })
    .await
    .unwrap();

    assert_eq!(total, 3 * 1024);
    assert_eq!(*counter.borrow(), 3, "колбэк должен вызываться на каждый чанк");
}

/// release_lock освобождает читателя.
#[wasm_bindgen_test]
async fn test_release_lock_after_read() {
    let url = "http://127.0.0.1:3000/api/stream/2";
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    // После read_stream_to_vec поток должен быть разблокирован.
    let _ = hello_wasm02::fetch::read_stream_to_vec(&stream).await.unwrap();
    assert!(!stream.locked(), "поток должен быть разблокирован");
}

/// Сравнение потокового и полного чтения.
#[wasm_bindgen_test]
async fn test_stream_large_file() {
    let url = "http://127.0.0.1:3000/api/stream/20";

    // Полное чтение через ArrayBuffer.
    let start_full = js_sys::Date::now();
    let full = hello_wasm02::fetch::get_bytes(url).await.unwrap();
    let full_time = js_sys::Date::now() - start_full;

    // Потоковое чтение.
    let start_stream = js_sys::Date::now();
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let streamed = hello_wasm02::fetch::read_stream_to_vec(&stream)
        .await
        .unwrap();
    let stream_time = js_sys::Date::now() - start_stream;

    // Оба способа должны дать одинаковое количество байт.
    assert_eq!(full.len(), streamed.len());
    assert_eq!(full.len(), 20 * 1024);

    // Потоковое чтение может быть медленнее (из-за пауз на сервере),
    // но это не ошибка — важно, что данные совпадают.
    assert!(stream_time > 0.0);
    assert!(full_time > 0.0);
}

/// Поток и полный ответ дают одинаковые байты.
#[wasm_bindgen_test]
async fn test_stream_bytes_match_full_response() {
    let url = "http://127.0.0.1:3000/api/stream/5";

    let full = hello_wasm02::fetch::get_bytes(url).await.unwrap();
    let stream = hello_wasm02::fetch::get_stream(url).await.unwrap();
    let streamed = hello_wasm02::fetch::read_stream_to_vec(&stream)
        .await
        .unwrap();

    assert_eq!(full.len(), streamed.len());
    assert_eq!(full, streamed, "байты должны совпадать");
}

/// Abort посреди потока.
#[wasm_bindgen_test]
async fn test_stream_abort_midway() {
    let controller = hello_wasm02::fetch::make_abort_controller().unwrap();
    let signal = controller.signal();

    // Отменяем сразу — запрос не начнётся.
    controller.abort();

    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_signal(Some(&signal));

    let request = web_sys::Request::new_with_str_and_init(
        "http://127.0.0.1:3000/api/stream/5",
        &opts,
    )
    .unwrap();

    let window = web_sys::window().unwrap();
    let result = wasm_bindgen_futures::JsFuture::from(
        window.fetch_with_request(&request),
    )
    .await;

    assert!(result.is_err(), "запрос должен быть отменён");
    let classified = hello_wasm02::fetch::classify_error(result.unwrap_err());
    match classified {
        hello_wasm02::fetch::FetchError::Aborted => {}
        other => panic!("ожидался Aborted, получено {other:?}"),
    }
}