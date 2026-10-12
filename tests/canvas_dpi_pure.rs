//! Host-тесты чистой арифметики canvas-размеров.
//! Без браузера, без wasm — миллисекунды.

use hello_wasm02::canvas_dpi::{clamp_dpr, compute_canvas_size};

#[test]
fn clamp_dpr_normalizes_common_values() {
    assert_eq!(clamp_dpr(1.0), 1.0);
    assert_eq!(clamp_dpr(1.5), 1.5);
    assert_eq!(clamp_dpr(2.0), 2.0);
    assert_eq!(clamp_dpr(3.0), 3.0);
}

#[test]
fn clamp_dpr_bounds_extremes() {
    // Ниже 1 — прижимаем к 1 (браузеры не дают меньше, но защищаемся).
    assert_eq!(clamp_dpr(0.5), 1.0);
    assert_eq!(clamp_dpr(0.0), 1.0);
    assert_eq!(clamp_dpr(-2.0), 1.0);
    // Выше 4 — прижимаем к 4.
    assert_eq!(clamp_dpr(4.5), 4.0);
    assert_eq!(clamp_dpr(100.0), 4.0);
}

#[test]
fn clamp_dpr_handles_non_finite() {
    assert_eq!(clamp_dpr(f64::NAN), 1.0);
    assert_eq!(clamp_dpr(f64::INFINITY), 4.0);
    assert_eq!(clamp_dpr(f64::NEG_INFINITY), 1.0);
}

#[test]
fn canvas_size_on_normal_screen_is_identity() {
    let (w, h) = compute_canvas_size(400.0, 200.0, 1.0);
    assert_eq!((w, h), (400, 200));
}

#[test]
fn canvas_size_on_retina_doubles() {
    let (w, h) = compute_canvas_size(400.0, 200.0, 2.0);
    assert_eq!((w, h), (800, 400));
}

#[test]
fn canvas_size_rounds_to_nearest() {
    // 333 * 1.5 = 499.5 → 500
    // 111 * 1.5 = 166.5 → 167
    let (w, h) = compute_canvas_size(333.0, 111.0, 1.5);
    assert_eq!((w, h), (500, 167));
}

#[test]
fn canvas_size_never_zero() {
    assert_eq!(compute_canvas_size(0.0, 0.0, 2.0), (1, 1));
    assert_eq!(compute_canvas_size(0.4, 0.4, 1.0), (1, 1));
}

#[test]
fn canvas_size_handles_extreme_dpr() {
    // Защита от NaN-мусора: при невалидном DPR возвращаем 1.0 → размер = css.
    assert_eq!(compute_canvas_size(400.0, 200.0, f64::NAN), (400, 200));
    // Защита от слишком большого DPR: 4.0 максимум.
    assert_eq!(compute_canvas_size(400.0, 200.0, 100.0), (1600, 800));
}