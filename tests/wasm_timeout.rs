use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Создание AbortController не падает.
#[wasm_bindgen_test]
async fn test_make_abort_controller() {
    let controller = hello_wasm02::fetch::make_abort_controller().unwrap();
    let signal = controller.signal();
    assert!(!signal.aborted(), "новый сигнал не должен быть отменён");
}

/// Новый сигнал не отменён.
#[wasm_bindgen_test]
async fn test_abort_signal_initially_not_aborted() {
    let controller = hello_wasm02::fetch::make_abort_controller().unwrap();
    assert!(!controller.signal().aborted());
}

/// После abort() сигнал помечен как отменённый.
#[wasm_bindgen_test]
async fn test_abort_signal_after_abort() {
    let controller = hello_wasm02::fetch::make_abort_controller().unwrap();
    controller.abort();
    assert!(controller.signal().aborted());
}

/// Таймаут срабатывает: /api/slow/5 не успевает за 200мс.
#[wasm_bindgen_test]
async fn test_timeout_fires() {
    let result = hello_wasm02::fetch::get_with_timeout(
        "http://127.0.0.1:3000/api/slow/5",
        200,
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Aborted => {}
        other => panic!("ожидался Aborted, получено {other:?}"),
    }
}

/// Быстрый запрос не должен сработать по таймауту.
#[wasm_bindgen_test]
async fn test_timeout_does_not_fire_when_fast() {
    // JSONPlaceholder отвечает быстро, таймаут 5 секунд не сработает.
    let result = hello_wasm02::fetch::get_with_timeout(
        "https://jsonplaceholder.typicode.com/posts/1",
        5000,
    )
    .await;

    assert!(result.is_ok(), "быстрый запрос не должен отменяться");
    assert_eq!(result.unwrap().status(), 200);
}

/// Ручная отмена во время запроса.
#[wasm_bindgen_test]
async fn test_abort_during_request() {
    let controller = hello_wasm02::fetch::make_abort_controller().unwrap();
    let signal = controller.signal();

    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_signal(Some(&signal));

    let request = web_sys::Request::new_with_str_and_init(
        "http://127.0.0.1:3000/api/slow/5",
        &opts,
    )
    .unwrap();

    // Отменяем сразу после старта.
    controller.abort();

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