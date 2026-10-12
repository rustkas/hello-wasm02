//! 48. Тесты panic-guard: prevention, classification, recovery.
//!
//! ## Что тестируется
//!
//! - `safe_*` функции возвращают `Err` там, где обычный Rust паникует.
//! - `classify_panic_msg` правильно категоризирует сообщения.
//! - `save_state` / `load_state` / `clear_state` — round-trip через localStorage.
//! - `install_panic_hook` — идемпотентна.
//!
//! ## Что НЕ тестируется
//!
//! Реальная паника. С `panic = "abort"` она завершает WASM-модуль,
//! и тест не сможет продолжить. Демонстрация паники — в `app.rs`
//! через Worker.

use wasm_bindgen_test::*;

use hello_wasm02::panic_guard::{
    classify_panic_msg, clear_state, install_panic_hook, load_state,
    safe_at_u32, safe_divide, safe_parse_int, safe_unwrap_or_err, save_state,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Prevention — safe_divide
// =============================================================================

#[wasm_bindgen_test]
fn test_safe_divide_ok() {
    assert_eq!(safe_divide(10.0, 2.0), Ok(5.0));
    assert_eq!(safe_divide(-9.0, 3.0), Ok(-3.0));
}

#[wasm_bindgen_test]
fn test_safe_divide_by_zero() {
    let r = safe_divide(10.0, 0.0);
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("ноль"));
}

#[wasm_bindgen_test]
fn test_safe_divide_nan() {
    assert!(safe_divide(f64::NAN, 1.0).is_err());
    assert!(safe_divide(1.0, f64::INFINITY).is_err());
}

// =============================================================================
// Prevention — safe_at_u32
// =============================================================================

#[wasm_bindgen_test]
fn test_safe_at_ok() {
    let data = [10u32, 20, 30];
    assert_eq!(safe_at_u32(&data, 0), Ok(10));
    assert_eq!(safe_at_u32(&data, 2), Ok(30));
}

#[wasm_bindgen_test]
fn test_safe_at_out_of_bounds() {
    let data = [10u32, 20, 30];
    let r = safe_at_u32(&data, 5);
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("вне диапазона"));
}

#[wasm_bindgen_test]
fn test_safe_at_empty() {
    let data: [u32; 0] = [];
    assert!(safe_at_u32(&data, 0).is_err());
}

// =============================================================================
// Prevention — safe_parse_int
// =============================================================================

#[wasm_bindgen_test]
fn test_safe_parse_ok() {
    assert_eq!(safe_parse_int("42"), Ok(42));
    assert_eq!(safe_parse_int("  -17  "), Ok(-17));
}

#[wasm_bindgen_test]
fn test_safe_parse_err() {
    assert!(safe_parse_int("abc").is_err());
    assert!(safe_parse_int("").is_err());
    assert!(safe_parse_int("42.5").is_err());
}

// =============================================================================
// Prevention — safe_unwrap_or_err
// =============================================================================

#[wasm_bindgen_test]
fn test_safe_unwrap_ok() {
    assert_eq!(safe_unwrap_or_err(Some(7), "x"), Ok(7));
}

#[wasm_bindgen_test]
fn test_safe_unwrap_err() {
    let r: Result<i32, String> = safe_unwrap_or_err(None, "items");
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("items"));
}

// =============================================================================
// Classification
// =============================================================================

#[wasm_bindgen_test]
fn test_classify_index_out_of_bounds() {
    assert_eq!(
        classify_panic_msg("index out of bounds: the len is 3 but the index is 5"),
        "IndexOutOfBounds"
    );
}

#[wasm_bindgen_test]
fn test_classify_divide_by_zero() {
    assert_eq!(classify_panic_msg("attempt to divide by zero"), "DivideByZero");
    assert_eq!(classify_panic_msg("Division by zero"), "DivideByZero");
}

#[wasm_bindgen_test]
fn test_classify_unwrap() {
    assert_eq!(
        classify_panic_msg("called `Option::unwrap()` on a `None` value"),
        "UnwrapOnNone"
    );
    assert_eq!(
        classify_panic_msg("called `Result::unwrap()` on an `Err` value: ..."),
        "UnwrapOnErr"
    );
}

#[wasm_bindgen_test]
fn test_classify_assertion() {
    assert_eq!(
        classify_panic_msg("assertion failed: x > 0"),
        "AssertionFailed"
    );
}

#[wasm_bindgen_test]
fn test_classify_unreachable() {
    assert_eq!(classify_panic_msg("entered unreachable code"), "Unreachable");
}

#[wasm_bindgen_test]
fn test_classify_overflow() {
    assert_eq!(classify_panic_msg("attempt to add with overflow"), "ArithmeticOverflow");
    assert_eq!(classify_panic_msg("capacity overflow"), "CapacityOverflow");
}

#[wasm_bindgen_test]
fn test_classify_explicit() {
    assert_eq!(classify_panic_msg("custom panic message"), "Explicit");
}

// =============================================================================
// Recovery — localStorage round-trip
// =============================================================================

#[wasm_bindgen_test]
fn test_save_load_clear_roundtrip() {
    let key = "__wasm_panic_test_key";
    let value = "recovered value 42";

    // Идемпотентный старт: чистим возможный остаток от предыдущего прогона.
    let _ = clear_state(key);

    save_state(key, value).expect("save failed");
    let loaded = load_state(key);
    assert_eq!(loaded.as_deref(), Some(value));

    clear_state(key).expect("clear failed");
    assert_eq!(load_state(key), None);
}

#[wasm_bindgen_test]
fn test_clear_idempotent() {
    let key = "__wasm_panic_test_idempotent";
    let _ = clear_state(key);
    clear_state(key).expect("second clear should succeed");
    clear_state(key).expect("third clear should succeed");
}

#[wasm_bindgen_test]
fn test_install_panic_hook_idempotent() {
    install_panic_hook();
    install_panic_hook();
    // Если не паникует — ок.
}