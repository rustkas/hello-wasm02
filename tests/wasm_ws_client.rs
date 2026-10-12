//! 36. Wasm-тесты ws_client: состояния, backoff, hex, конструктор.
//!
//! Тесты без сети — только чистая логика. Тесты с реальным
//! соединением помечены #[ignore].

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::ws_client::{backoff_delay_ms, hex_preview, WsConfig, WsState};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- WsState ----------------

#[wasm_bindgen_test]
fn wasm_ws_state_from_code_all() {
    assert_eq!(WsState::from_code(0), WsState::Connecting);
    assert_eq!(WsState::from_code(1), WsState::Open);
    assert_eq!(WsState::from_code(2), WsState::Closing);
    assert_eq!(WsState::from_code(3), WsState::Closed);
}

#[wasm_bindgen_test]
fn wasm_ws_state_from_code_unknown_is_closed() {
    assert_eq!(WsState::from_code(99), WsState::Closed);
    assert_eq!(WsState::from_code(255), WsState::Closed);
}

#[wasm_bindgen_test]
fn wasm_ws_state_as_str() {
    assert_eq!(WsState::Connecting.as_str(), "connecting");
    assert_eq!(WsState::Open.as_str(), "open");
    assert_eq!(WsState::Closing.as_str(), "closing");
    assert_eq!(WsState::Closed.as_str(), "closed");
}

// ---------------- Backoff ----------------

#[wasm_bindgen_test]
fn wasm_backoff_attempt_zero_is_base() {
    let d = backoff_delay_ms(0, 500, 15_000);
    assert_eq!(d, 500);
}

#[wasm_bindgen_test]
fn wasm_backoff_attempt_one_near_base() {
    // ±20 % от 500 = [400, 600].
    for _ in 0..20 {
        let d = backoff_delay_ms(1, 500, 15_000);
        assert!(d >= 400 && d <= 600, "d = {d}");
    }
}

#[wasm_bindgen_test]
fn wasm_backoff_grows_exponentially() {
    // Минимум при attempt=4 должен быть больше максимума при attempt=1.
    let mut max_a1 = 0u32;
    let mut min_a4 = u32::MAX;
    for _ in 0..50 {
        let a1 = backoff_delay_ms(1, 500, 15_000);
        let a4 = backoff_delay_ms(4, 500, 15_000);
        max_a1 = max_a1.max(a1);
        min_a4 = min_a4.min(a4);
    }
    assert!(min_a4 > max_a1, "min_a4 = {min_a4}, max_a1 = {max_a1}");
}

#[wasm_bindgen_test]
fn wasm_backoff_capped_at_max() {
    // attempt=10 при базе 500 должен быть закэплен на 15000 (±20 %).
    for _ in 0..20 {
        let d = backoff_delay_ms(10, 500, 15_000);
        assert!(d <= 15_000, "d = {d}");
        assert!(d >= 12_000, "d = {d}");
    }
}

#[wasm_bindgen_test]
fn wasm_backoff_never_zero() {
    for attempt in 0..=20 {
        let d = backoff_delay_ms(attempt, 100, 15_000);
        assert!(d > 0, "attempt = {attempt}, d = {d}");
    }
}

#[wasm_bindgen_test]
fn wasm_backoff_has_jitter() {
    // С jitter 10 вызовов не должны вернуть одно и то же число.
    let mut seen = std::collections::HashSet::new();
    for _ in 0..10 {
        seen.insert(backoff_delay_ms(3, 500, 15_000));
    }
    assert!(seen.len() > 1, "no jitter detected");
}

// ---------------- Hex preview ----------------

#[wasm_bindgen_test]
fn wasm_hex_preview_basic() {
    let bytes = [0xde, 0xad, 0xbe, 0xef];
    assert_eq!(hex_preview(&bytes, 8), "de ad be ef");
}

#[wasm_bindgen_test]
fn wasm_hex_preview_truncates() {
    let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
    let s = hex_preview(&bytes, 3);
    assert!(s.starts_with("01 02 03"));
    assert!(s.ends_with('…'));
}

#[wasm_bindgen_test]
fn wasm_hex_preview_empty() {
    let bytes: &[u8] = &[];
    assert_eq!(hex_preview(bytes, 8), "");
}

#[wasm_bindgen_test]
fn wasm_hex_preview_pads_single_digit() {
    let bytes = [0x0a, 0x00, 0xff];
    assert_eq!(hex_preview(&bytes, 8), "0a 00 ff");
}

// ---------------- WsConfig ----------------

#[wasm_bindgen_test]
fn wasm_ws_config_defaults() {
    let c = WsConfig::new("wss://example.com");
    assert_eq!(c.url, "wss://example.com");
    assert_eq!(c.max_reconnect_attempts, 5);
    assert_eq!(c.base_reconnect_ms, 500);
    assert_eq!(c.max_reconnect_ms, 15_000);
    assert!(c.binary_as_arraybuffer);
}