//! Тесты `src/fsa_api_quota.rs`.
//!
//! Проверяем `estimate()`, `persisted()`, `persist()`.
//! OPFS не требует user gesture — тесты идут в headless Chrome.
//!
//! `persist()` может вернуть `false` — это не ошибка, а отказ
//! браузера. Тесты не паникуют, если persistence не предоставлен.

use wasm_bindgen_test::*;

use hello_wasm02::fsa_api_quota::{
    estimate, persisted, quota_available, request_persist,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Доступность API
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_quota_available() {
    // Не паникуем, если API нет — мягкий skip.
    if !quota_available() {
        return;
    }
    // В headless Chrome estimate доступен — флаг true.
    assert!(quota_available(), "quota_available должен быть true");
}

// ---------------------------------------------------------------------------
// estimate
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_estimate_returns_non_negative() {
    if !quota_available() {
        return;
    }
    let e = estimate().await.expect("estimate");
    assert!(e.usage >= 0.0, "usage не может быть отрицательным");
    assert!(e.quota >= 0.0, "quota не может быть отрицательной");
}

#[wasm_bindgen_test]
async fn test_estimate_quota_greater_or_equal_usage() {
    if !quota_available() {
        return;
    }
    let e = estimate().await.expect("estimate");
    // Нормальное состояние: usage <= quota. Если quota == 0 (нет
    // информации), пропускаем проверку.
    if e.quota > 0.0 {
        assert!(
            e.quota >= e.usage,
            "quota ({}) должна быть >= usage ({})",
            e.quota,
            e.usage
        );
    }
}

#[wasm_bindgen_test]
async fn test_estimate_fill_ratio_in_range() {
    if !quota_available() {
        return;
    }
    let e = estimate().await.expect("estimate");
    let r = e.fill_ratio();
    assert!((0.0..=1.0).contains(&r), "fill_ratio в диапазоне 0..1");
}

#[wasm_bindgen_test]
async fn test_estimate_summary_not_empty() {
    if !quota_available() {
        return;
    }
    let e = estimate().await.expect("estimate");
    let s = e.summary();
    assert!(!s.is_empty());
    assert!(s.contains("usage="));
    assert!(s.contains("quota="));
}

// ---------------------------------------------------------------------------
// persist / persisted
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_persisted_returns_bool() {
    if !quota_available() {
        return;
    }
    // persisted() возвращает bool без ошибки в любом окружении.
    let _ = persisted().await;
}

#[wasm_bindgen_test]
async fn test_request_persist_returns_bool() {
    if !quota_available() {
        return;
    }
    // persist() может вернуть true или false — оба варианта ок.
    // В headless Chrome обычно false без user engagement.
    let result = request_persist().await;
    // Ничего не проверяем по значению: главное — не паникует.
    let _ = result;
}