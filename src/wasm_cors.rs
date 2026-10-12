use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// CORS-запрос к JSONPlaceholder проходит.
#[wasm_bindgen_test]
async fn test_cors_request_succeeds() {
    let url = "https://jsonplaceholder.typicode.com/posts/1";
    let request = hello_wasm02::fetch::make_cors_request(url).unwrap();
    let resp = hello_wasm02::fetch::send_prepared_request(&request)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

/// Cross-origin без CORS-заголовков отклоняется браузером.
#[wasm_bindgen_test]
async fn test_cross_origin_without_cors_fails() {
    let url = "https://example.com/";
    let request = hello_wasm02::fetch::make_cors_request(url).unwrap();
    let result = hello_wasm02::fetch::send_prepared_request(&request).await;
    assert!(result.is_err(), "example.com не отправляет CORS-заголовки");
}

/// Same-origin режим работает для относительных URL.
#[wasm_bindgen_test]
async fn test_same_origin_mode() {
    // Относительный URL — тот же origin, что и страница.
    // Vite-прокси перенаправит его на auth-server, но для браузера
    // это останется same-origin.
    let url = "/api/flaky/reset";
    let request = hello_wasm02::fetch::make_same_origin_request(url).unwrap();

    // Запрос дойдёт до сервера, потому что CORS не участвует.
    let result = hello_wasm02::fetch::send_prepared_request(&request).await;
    // Сервер ответит 200 — счётчик сброшен.
    assert!(result.is_ok());
}

/// Режим запроса устанавливается корректно.
#[wasm_bindgen_test]
async fn test_request_mode_set() {
    let url = "https://jsonplaceholder.typicode.com/posts/1";
    let request = hello_wasm02::fetch::make_cors_request(url).unwrap();
    assert_eq!(request.mode(), "cors");

    let same = hello_wasm02::fetch::make_same_origin_request("/").unwrap();
    assert_eq!(same.mode(), "same-origin");
}