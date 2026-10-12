//! Тесты с моками `fetch` через `js_sys::eval`.
//!
//! Идея: в JavaScript можно в любой момент заменить глобальную
//! функцию `fetch` на свою. Наш Rust-код вызывает
//! `window.fetch_with_request(&request)` — значит, он увидит
//! именно мок.
//!
//! **Важно про сигнатуру мока.** `fetch_with_request` вызывает
//! `fetch(request)` с ОДНИМ аргументом — объектом `Request`.
//! Второй аргумент (`init`) там отсутствует. Поэтому заголовки
//! и тело мок должен читать из самого `Request`:
//! `request.headers.get(...)`, `request.clone().text()`,
//! `request.method`.
//!
//! После каждого теста восстанавливаем оригинальный `fetch`.

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Устанавливает мок `fetch`, который возвращает заданный
/// статус и тело.
///
/// Возвращает `String` — код для восстановления оригинала.
/// Вызывающий должен сохранить его и вызвать `restore_fetch()`
/// в конце теста.
fn install_mock_fetch(status: u16, body: &str, content_type: &str) -> String {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    // Экранируем тело и content-type для вставки в JS-строку.
    let body_escaped = body
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    let ct_escaped = content_type.replace('"', "\\\"");

    let mock_code = format!(
        r#"
        window.fetch = function(request) {{
            return Promise.resolve(new Response(
                "{body_escaped}",
                {{
                    status: {status},
                    headers: {{ "Content-Type": "{ct_escaped}" }}
                }}
            ));
        }};
        "#
    );

    js_sys::eval(&mock_code).unwrap();
    "window.fetch = window.__originalFetch;".to_string()
}

/// Восстанавливает оригинальный `fetch`.
fn restore_fetch() {
    js_sys::eval("window.fetch = window.__originalFetch;").ok();
}

// ============================================================
// Базовые сценарии
// ============================================================

/// Мок возвращает 200 OK — клиент разбирает JSON.
#[wasm_bindgen_test]
async fn test_mock_200_response() {
    let _restore = install_mock_fetch(200, r#"{"id":1,"name":"Alice"}"#, "application/json");

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com");
    let json: serde_json::Value = client.get("/users/1").await.unwrap();

    assert_eq!(json["id"], 1);
    assert_eq!(json["name"], "Alice");

    restore_fetch();
}

/// Мок возвращает 404 — клиент возвращает `Http(404)`.
#[wasm_bindgen_test]
async fn test_mock_404_response() {
    let _restore = install_mock_fetch(404, "Not Found", "text/plain");

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com");
    let result = client.get::<serde_json::Value>("/users/999").await;

    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Http(404, body) => {
            assert_eq!(body, "Not Found");
        }
        other => panic!("ожидался Http(404), получено {other:?}"),
    }

    restore_fetch();
}

/// Мок возвращает 401 — клиент возвращает `Http(401)`.
#[wasm_bindgen_test]
async fn test_mock_401_response() {
    let _restore = install_mock_fetch(401, "Unauthorized", "text/plain");

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com");
    let result = client.get::<serde_json::Value>("/protected").await;

    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Http(401, _) => {}
        other => panic!("ожидался Http(401), получено {other:?}"),
    }

    restore_fetch();
}

/// Мок возвращает пустое тело — клиент получает `Parse`.
#[wasm_bindgen_test]
async fn test_mock_empty_body() {
    let _restore = install_mock_fetch(200, "", "application/json");

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com");
    let result = client.get::<serde_json::Value>("/empty").await;

    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Parse(_) => {}
        other => panic!("ожидался Parse, получено {other:?}"),
    }

    restore_fetch();
}

// ============================================================
// Retry
// ============================================================

/// Retry повторяет запрос при 500. Три попытки, на третьей — успех.
#[wasm_bindgen_test]
async fn test_mock_retry_then_success() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    // Счётчик вызовов храним в window.__callCount.
    let mock = r#"
        window.__callCount = 0;
        window.fetch = function(request) {
            window.__callCount++;
            if (window.__callCount < 3) {
                return Promise.resolve(new Response(
                    "Server Error",
                    { status: 500, headers: { "Content-Type": "text/plain" } }
                ));
            }
            return Promise.resolve(new Response(
                '{"ok":true}',
                { status: 200, headers: { "Content-Type": "application/json" } }
            ));
        };
    "#;
    js_sys::eval(mock).unwrap();

    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 5,
        base_delay_ms: 10,
        max_delay_ms: 50,
        jitter_factor: 0.0,
    };

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com")
        .with_retry(config);

    let json: serde_json::Value = client.get("/retry").await.unwrap();
    assert_eq!(json["ok"], true);

    let calls = js_sys::eval("window.__callCount").unwrap();
    assert_eq!(calls.as_f64().unwrap() as u32, 3);

    restore_fetch();
}

/// 404 не повторяется — ровно одна попытка.
#[wasm_bindgen_test]
async fn test_mock_404_not_retried() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    let mock = r#"
        window.__callCount = 0;
        window.fetch = function(request) {
            window.__callCount++;
            return Promise.resolve(new Response(
                "Not Found",
                { status: 404, headers: { "Content-Type": "text/plain" } }
            ));
        };
    "#;
    js_sys::eval(mock).unwrap();

    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 5,
        base_delay_ms: 10,
        max_delay_ms: 50,
        jitter_factor: 0.0,
    };

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com")
        .with_retry(config);

    let _ = client.get::<serde_json::Value>("/missing").await;

    let calls = js_sys::eval("window.__callCount").unwrap();
    assert_eq!(calls.as_f64().unwrap() as u32, 1);

    restore_fetch();
}

/// Сетевой сбой классифицируется как `Network`.
#[wasm_bindgen_test]
async fn test_mock_network_failure() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    let mock = r#"
        window.fetch = function(request) {
            return Promise.reject(new TypeError("Failed to fetch"));
        };
    "#;
    js_sys::eval(mock).unwrap();

    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 1,
        base_delay_ms: 10,
        max_delay_ms: 50,
        jitter_factor: 0.0,
    };

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com")
        .with_retry(config);

    let result = client.get::<serde_json::Value>("/data").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Network(_) => {}
        other => panic!("ожидался Network, получено {other:?}"),
    }

    restore_fetch();
}

// ============================================================
// Кэш
// ============================================================

/// Кэш: второй запрос к тому же URL не идёт в сеть.
#[wasm_bindgen_test]
async fn test_mock_cache_hits() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    let mock = r#"
        window.__callCount = 0;
        window.fetch = function(request) {
            window.__callCount++;
            return Promise.resolve(new Response(
                '{"cached":true}',
                { status: 200, headers: { "Content-Type": "application/json" } }
            ));
        };
    "#;
    js_sys::eval(mock).unwrap();

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com")
        .with_cache(60_000);

    let _: serde_json::Value = client.get("/data").await.unwrap();
    let _: serde_json::Value = client.get("/data").await.unwrap();

    let calls = js_sys::eval("window.__callCount").unwrap();
    assert_eq!(calls.as_f64().unwrap() as u32, 1, "должен быть один сетевой вызов");

    restore_fetch();
}

// ============================================================
// Заголовки
// ============================================================

/// Клиент добавляет заголовок Authorization.
///
/// `fetch_with_request` вызывает `fetch(request)` — один аргумент,
/// объект Request. Заголовки читаем из `request.headers`.
#[wasm_bindgen_test]
async fn test_mock_auth_header_sent() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    let mock = r#"
        window.__lastAuthHeader = null;
        window.fetch = function(request) {
            // request — это Request-объект, а не URL.
            // У него есть headers, method, body.
            try {
                window.__lastAuthHeader = request.headers.get("Authorization");
            } catch (e) {
                window.__lastAuthHeader = "ERROR: " + String(e);
            }
            return Promise.resolve(new Response(
                '{"ok":true}',
                { status: 200, headers: { "Content-Type": "application/json" } }
            ));
        };
    "#;
    js_sys::eval(mock).unwrap();

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com")
        .with_token("test-token-12345");

    let _: serde_json::Value = client.get("/me").await.unwrap();

    let header = js_sys::eval("window.__lastAuthHeader").unwrap();
    assert_eq!(header.as_string().as_deref(), Some("Bearer test-token-12345"));

    restore_fetch();
}

/// Клиент добавляет кастомный заголовок.
#[wasm_bindgen_test]
async fn test_mock_custom_header_sent() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    let mock = r#"
        window.__lastCustomHeader = null;
        window.fetch = function(request) {
            try {
                window.__lastCustomHeader = request.headers.get("X-Custom-Header");
            } catch (e) {
                window.__lastCustomHeader = "ERROR: " + String(e);
            }
            return Promise.resolve(new Response(
                '{}',
                { status: 200, headers: { "Content-Type": "application/json" } }
            ));
        };
    "#;
    js_sys::eval(mock).unwrap();

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com")
        .with_header("X-Custom-Header", "Rust-Wasm-Client");

    let _: serde_json::Value = client.get("/data").await.unwrap();

    let header = js_sys::eval("window.__lastCustomHeader").unwrap();
    assert_eq!(header.as_string().as_deref(), Some("Rust-Wasm-Client"));

    restore_fetch();
}

/// POST отправляет JSON-тело.
///
/// Тело в `Request` читается через `request.clone().text()`:
/// `request.body` — это `ReadableStream`, а не строка. После
/// первого чтения `Request.bodyUsed = true`, поэтому клонируем.
#[wasm_bindgen_test]
async fn test_mock_post_body_sent() {
    js_sys::eval("window.__originalFetch = window.fetch;").unwrap();

    let mock = r#"
        window.__lastBody = null;
        window.__lastMethod = null;
        window.fetch = async function(request) {
            window.__lastMethod = request.method;
            try {
                window.__lastBody = await request.clone().text();
            } catch (e) {
                window.__lastBody = "ERROR: " + String(e);
            }
            return new Response(
                '{"id":42}',
                { status: 201, headers: { "Content-Type": "application/json" } }
            );
        };
    "#;
    js_sys::eval(mock).unwrap();

    #[derive(serde::Serialize)]
    struct NewUser {
        name: String,
    }

    let client = hello_wasm02::api_client::ApiClient::new("https://mock.example.com");
    let body = NewUser { name: "Alice".into() };
    let json: serde_json::Value = client.post("/users", &body).await.unwrap();

    assert_eq!(json["id"], 42);

    let method = js_sys::eval("window.__lastMethod").unwrap();
    assert_eq!(method.as_string().as_deref(), Some("POST"));

    let sent_body = js_sys::eval("window.__lastBody").unwrap();
    let sent_str = sent_body.as_string().unwrap_or_default();
    assert!(
        sent_str.contains("Alice"),
        "тело запроса должно содержать Alice, получено: {sent_str}"
    );

    restore_fetch();
}