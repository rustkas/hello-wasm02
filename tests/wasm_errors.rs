use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Сетевая ошибка: несуществующий домен → FetchError::Network.
#[wasm_bindgen_test]
async fn test_classify_network_error() {
    let url = "https://nonexistent-domain-xyz-42.invalid/api/test";
    let result = hello_wasm02::fetch::send_request_classified(url).await;
    assert!(result.is_err(), "должна быть ошибка");

    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Network(_) => {}
        other => panic!("ожидался Network, получено {other:?}"),
    }
}

/// HTTP 404: ресурс не найден → FetchError::Http(404, _).
#[wasm_bindgen_test]
async fn test_classify_http_404() {
    let url = "https://jsonplaceholder.typicode.com/posts/999999";
    let result = hello_wasm02::fetch::send_request_classified(url).await;
    assert!(result.is_err());

    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Http(404, _) => {}
        other => panic!("ожидался Http(404), получено {other:?}"),
    }
}

/// HTTP 500: ошибка сервера → FetchError::Http(500, _).
#[wasm_bindgen_test]
async fn test_classify_http_500() {
    let url = "http://127.0.0.1:3000/api/error500";
    let result = hello_wasm02::fetch::send_request_classified(url).await;
    assert!(result.is_err());

    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Http(500, _) => {}
        other => panic!("ожидался Http(500), получено {other:?}"),
    }
}

/// AbortError: отмена через AbortController → FetchError::Aborted.
#[wasm_bindgen_test]
async fn test_classify_abort_error() {
    let controller = web_sys::AbortController::new().unwrap();
    let signal = controller.signal();

    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_signal(Some(&signal));

    let request = web_sys::Request::new_with_str_and_init(
        "https://jsonplaceholder.typicode.com/posts/1",
        &opts,
    )
    .unwrap();

    controller.abort();

    let window = web_sys::window().unwrap();
    let result = wasm_bindgen_futures::JsFuture::from(
        window.fetch_with_request(&request),
    )
    .await;

    assert!(result.is_err());
    let classified = hello_wasm02::fetch::classify_error(result.unwrap_err());
    match classified {
        hello_wasm02::fetch::FetchError::Aborted => {}
        other => panic!("ожидался Aborted, получено {other:?}"),
    }
}

/// Ошибка парсинга → FetchError::Parse.
///
/// Проверяем вручную, потому что `send_request_classified` не парсит тело.
#[wasm_bindgen_test]
async fn test_classify_parse_error() {
    let err = hello_wasm02::fetch::FetchError::Parse("bad json".to_string());
    match err {
        hello_wasm02::fetch::FetchError::Parse(msg) => {
            assert_eq!(msg, "bad json");
        }
        _ => panic!("ожидался Parse"),
    }
}

/// Проверяет текст сообщения для HTTP-ошибки.
#[wasm_bindgen_test]
async fn test_http_error_message() {
    let err = hello_wasm02::fetch::FetchError::Http(500, "".to_string());
    let msg = err.user_message();
    assert!(msg.contains("500"), "сообщение должно содержать статус");
    assert!(err.is_retryable(), "5xx должен быть retryable");

    let err404 = hello_wasm02::fetch::FetchError::Http(404, "".to_string());
    assert!(!err404.is_retryable(), "404 не должен быть retryable");
}

/// Сетевая ошибка не должна содержать `Response` — только сообщение.
#[wasm_bindgen_test]
async fn test_network_error_no_response() {
    let err = hello_wasm02::fetch::FetchError::Network("timeout".to_string());

    // Сначала проверяем то, что нужно по ссылке.
    assert!(err.is_retryable(), "сетевая ошибка должна быть retryable");

    // И только потом разбираем enum, перемещая владение.
    match err {
        hello_wasm02::fetch::FetchError::Network(msg) => assert_eq!(msg, "timeout"),
        _ => panic!("ожидался Network"),
    }
}

/// Успешный запрос не должен возвращать ошибку.
#[wasm_bindgen_test]
async fn test_ok_response_no_error() {
    let url = "https://jsonplaceholder.typicode.com/posts/1";
    let result = hello_wasm02::fetch::send_request_classified(url).await;
    assert!(result.is_ok(), "успешный запрос не должен возвращать ошибку");
    assert_eq!(result.unwrap().status(), 200);
}