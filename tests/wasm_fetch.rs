//! Тесты для Fetch API: базовые GET-запросы.
//!
//! Покрытие:
//! - Создание Request через RequestInit.
//! - Отправка запроса, чтение Response (статус, ok, url).
//! - GET JSON и GET text.
//! - Механика Promise → JsFuture.
use wasm_bindgen_test::*;

use hello_wasm02::fetch;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// Создание запросов
// ============================================================

#[wasm_bindgen_test]
fn test_make_get_request() {
    let request = fetch::make_get_request(
        "https://jsonplaceholder.typicode.com/posts/1",
        web_sys::RequestMode::Cors,
    );
    assert!(request.is_ok(), "GET request should be created");
}

// ============================================================
// GET-запросы
// ============================================================

#[wasm_bindgen_test]
async fn test_get_json_single_post() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct Post {
        id: u32,
        #[serde(default)]
        title: String,
    }

    let result = fetch::get_json::<Post>(
        "https://jsonplaceholder.typicode.com/posts/1",
    )
    .await;

    assert!(result.is_ok(), "GET /posts/1 should succeed: {:?}", result);
    let post = result.unwrap();
    assert_eq!(post.id, 1);
    assert!(!post.title.is_empty());
}

#[wasm_bindgen_test]
async fn test_get_text() {
    let result =
        fetch::get_text("https://jsonplaceholder.typicode.com/posts/1").await;

    assert!(result.is_ok(), "GET text should succeed: {:?}", result);
    let text = result.unwrap();
    assert!(text.contains("userId"));
    assert!(text.contains("title"));
}

// ============================================================
// Response: свойства
// ============================================================

#[wasm_bindgen_test]
async fn test_response_status_and_ok() {
    let resp = fetch::get("https://jsonplaceholder.typicode.com/posts/1")
        .await
        .expect("GET should succeed");

    assert_eq!(fetch::response_status(&resp), 200);
    assert!(fetch::response_is_ok(&resp));
}

#[wasm_bindgen_test]
async fn test_response_url() {
    let resp = fetch::get("https://jsonplaceholder.typicode.com/posts/1")
        .await
        .expect("GET should succeed");

    let url = fetch::response_url(&resp);
    assert!(
        url.contains("jsonplaceholder.typicode.com"),
        "URL should contain host: {}",
        url
    );
    assert!(
        url.contains("posts/1"),
        "URL should contain path: {}",
        url
    );
}

// ============================================================
// Механика Promise → JsFuture
// ============================================================

#[wasm_bindgen_test]
async fn test_jsfuture_promise_resolution() {
    use wasm_bindgen_futures::JsFuture;

    // Создаём Promise, который разрешается значением 42
    let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::from(42));

    // Превращаем в Future и ждём
    let value = JsFuture::from(promise)
        .await
        .expect("Promise should resolve");

    assert_eq!(value.as_f64(), Some(42.0));
}

// ============================================================
// HTTP-методы: создание запросов
// ============================================================

#[wasm_bindgen_test]
fn test_make_post_json_request() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Data {
        title: String,
    }

    let data = Data {
        title: "test".into(),
    };

    let request = fetch::make_post_json_request(
        "https://jsonplaceholder.typicode.com/posts",
        &data,
        web_sys::RequestMode::Cors,
    );
    assert!(request.is_ok(), "POST JSON request should be created");
}

#[wasm_bindgen_test]
fn test_make_post_form_urlencoded_request() {
    let request = fetch::make_post_form_urlencoded_request(
        "https://jsonplaceholder.typicode.com/posts",
        &[("title", "test"), ("body", "content")],
        web_sys::RequestMode::Cors,
    );
    assert!(
        request.is_ok(),
        "POST form-urlencoded request should be created"
    );
}

#[wasm_bindgen_test]
fn test_make_post_form_data_request() {
    let request = fetch::make_post_form_data_request(
        "https://jsonplaceholder.typicode.com/posts",
        &[("title", "test"), ("body", "content")],
        web_sys::RequestMode::Cors,
    );
    assert!(request.is_ok(), "POST FormData request should be created");
}

#[wasm_bindgen_test]
fn test_make_put_request() {
    let request = fetch::make_put_request(
        "https://jsonplaceholder.typicode.com/posts/1",
        web_sys::RequestMode::Cors,
    );
    assert!(request.is_ok(), "PUT request should be created");
}

#[wasm_bindgen_test]
fn test_make_delete_request() {
    let request = fetch::make_delete_request(
        "https://jsonplaceholder.typicode.com/posts/1",
        web_sys::RequestMode::Cors,
    );
    assert!(request.is_ok(), "DELETE request should be created");
}

// ============================================================
// HTTP-методы: реальные запросы
// ============================================================

#[wasm_bindgen_test]
async fn test_post_json() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    struct Post {
        #[serde(default)]
        id: u32,
        title: String,
        body: String,
        #[serde(default)]
        user_id: u32,
    }

    let new_post = Post {
        id: 0,
        title: "Test".into(),
        body: "Body".into(),
        user_id: 1,
    };

    let result = fetch::post_json_response::<Post, Post>(
        "https://jsonplaceholder.typicode.com/posts",
        &new_post,
    )
    .await;

    assert!(result.is_ok(), "POST should succeed: {:?}", result);
    let created = result.unwrap();
    assert_eq!(created.title, "Test");
    assert_eq!(created.body, "Body");
}

#[wasm_bindgen_test]
async fn test_put_json() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Post {
        id: u32,
        title: String,
        body: String,
        user_id: u32,
    }

    let updated = Post {
        id: 1,
        title: "Updated".into(),
        body: "New body".into(),
        user_id: 1,
    };

    let result = fetch::put_json(
        "https://jsonplaceholder.typicode.com/posts/1",
        &updated,
    )
    .await;

    assert!(result.is_ok(), "PUT should succeed: {:?}", result);
    let resp = result.unwrap();
    assert!(fetch::response_is_ok(&resp));
}

#[wasm_bindgen_test]
async fn test_delete() {
    let result =
        fetch::delete_request("https://jsonplaceholder.typicode.com/posts/1")
            .await;

    assert!(result.is_ok(), "DELETE should succeed: {:?}", result);
    let resp = result.unwrap();
    assert!(fetch::response_is_ok(&resp));
}

// ============================================================
// Content-Type заголовки
// ============================================================

#[wasm_bindgen_test]
fn test_content_type_is_set_for_json() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Data {
        value: u32,
    }

    let data = Data { value: 42 };

    let request = fetch::make_post_json_request(
        "https://jsonplaceholder.typicode.com/posts",
        &data,
        web_sys::RequestMode::Cors,
    )
    .expect("request should be created");

    let content_type = request
        .headers()
        .get("Content-Type")
        .expect("header read should not fail")
        .expect("Content-Type should be set");

    assert!(
        content_type.contains("application/json"),
        "Content-Type should be application/json, got: {}",
        content_type
    );
}

// ============================================================
// Headers: создание и установка
// ============================================================

#[wasm_bindgen_test]
fn test_make_headers() {
    let headers = fetch::make_headers(&[
        ("Accept", "application/json"),
        ("X-Custom", "test-value"),
    ]);

    assert!(headers.is_ok(), "Headers should be created");
    let h = headers.unwrap();

    let accept = h.get("Accept").unwrap().unwrap();
    assert_eq!(accept, "application/json");

    let custom = h.get("X-Custom").unwrap().unwrap();
    assert_eq!(custom, "test-value");
}

#[wasm_bindgen_test]
fn test_headers_case_insensitive() {
    let headers = fetch::make_headers(&[
        ("Content-Type", "application/json"),
    ])
    .unwrap();

    // Разные регистры — один и тот же заголовок
    assert_eq!(
        headers.get("Content-Type").unwrap().unwrap(),
        "application/json"
    );
    assert_eq!(
        headers.get("content-type").unwrap().unwrap(),
        "application/json"
    );
    assert_eq!(
        headers.get("CONTENT-TYPE").unwrap().unwrap(),
        "application/json"
    );
    assert_eq!(
        headers.get("CoNtEnT-TyPe").unwrap().unwrap(),
        "application/json"
    );
}

// ============================================================
// Headers: реальные запросы
// ============================================================

#[wasm_bindgen_test]
async fn test_request_with_custom_headers() {
    let result = fetch::get_with_headers(
        "https://jsonplaceholder.typicode.com/posts/1",
        &[
            ("Accept", "application/json"),
            ("X-Custom-Header", "test-value"),
        ],
    )
    .await;

    assert!(result.is_ok(), "GET with headers should succeed");
    let resp = result.unwrap();
    assert!(fetch::response_is_ok(&resp));
    assert_eq!(fetch::response_status(&resp), 200);
}

#[wasm_bindgen_test]
async fn test_response_headers_content_type() {
    let resp = fetch::get("https://jsonplaceholder.typicode.com/posts/1")
        .await
        .expect("GET should succeed");

    let content_type = fetch::get_response_header(&resp, "content-type");
    assert!(content_type.is_some(), "Content-Type should be present");
    assert!(
        content_type.unwrap().contains("application/json"),
        "Content-Type should be application/json"
    );

    // Case-insensitivity при чтении
    let ct_upper = fetch::get_response_header(&resp, "CONTENT-TYPE");
    assert!(ct_upper.is_some(), "CONTENT-TYPE should also work");
}

#[wasm_bindgen_test]
async fn test_response_headers_cache_control() {
    let resp = fetch::get("https://jsonplaceholder.typicode.com/posts/1")
        .await
        .expect("GET should succeed");

    // Cache-Control — CORS-safelisted, должен быть доступен
    let cache_control = fetch::get_response_header(&resp, "cache-control");
    assert!(
        cache_control.is_some(),
        "Cache-Control should be present for CORS request"
    );
}

#[wasm_bindgen_test]
async fn test_response_headers_has() {
    let resp = fetch::get("https://jsonplaceholder.typicode.com/posts/1")
        .await
        .expect("GET should succeed");

    assert!(
        fetch::has_response_header(&resp, "content-type"),
        "Content-Type should exist"
    );
    assert!(
        fetch::has_response_header(&resp, "CONTENT-TYPE"),
        "Case-insensitive check should also work"
    );
    assert!(
        !fetch::has_response_header(&resp, "x-nonexistent-header"),
        "Non-existent header should return false"
    );
}