//! Wasm-тесты модуля ls_bloat: раздутие base64, кодирование/декодирование,
//! сохранение бинарных данных через localStorage.
//!
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::ls_bloat;
use hello_wasm02::ls_probe;

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

// ---------- random_bytes: детерминированность ----------

#[wasm_bindgen_test]
fn random_bytes_length() {
    assert_eq!(ls_bloat::random_bytes(10, 42).len(), 10);
    assert_eq!(ls_bloat::random_bytes(0, 42).len(), 0);
    assert_eq!(ls_bloat::random_bytes(1024, 42).len(), 1024);
}

#[wasm_bindgen_test]
fn random_bytes_deterministic_by_seed() {
    let a = ls_bloat::random_bytes(20, 12345);
    let b = ls_bloat::random_bytes(20, 12345);
    assert_eq!(a, b, "одинаковый seed должен давать одинаковые байты");
}

#[wasm_bindgen_test]
fn random_bytes_different_for_different_seeds() {
    let a = ls_bloat::random_bytes(20, 1);
    let b = ls_bloat::random_bytes(20, 2);
    assert_ne!(a, b);
}

// ---------- base64: roundtrip ----------

#[wasm_bindgen_test]
fn base64_roundtrip_empty() {
    let enc = ls_bloat::encode_base64(&[]);
    assert_eq!(enc, "");
    assert_eq!(ls_bloat::decode_base64(&enc).unwrap(), Vec::<u8>::new());
}

#[wasm_bindgen_test]
fn base64_roundtrip_known() {
    // Классический пример: [0x01, 0x02, 0x03] → "AQID".
    let enc = ls_bloat::encode_base64(&[1, 2, 3]);
    assert_eq!(enc, "AQID");
    assert_eq!(ls_bloat::decode_base64("AQID").unwrap(), vec![1, 2, 3]);
}

#[wasm_bindgen_test]
fn base64_roundtrip_arbitrary() {
    let data = ls_bloat::random_bytes(100, 7);
    let enc = ls_bloat::encode_base64(&data);
    let dec = ls_bloat::decode_base64(&enc).unwrap();
    assert_eq!(dec, data);
}

#[wasm_bindgen_test]
fn base64_decode_invalid_fails() {
    let r = ls_bloat::decode_base64("!!!");
    assert!(r.is_err(), "невалидная base64 должна дать ошибку");
}

// ---------- analyze_bloat: математика ----------

#[wasm_bindgen_test]
fn analyze_bloat_1000_bytes() {
    let data = vec![0u8; 1000];
    let r = ls_bloat::analyze_bloat(&data);
    assert_eq!(r.raw_bytes, 1000);
    // Base64: ceil(1000/3)*4 = 1336 символов.
    assert_eq!(r.base64_chars, 1336);
    // localStorage хранит строки в UTF-16: 1336 × 2 = 2672 байт.
    assert_eq!(r.utf16_bytes, 2672);
    // Отношение ≈ 2.672.
    assert!((r.ratio - 2.672).abs() < 0.001);
}

#[wasm_bindgen_test]
fn analyze_bloat_empty_returns_zero_ratio() {
    let r = ls_bloat::analyze_bloat(&[]);
    assert_eq!(r.raw_bytes, 0);
    assert_eq!(r.base64_chars, 0);
    assert_eq!(r.utf16_bytes, 0);
    assert_eq!(r.ratio, 0.0);
}

// ---------- сохранение бинарных данных через localStorage ----------

#[wasm_bindgen_test]
fn store_and_load_bytes_roundtrip() {
    let key = format!("bloat-{}", uid());
    let data = ls_bloat::random_bytes(500, 99);

    let stored_bytes = ls_bloat::store_bytes_as_base64(&key, &data).unwrap();
    assert!(stored_bytes > 0, "размер записи должен быть > 0");

    let back = ls_bloat::load_bytes_from_base64(&key).unwrap();
    assert_eq!(back, Some(data), "данные должны вернуться без искажений");

    ls_probe::remove(&key).unwrap();
}

#[wasm_bindgen_test]
fn load_missing_returns_none() {
    let key = format!("bloat-missing-{}", uid());
    let r = ls_bloat::load_bytes_from_base64(&key).unwrap();
    assert!(r.is_none());
}

#[wasm_bindgen_test]
fn stored_size_matches_utf16_bytes() {
    let key = format!("bloat-sz-{}", uid());
    let data = vec![42u8; 100];
    let stored = ls_bloat::store_bytes_as_base64(&key, &data).unwrap();
    // В localStorage записана base64-строка; её размер в UTF-16
    // = количество символов × 2.
    let base64_len = ls_bloat::encode_base64(&data).len();
    assert_eq!(stored, base64_len * 2);
    ls_probe::remove(&key).unwrap();
}