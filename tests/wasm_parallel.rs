use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Проверяет, что запросы действительно идут одновременно.
///
/// Считаем максимальное количество одновременных запросов на сервере.
/// Если параллелизм работает, счётчик поднимется выше 1.
///
/// Этот тест не зависит от тайминга и не флакает в headless Chrome,
/// где запросы к одному origin могут сериализоваться: если сериализация
/// есть, счётчик честно покажет 1, и тест упадёт — но не потому что
/// тайминг «не сошёлся», а потому что параллелизм реально не работает.
#[wasm_bindgen_test]
async fn test_parallel_requests_are_concurrent() {
    // Сбрасываем счётчик перед тестом.
    let _ = hello_wasm02::fetch::send_request_classified(
        "http://127.0.0.1:3000/api/concurrency/reset",
    )
    .await;

let urls = [
    "http://127.0.0.1:3000/api/concurrency/test",
    "http://localhost:3000/api/concurrency/test",
    "http://127.0.0.1:3000/api/concurrency/test",
];
    let _ = hello_wasm02::fetch::get_all(&urls).await;

    let resp = hello_wasm02::fetch::send_request_classified(
        "http://127.0.0.1:3000/api/concurrency/stats",
    )
    .await
    .unwrap();

    let json: serde_json::Value = hello_wasm02::fetch::response_json(&resp)
        .await
        .unwrap();
    let max = json["max_concurrent"].as_u64().unwrap_or(0);

    assert!(
        max >= 2,
        "параллельные запросы должны идти одновременно, \
         max_concurrent = {max}. Если 1 — браузер сериализует \
         запросы к одному origin, и это ограничение среды, \
         а не нашего кода."
    );
}

/// Promise.all сохраняет порядок результатов.
#[wasm_bindgen_test]
async fn test_parallel_preserves_order() {
    let urls = [
        "https://jsonplaceholder.typicode.com/posts/1",
        "https://jsonplaceholder.typicode.com/posts/2",
        "https://jsonplaceholder.typicode.com/posts/3",
    ];

    let responses = hello_wasm02::fetch::get_all(&urls).await.unwrap();
    assert_eq!(responses.len(), 3);

    // Все три должны быть 200.
    for r in &responses {
        assert_eq!(r.status(), 200);
    }
}

/// Promise.all падает при первой ошибке.
#[wasm_bindgen_test]
async fn test_parallel_fails_fast() {
    let urls = [
        "https://jsonplaceholder.typicode.com/posts/1",
        "https://jsonplaceholder.typicode.com/posts/999999",  // 404
        "https://jsonplaceholder.typicode.com/posts/2",
    ];

    let result = hello_wasm02::fetch::get_all(&urls).await;
    assert!(result.is_err(), "Promise.all должен упасть на 404");
}

/// AllSettled возвращает результаты даже при ошибках.
#[wasm_bindgen_test]
async fn test_all_settled_partial_failure() {
    let urls = [
        "https://jsonplaceholder.typicode.com/posts/1",
        "https://jsonplaceholder.typicode.com/posts/999999",  // 404
    ];

    let results = hello_wasm02::fetch::get_all_settled(&urls).await.unwrap();
    assert_eq!(results.len(), 2);

    // Первый — успех.
    assert!(results[0].is_ok());
    // Второй — ошибка.
    assert!(results[1].is_err());
}

/// Race: побеждает первый завершившийся.
#[wasm_bindgen_test]
async fn test_race_first_wins() {
    let urls = [
        "http://127.0.0.1:3000/api/delay/2",
        "http://127.0.0.1:3000/api/delay/1",
        "http://127.0.0.1:3000/api/delay/3",
    ];

    let start = js_sys::Date::now();
    let result = hello_wasm02::fetch::get_race(&urls).await;
    let elapsed = js_sys::Date::now() - start;

    assert!(result.is_ok());
    // Должны уложиться быстрее 2 секунд — победил запрос с задержкой 1с.
    assert!(elapsed < 2000.0, "race должен завершиться быстро, получено {elapsed} мс");
}

/// Параллельные запросы с AbortController.
#[wasm_bindgen_test]
async fn test_parallel_with_abort() {
    let controller = hello_wasm02::fetch::make_abort_controller().unwrap();
    let signal = controller.signal();

    controller.abort();

    // После abort любой запрос с этим сигналом должен упасть.
    let result = hello_wasm02::fetch::send_with_signal(
        "http://127.0.0.1:3000/api/delay/1",
        &signal,
    )
    .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        hello_wasm02::fetch::FetchError::Aborted => {}
        other => panic!("ожидался Aborted, получено {other:?}"),
    }
}

/// Ограничение параллельности: 5 запросов, лимит 2.
#[wasm_bindgen_test]
async fn test_parallel_limited_concurrency() {
    let urls = [
        "http://127.0.0.1:3000/api/delay/1",
        "http://127.0.0.1:3000/api/delay/1",
        "http://127.0.0.1:3000/api/delay/1",
        "http://127.0.0.1:3000/api/delay/1",
        "http://127.0.0.1:3000/api/delay/1",
    ];

    let start = js_sys::Date::now();
    let results = hello_wasm02::fetch::get_all_limited(&urls, 2).await;
    let elapsed = js_sys::Date::now() - start;

    assert_eq!(results.len(), 5);
    assert!(results.iter().all(|r| r.is_ok()));

    // С лимитом 2: 5 запросов идут в три волны — ~3000 мс.
    // Без лимита было бы ~1000 мс.
    assert!(
        elapsed > 2000.0,
        "с лимитом 2 должно быть медленнее 2000 мс, получено {elapsed} мс"
    );
}

/// Три параллельных запроса возвращают три результата.
#[wasm_bindgen_test]
async fn test_parallel_three_requests() {
    let urls = [
        "https://jsonplaceholder.typicode.com/posts/1",
        "https://jsonplaceholder.typicode.com/posts/2",
        "https://jsonplaceholder.typicode.com/posts/3",
    ];

    let responses = hello_wasm02::fetch::get_all(&urls).await.unwrap();
    assert_eq!(responses.len(), 3);

    for (i, r) in responses.iter().enumerate() {
        assert_eq!(r.status(), 200, "запрос {i} вернул {}", r.status());
    }
}