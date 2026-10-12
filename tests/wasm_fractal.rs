#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::fractal::{
    escape, is_offscreen_canvas_supported, is_worker_supported,
    render_mandelbrot_chunk,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn worker_is_supported_in_browser() {
    assert!(is_worker_supported());
}

#[wasm_bindgen_test]
fn offscreen_canvas_is_supported_in_browser() {
    assert!(is_offscreen_canvas_supported());
}

#[wasm_bindgen_test]
fn origin_is_inside_set() {
    // 0 + 0i — внутри множества, никогда не выходит за радиус 2.
    assert_eq!(escape(0.0, 0.0, 500), 500);
}

#[wasm_bindgen_test]
fn far_point_escapes_immediately() {
    // c = 10+10i: z0=0, z1=10+10i, |z1|²=200 > 4.
    // Выход на итерации 1.
    assert_eq!(escape(10.0, 10.0, 100), 1);
}

#[wasm_bindgen_test]
fn one_plus_zero_escapes_quickly() {
    // Точка 1 + 0i: z0=0, z1=1, z2=2, z3=5 — выход за 4 итерации.
    let iters = escape(1.0, 0.0, 100);
    assert!(iters >= 2, "must escape");
    assert!(iters < 10, "must escape quickly, got {}", iters);
}

#[wasm_bindgen_test]
fn render_chunk_sets_all_alphas_to_255() {
    let w = 8u32;
    let h = 8u32;
    let mut data = vec![0u8; (w * h * 4) as usize];
    render_mandelbrot_chunk(&mut data, w, h, 0, -0.5, 0.0, 0.01, 50, 0);
    for i in (0..data.len()).step_by(4) {
        assert_eq!(data[i + 3], 255, "alpha at pixel {} is set", i / 4);
    }
}

#[wasm_bindgen_test]
fn render_inside_pixel_is_black() {
    // Точка 0+0i — внутри множества, должна быть чёрной (0,0,0).
    let w = 1u32;
    let h = 1u32;
    let mut data = vec![0u8; 4];
    // cx=0, cy=0, pixel_size такой, что центр совпадает с (0,0).
    render_mandelbrot_chunk(&mut data, w, h, 0, 0.0, 0.0, 0.1, 100, 0);
    assert_eq!(data[0], 0);
    assert_eq!(data[1], 0);
    assert_eq!(data[2], 0);
    assert_eq!(data[3], 255);
}

#[wasm_bindgen_test]
fn render_outside_pixel_is_not_black_grayscale() {
    // Точка 10+10i — снаружи, grayscale palette → белый.
    let w = 1u32;
    let h = 1u32;
    let mut data = vec![0u8; 4];
    render_mandelbrot_chunk(&mut data, w, h, 0, 10.0, 10.0, 0.1, 100, 2);
    // iter == 0 → t=0 → v=0 → чёрный. Хм, тоже чёрный.
    // Возьмём точку, где iter > 0, но < max.
    // Точка 1+0i с max_iter=100 → iter=2, t=0.02, v=5.
    render_mandelbrot_chunk(&mut data, w, h, 0, 1.0, 0.0, 0.1, 100, 2);
    assert!(data[0] > 0, "grayscale should produce non-zero for iter>0");
}