//! Тесты для асинхронных помощников: sleep, timeout, spawn.
use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

use hello_wasm02::async_utils;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// delay_ms и sleep
// ============================================================

#[wasm_bindgen_test]
async fn test_delay_ms_resolves() {
    let promise = async_utils::delay_ms(10);
    let result = JsFuture::from(promise).await;

    assert!(result.is_ok());
}

#[wasm_bindgen_test]
async fn test_sleep_completes() {
    let result = async_utils::sleep(10).await;
    assert!(result.is_ok());
}

// ============================================================
// await_promise
// ============================================================

#[wasm_bindgen_test]
async fn test_await_promise_resolve() {
    let promise = async_utils::resolved(&JsValue::from_f64(42.0));
    let result = async_utils::await_promise(promise).await.unwrap();

    assert_eq!(result.as_f64(), Some(42.0));
}

#[wasm_bindgen_test]
async fn test_await_promise_reject() {
    let promise = async_utils::rejected(&JsValue::from_str("error"));
    let result = async_utils::await_promise(promise).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().as_string(), Some("error".into()));
}

// ============================================================
// promise_race
// ============================================================

#[wasm_bindgen_test]
async fn test_promise_race_first_wins() {
    let fast = async_utils::resolved(&JsValue::from_str("fast"));
    let slow = async_utils::delay_ms(1000).into();

    let promises = js_sys::Array::new();
    promises.push(&fast);
    promises.push(&slow);

    let race = async_utils::promise_race(&promises);
    let result = JsFuture::from(race).await.unwrap();

    assert_eq!(result.as_string(), Some("fast".into()));
}

// ============================================================
// with_timeout
// ============================================================

#[wasm_bindgen_test]
async fn test_with_timeout_completes() {
    // Promise разрешается быстро (10 мс), таймаут 500 мс
    let promise = async_utils::delay_ms(10);
    let result = async_utils::with_timeout(promise, 500).await;

    assert!(result.is_ok());
}

#[wasm_bindgen_test]
async fn test_with_timeout_fires() {
    // Promise разрешается через 500 мс, таймаут 10 мс
    let promise = async_utils::delay_ms(500);
    let result = async_utils::with_timeout(promise, 10).await;

    assert!(result.is_err());
}

// ============================================================
// spawn и spawn_logged
// ============================================================

#[wasm_bindgen_test]
async fn test_spawn_runs() {
    let flag = Rc::new(RefCell::new(false));
    let flag_clone = flag.clone();

    async_utils::spawn(async move {
        async_utils::sleep(10).await.ok();
        *flag_clone.borrow_mut() = true;
    });

    // Ждём, пока флаг установится
    async_utils::sleep(50).await.ok();

    assert!(*flag.borrow(), "spawned task did not run");
}

#[wasm_bindgen_test]
async fn test_spawn_logged_catches_error() {
    let called = Rc::new(RefCell::new(false));
    let called_clone = called.clone();

    async_utils::spawn_logged(async move {
        async_utils::sleep(10).await.ok();
        *called_clone.borrow_mut() = true;
        Err::<(), String>("ignored".into())
    });

    async_utils::sleep(50).await.ok();

    assert!(*called.borrow(), "spawned task did not run");
}

// ============================================================
// Последовательность
// ============================================================

#[wasm_bindgen_test]
async fn test_sequential_sleeps() {
    let start = js_sys::Date::now();

    async_utils::sleep(20).await.ok();
    async_utils::sleep(20).await.ok();

    let elapsed = js_sys::Date::now() - start;

    // Ожидаем минимум 40 мс, но не больше 200 мс
    // (с запасом на медленные CI)
    assert!(elapsed >= 35.0, "elapsed too small: {}", elapsed);
    assert!(elapsed < 200.0, "elapsed too large: {}", elapsed);
}