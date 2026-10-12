//! 37. Wasm-тесты wt_client: состояния, утилиты.
//!
//! WebTransport требует HTTPS-сервера, поэтому интеграционные тесты
//! помечены #[ignore]. Здесь — только чистая логика.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::wt_client::{
    format_bytes, WtConfig, WtState, WtStats,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- WtState ----------------

#[wasm_bindgen_test]
fn wasm_wt_state_as_str() {
    assert_eq!(WtState::Idle.as_str(), "idle");
    assert_eq!(WtState::Connecting.as_str(), "connecting");
    assert_eq!(WtState::Ready.as_str(), "ready");
    assert_eq!(WtState::Closed.as_str(), "closed");
    assert_eq!(WtState::Failed.as_str(), "failed");
}

// ---------------- WtConfig ----------------

#[wasm_bindgen_test]
fn wasm_wt_config_defaults() {
    let c = WtConfig::new("https://example.com:4433/echo");
    assert_eq!(c.url, "https://example.com:4433/echo");
    assert_eq!(c.datagram_buffer, 32);
    assert!(c.use_streams);
}

// ---------------- WtStats ----------------

#[wasm_bindgen_test]
fn wasm_wt_stats_default() {
    let s = WtStats::default();
    assert_eq!(s.datagrams_sent, 0);
    assert_eq!(s.datagrams_recv, 0);
    assert_eq!(s.streams_opened, 0);
    assert_eq!(s.streams_completed, 0);
    assert_eq!(s.bytes_sent, 0);
    assert_eq!(s.bytes_recv, 0);
}

// ---------------- format_bytes ----------------

#[wasm_bindgen_test]
fn wasm_format_bytes_b() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(1023), "1023 B");
}

#[wasm_bindgen_test]
fn wasm_format_bytes_kb() {
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(2048), "2.0 KB");
    assert_eq!(format_bytes(1536), "1.5 KB");
}

#[wasm_bindgen_test]
fn wasm_format_bytes_mb() {
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MB");
}