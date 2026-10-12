use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Создание клиента.
#[wasm_bindgen_test]
async fn test_api_client_new() {
    let client = hello_wasm02::api_client::ApiClient::new("https://example.com");
    assert_eq!(client.base_url(), "https://example.com");
}

/// Базовый URL нормализуется (убирается trailing slash).
#[wasm_bindgen_test]
async fn test_api_client_base_url() {
    let client = hello_wasm02::api_client::ApiClient::new("https://example.com/");
    assert_eq!(client.base_url(), "https://example.com");
}

/// GET-запрос через клиент.
#[wasm_bindgen_test]
async fn test_api_client_get() {
    let client = hello_wasm02::api_client::ApiClient::new(
        "https://jsonplaceholder.typicode.com",
    );
    let json: serde_json::Value = client.get("/posts/1").await.unwrap();
    assert_eq!(json["id"], 1);
    assert!(json["title"].is_string());
}

/// POST-запрос через клиент.
#[wasm_bindgen_test]
async fn test_api_client_post() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Body {
        title: String,
        body: String,
        #[serde(rename = "userId")]
        user_id: u32,
    }

    let client = hello_wasm02::api_client::ApiClient::new(
        "https://jsonplaceholder.typicode.com",
    );

    let body = Body {
        title: "Test".into(),
        body: "Test body".into(),
        user_id: 1,
    };

    let json: serde_json::Value = client.post("/posts", &body).await.unwrap();
    assert_eq!(json["title"], "Test");
    assert_eq!(json["userId"], 1);
}

/// Токен добавляется в заголовки.
#[wasm_bindgen_test]
async fn test_api_client_with_token() {
    // Сначала логинимся на auth-server, получаем токен.
    let store = hello_wasm02::auth::TokenStore::new();
    hello_wasm02::auth::login(&store, "user", "secret")
        .await
        .unwrap();

    let token = store.borrow().access_token.clone().unwrap();

    // Создаём клиент с этим токеном.
    let client = hello_wasm02::api_client::ApiClient::new("http://127.0.0.1:3000")
        .with_token(&token);

    // Запрос к защищённому эндпоинту.
    let json: serde_json::Value = client.get("/api/me").await.unwrap();
    assert_eq!(json["username"], "user");
}

/// Пользовательские заголовки.
#[wasm_bindgen_test]
async fn test_api_client_custom_headers() {
    let client = hello_wasm02::api_client::ApiClient::new(
        "https://jsonplaceholder.typicode.com",
    )
    .with_header("X-Custom-Header", "Rust-Wasm-Client");

    // Проверяем, что запрос всё ещё проходит (заголовок не мешает).
    let json: serde_json::Value = client.get("/posts/1").await.unwrap();
    assert_eq!(json["id"], 1);
}

/// Ошибка HTTP возвращается как FetchError::Http.
#[wasm_bindgen_test]
async fn test_api_client_error_handling() {
    let client = hello_wasm02::api_client::ApiClient::new(
        "https://jsonplaceholder.typicode.com",
    );

    let result = client
        .get::<serde_json::Value>("/posts/999999")
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Http(404, _) => {}
        other => panic!("ожидался Http(404), получено {other:?}"),
    }
}

/// Retry-конфиг применяется.
///
/// Проверяем через «флейки»-эндпоинт нашего auth-server: он падает
/// первые 2 раза, но клиент с retry должен справиться.
#[wasm_bindgen_test]
async fn test_api_client_retry() {
    // Сброс счётчика.
    let _ = hello_wasm02::fetch::send_request_classified(
        "http://127.0.0.1:3000/api/flaky/reset",
    )
    .await;

    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 5,
        base_delay_ms: 50,
        max_delay_ms: 500,
        jitter_factor: 0.0,
    };

    let client = hello_wasm02::api_client::ApiClient::new("http://127.0.0.1:3000")
        .with_retry(config);

    let json: serde_json::Value = client.get("/api/flaky").await.unwrap();
    assert!(json["message"].is_string());
}