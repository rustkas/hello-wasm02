use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Retry succeeds after failures: /api/flaky падает 2 раза, потом 200.
#[wasm_bindgen_test]
async fn test_retry_succeeds_after_failures() {
    // Сбрасываем счётчик сервера перед тестом.
    let _ = hello_wasm02::fetch::send_request_classified(
        "http://127.0.0.1:3000/api/flaky/reset",
    )
    .await;

    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 4,
        base_delay_ms: 50,
        max_delay_ms: 500,
        jitter_factor: 0.0,
    };

    let result = hello_wasm02::fetch::send_with_retry(
        "http://127.0.0.1:3000/api/flaky",
        &config,
    )
    .await;

    assert!(result.is_ok(), "третья попытка должна быть успешной");
    assert_eq!(result.unwrap().status(), 200);
}

/// Retry gives up after max: /api/error500 всегда отвечает 500.
#[wasm_bindgen_test]
async fn test_retry_gives_up_after_max() {
    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 2,
        base_delay_ms: 30,
        max_delay_ms: 100,
        jitter_factor: 0.0,
    };

    let result = hello_wasm02::fetch::send_with_retry(
        "http://127.0.0.1:3000/api/error500",
        &config,
    )
    .await;

    assert!(result.is_err(), "все попытки должны вернуть ошибку");
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Http(500, _) => {}
        other => panic!("ожидался Http(500), получено {other:?}"),
    }
}

/// Network-ошибка retriable.
#[wasm_bindgen_test]
async fn test_is_retriable_network() {
    let err = hello_wasm02::fetch::FetchError::Network("timeout".into());
    assert!(hello_wasm02::fetch::is_retriable(&err));
}

/// 500 retriable.
#[wasm_bindgen_test]
async fn test_is_retriable_500() {
    let err = hello_wasm02::fetch::FetchError::Http(500, "".into());
    assert!(hello_wasm02::fetch::is_retriable(&err));
}

/// 404 не retriable.
#[wasm_bindgen_test]
async fn test_is_not_retriable_404() {
    let err = hello_wasm02::fetch::FetchError::Http(404, "".into());
    assert!(!hello_wasm02::fetch::is_retriable(&err));
}

/// 429 retriable.
#[wasm_bindgen_test]
async fn test_is_retriable_429() {
    let err = hello_wasm02::fetch::FetchError::Http(429, "".into());
    assert!(hello_wasm02::fetch::is_retriable(&err));
}

/// Backoff растёт экспоненциально.
#[wasm_bindgen_test]
async fn test_backoff_increases() {
    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 5,
        base_delay_ms: 100,
        max_delay_ms: 10_000,
        jitter_factor: 0.0,
    };

    let d0 = hello_wasm02::fetch::calculate_backoff(0, &config);
    let d1 = hello_wasm02::fetch::calculate_backoff(1, &config);
    let d2 = hello_wasm02::fetch::calculate_backoff(2, &config);
    let d3 = hello_wasm02::fetch::calculate_backoff(3, &config);

    assert_eq!(d0, 100);
    assert_eq!(d1, 200);
    assert_eq!(d2, 400);
    assert_eq!(d3, 800);
}

/// Backoff с jitter даёт разные значения.
#[wasm_bindgen_test]
async fn test_backoff_has_jitter() {
    let config = hello_wasm02::fetch::RetryConfig {
        max_attempts: 3,
        base_delay_ms: 1000,
        max_delay_ms: 10_000,
        jitter_factor: 0.5,
    };

    let mut values = std::collections::HashSet::new();
    for _ in 0..20 {
        values.insert(hello_wasm02::fetch::calculate_backoff(0, &config));
    }

    assert!(
        values.len() > 1,
        "jitter должен давать разные значения, получено {} уникальных",
        values.len()
    );

    for v in &values {
        assert!(*v >= 1000, "jitter не должен уменьшать задержку");
    }
}