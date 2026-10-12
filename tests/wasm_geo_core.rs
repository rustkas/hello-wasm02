//! Wasm-тесты geo_core: доступность API, коды ошибок, options,
//! геометрия, форматирование координат.
//!
//! ПРАВИЛО: PositionError нельзя сконструировать в JS — только
//! получить от браузера. Маппинг кодов тестируется через from_code.
//!
//! ПРАВИЛО: PositionOptions — dictionary. web-sys не генерирует
//! getter'ы для dictionary, только setters.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::geo_core::{
    self, GeoError, GeoErrorKind, GeoFormat, GeoOptions, GeoPosition,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- Доступность API ----------------

#[wasm_bindgen_test]
fn wasm_geolocation_api_is_supported() {
    assert!(geo_core::is_supported());
}

// ---------------- GeoOptions ----------------

#[wasm_bindgen_test]
fn wasm_default_options_have_finite_timeout() {
    let o = GeoOptions::default();
    assert!(o.timeout_ms > 0);
    assert!(o.timeout_ms <= 30_000);
}

#[wasm_bindgen_test]
fn wasm_options_convert_to_web_sys() {
    let opts = GeoOptions {
        high_accuracy: true,
        timeout_ms: 5000,
        max_age_ms: 1000,
    };
    // Dictionary: getters отсутствуют, проверяем только успешность.
    assert!(opts.to_web_sys().is_ok());
}

// ---------------- Маппинг кодов ошибок ----------------

#[wasm_bindgen_test]
fn wasm_error_kind_from_code_all() {
    assert_eq!(GeoErrorKind::from_code(1), GeoErrorKind::PermissionDenied);
    assert_eq!(GeoErrorKind::from_code(2), GeoErrorKind::PositionUnavailable);
    assert_eq!(GeoErrorKind::from_code(3), GeoErrorKind::Timeout);
    assert_eq!(GeoErrorKind::from_code(0), GeoErrorKind::Unknown);
    assert_eq!(GeoErrorKind::from_code(99), GeoErrorKind::Unknown);
}

#[wasm_bindgen_test]
fn wasm_error_recoverable_flags() {
    assert!(GeoErrorKind::Timeout.is_recoverable());
    assert!(GeoErrorKind::PositionUnavailable.is_recoverable());
    assert!(!GeoErrorKind::PermissionDenied.is_recoverable());
    assert!(!GeoErrorKind::Unknown.is_recoverable());
}

#[wasm_bindgen_test]
fn wasm_user_hints_are_non_empty_for_all_kinds() {
    for kind in [
        GeoErrorKind::PermissionDenied,
        GeoErrorKind::PositionUnavailable,
        GeoErrorKind::Timeout,
        GeoErrorKind::Unknown,
    ] {
        let e = GeoError {
            kind,
            message: "test".to_string(),
        };
        assert!(!e.user_hint().is_empty(), "kind = {:?}", kind);
    }
}

// ---------------- Haversine ----------------

#[wasm_bindgen_test]
fn wasm_haversine_zero_for_same_point() {
    let d = geo_core::haversine_m(12.87, 100.89, 12.87, 100.89);
    assert!(d < 0.001, "distance = {d}");
}

#[wasm_bindgen_test]
fn wasm_haversine_pattaya_to_bangkok() {
    // Паттайя (12.9236, 100.8824) — Бангкок (13.7563, 100.5018).
    // Реальное расстояние ≈ 100 км (по прямой).
    let d = geo_core::haversine_m(12.9236, 100.8824, 13.7563, 100.5018);
    assert!(d > 90_000.0, "too short: {d}");
    assert!(d < 110_000.0, "too long: {d}");
}

#[wasm_bindgen_test]
fn wasm_haversine_equator_one_degree() {
    // 1° по долготе на экваторе ≈ 111 195 м.
    let d = geo_core::haversine_m(0.0, 0.0, 0.0, 1.0);
    assert!((d - 111_195.0).abs() < 100.0, "d = {d}");
}

#[wasm_bindgen_test]
fn wasm_haversine_symmetric() {
    let d1 = geo_core::haversine_m(10.0, 20.0, 30.0, 40.0);
    let d2 = geo_core::haversine_m(30.0, 40.0, 10.0, 20.0);
    assert!((d1 - d2).abs() < 0.001);
}

// ---------------- Bearing ----------------

#[wasm_bindgen_test]
fn wasm_bearing_north_is_zero() {
    // Из (0,0) в (1,0) — строго на север.
    let b = geo_core::bearing_deg(0.0, 0.0, 1.0, 0.0);
    assert!(b < 0.5 || b > 359.5, "b = {b}");
}

#[wasm_bindgen_test]
fn wasm_bearing_east_is_ninety() {
    // Из (0,0) в (0,1) — строго на восток.
    let b = geo_core::bearing_deg(0.0, 0.0, 0.0, 1.0);
    assert!((b - 90.0).abs() < 0.5, "b = {b}");
}

#[wasm_bindgen_test]
fn wasm_bearing_south_is_180() {
    let b = geo_core::bearing_deg(1.0, 0.0, 0.0, 0.0);
    assert!((b - 180.0).abs() < 0.5, "b = {b}");
}

#[wasm_bindgen_test]
fn wasm_bearing_in_range() {
    for (lat1, lon1, lat2, lon2) in [
        (10.0, 20.0, 30.0, 40.0),
        (-5.0, 100.0, 5.0, -100.0),
        (45.0, 0.0, 45.0, 179.9),
    ] {
        let b = geo_core::bearing_deg(lat1, lon1, lat2, lon2);
        assert!(b >= 0.0 && b < 360.0, "b = {b}");
    }
}

// ---------------- Smoothing ----------------

#[wasm_bindgen_test]
fn wasm_smooth_ema_identity_at_one() {
    // alpha = 1.0 — новое значение полностью перетирает старое.
    assert!((geo_core::smooth_ema(0.0, 10.0, 1.0) - 10.0).abs() < 1e-9);
}

#[wasm_bindgen_test]
fn wasm_smooth_ema_no_change_at_zero() {
    // alpha = 0.0 — старое значение сохраняется.
    assert!((geo_core::smooth_ema(5.0, 10.0, 0.0) - 5.0).abs() < 1e-9);
}

#[wasm_bindgen_test]
fn wasm_smooth_ema_midpoint() {
    // alpha = 0.5 — среднее.
    assert!((geo_core::smooth_ema(0.0, 10.0, 0.5) - 5.0).abs() < 1e-9);
}

// ---------------- Формат координат ----------------

#[wasm_bindgen_test]
fn wasm_format_decimal_lat() {
    let s = GeoFormat::Decimal.format_lat(12.871196);
    assert!(s.contains("N"), "s = {s}");
    assert!(s.contains("12.871196"), "s = {s}");
}

#[wasm_bindgen_test]
fn wasm_format_decimal_lon_west() {
    let s = GeoFormat::Decimal.format_lon(-100.5);
    assert!(s.contains("W"), "s = {s}");
    assert!(s.contains("100.5"), "s = {s}");
}

#[wasm_bindgen_test]
fn wasm_format_dms_lat() {
    // 12.871196° = 12° 52' 16.3" N.
    let s = GeoFormat::Dms.format_lat(12.871196);
    assert!(s.contains("12°"), "s = {s}");
    assert!(s.contains("52'"), "s = {s}");
    assert!(s.contains("N"), "s = {s}");
}

#[wasm_bindgen_test]
fn wasm_format_dms_zero() {
    let s = GeoFormat::Dms.format_lat(0.0);
    assert!(s.contains("0° 0' 0.0\""), "s = {s}");
}

// ---------------- Accuracy labels ----------------

#[wasm_bindgen_test]
fn wasm_accuracy_labels_boundaries() {
    let make = |acc: f64| GeoPosition {
        latitude: 0.0,
        longitude: 0.0,
        accuracy_m: acc,
        altitude_m: None,
        heading_deg: None,
        speed_mps: None,
        timestamp_ms: 0.0,
    };
    assert_eq!(make(0.0).accuracy_label(), "высокая (GPS)");
    assert_eq!(make(19.99).accuracy_label(), "высокая (GPS)");
    assert_eq!(make(20.0).accuracy_label(), "средняя (сеть)");
    assert_eq!(make(199.99).accuracy_label(), "средняя (сеть)");
    assert_eq!(make(200.0).accuracy_label(), "низкая (IP)");
    assert_eq!(make(5000.0).accuracy_label(), "низкая (IP)");
}

// ---------------- clear_watch идемпотентен ----------------

#[wasm_bindgen_test]
fn wasm_clear_watch_is_idempotent() {
    let _ = geo_core::clear_watch(99999);
    let _ = geo_core::clear_watch(99999);
}

// ---------------- Permissions API ----------------

#[wasm_bindgen_test]
fn wasm_permission_state_returns_something_or_none() {
    let _ = geo_core::permission_state();
}