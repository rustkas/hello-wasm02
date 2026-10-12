#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::worker_pool::{
    create_pool, is_atomics_supported, is_shared_array_buffer_supported,
    is_worker_supported, sab_status, support_summary, terminate_pool,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn worker_is_supported_in_browser() {
    assert!(is_worker_supported(), "Worker should be available");
}

#[wasm_bindgen_test]
fn shared_array_buffer_detection_does_not_panic() {
    // В большинстве dev-окружений без COOP/COEP SAB недоступен.
    // Тест проверяет только то, что detection возвращает bool.
    let _ = is_shared_array_buffer_supported();
}

#[wasm_bindgen_test]
fn atomics_detection_does_not_panic() {
    let _ = is_atomics_supported();
}

#[wasm_bindgen_test]
fn sab_status_returns_string() {
    let s = sab_status();
    assert!(!s.is_empty(), "sab_status must not be empty");
}

#[wasm_bindgen_test]
fn support_summary_returns_string() {
    let s = support_summary();
    assert!(s.contains("Worker"));
    assert!(s.contains("SAB"));
    assert!(s.contains("Atomics"));
}

#[wasm_bindgen_test]
fn create_pool_of_four() {
    let pool = create_pool("/nonexistent_worker_xyz.js", 4);
    assert!(pool.is_ok(), "pool should be creatable");
    let pool = pool.unwrap();
    assert_eq!(pool.len(), 4, "pool size must be 4");
    terminate_pool(&pool);
}

#[wasm_bindgen_test]
fn create_empty_pool() {
    let pool = create_pool("/nonexistent_worker_xyz.js", 0).unwrap();
    assert_eq!(pool.len(), 0);
}

#[wasm_bindgen_test]
fn terminate_pool_does_not_panic() {
    let pool = create_pool("/nonexistent_worker_xyz.js", 3).unwrap();
    terminate_pool(&pool);
}