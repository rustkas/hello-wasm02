#![cfg(target_arch = "wasm32")]

//! Тесты 23.12 — video filters.

use wasm_bindgen_test::*;
use hello_wasm02::video_filters::{
    apply_filter, FILTER_GRAYSCALE, FILTER_INVERT, FILTER_NONE, FILTER_SEPIA,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn filter_none_returns_input_unchanged() {
    let input = [10u8, 20, 30, 255, 40, 50, 60, 128];
    let out = apply_filter(&input, 2, 1, FILTER_NONE);
    assert_eq!(out, input);
}

#[wasm_bindgen_test]
fn grayscale_gives_equal_channels() {
    // Pure red (255, 0, 0) → Y = 0.299 * 255 = 76.245 → 76.
    let input = [255u8, 0, 0, 255];
    let out = apply_filter(&input, 1, 1, FILTER_GRAYSCALE);
    assert_eq!(out[0], 76);
    assert_eq!(out[1], 76);
    assert_eq!(out[2], 76);
    assert_eq!(out[3], 255);
}

#[wasm_bindgen_test]
fn grayscale_preserves_alpha() {
    let input = [255u8, 255, 255, 0];
    let out = apply_filter(&input, 1, 1, FILTER_GRAYSCALE);
    assert_eq!(out[3], 0);
}

#[wasm_bindgen_test]
fn invert_flips_channels() {
    let input = [0u8, 128, 255, 200];
    let out = apply_filter(&input, 1, 1, FILTER_INVERT);
    assert_eq!(out[0], 255);
    assert_eq!(out[1], 127);
    assert_eq!(out[2], 0);
    assert_eq!(out[3], 200);
}

#[wasm_bindgen_test]
fn invert_is_self_inverse() {
    let input = [100u8, 150, 200, 255];
    let once = apply_filter(&input, 1, 1, FILTER_INVERT);
    let twice = apply_filter(&once, 1, 1, FILTER_INVERT);
    assert_eq!(twice, input);
}

#[wasm_bindgen_test]
fn sepia_produces_warm_tone() {
    // Pure white → warm sepia tone (red dominates blue).
    let input = [255u8, 255, 255, 255];
    let out = apply_filter(&input, 1, 1, FILTER_SEPIA);
    assert!(out[0] >= out[2], "red should dominate blue in sepia");
    assert_eq!(out[3], 255);
}

#[wasm_bindgen_test]
fn filter_handles_multiple_pixels() {
    let input: Vec<u8> = (0..32).collect();
    let out = apply_filter(&input, 4, 2, FILTER_INVERT);
    assert_eq!(out.len(), input.len());
    for i in 0..(input.len() / 4) {
        let base = i * 4;
        assert_eq!(out[base], 255 - input[base]);
        assert_eq!(out[base + 1], 255 - input[base + 1]);
        assert_eq!(out[base + 2], 255 - input[base + 2]);
        assert_eq!(out[base + 3], input[base + 3]);
    }
}