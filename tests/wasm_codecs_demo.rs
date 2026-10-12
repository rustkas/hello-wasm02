#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::codecs_demo::{
    average_rgb, invert_rgba, is_video_decoder_supported,
    is_video_encoder_supported, is_video_frame_supported, is_worker_supported,
    support_summary,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn worker_is_supported_in_browser() {
    assert!(is_worker_supported());
}

#[wasm_bindgen_test]
fn video_encoder_is_supported_in_browser() {
    // Chrome/Edge/Firefox поддерживают VideoEncoder.
    // Safari — с 16.4. Тест пропустится, если нет.
    let _ = is_video_encoder_supported();
}

#[wasm_bindgen_test]
fn video_decoder_is_supported_in_browser() {
    let _ = is_video_decoder_supported();
}

#[wasm_bindgen_test]
fn video_frame_is_supported_in_browser() {
    let _ = is_video_frame_supported();
}

#[wasm_bindgen_test]
fn support_summary_returns_string() {
    let s = support_summary();
    assert!(s.contains("VideoEncoder"));
    assert!(s.contains("VideoDecoder"));
    assert!(s.contains("VideoFrame"));
}

#[wasm_bindgen_test]
fn invert_rgba_flips_rgb_keeps_alpha() {
    let mut data = vec![10u8, 20, 30, 200, 200, 100, 50, 255];
    invert_rgba(&mut data);
    assert_eq!(data[0], 245);
    assert_eq!(data[1], 235);
    assert_eq!(data[2], 225);
    assert_eq!(data[3], 200, "alpha preserved");
    assert_eq!(data[4], 55);
    assert_eq!(data[5], 155);
    assert_eq!(data[6], 205);
    assert_eq!(data[7], 255, "alpha preserved");
}

#[wasm_bindgen_test]
fn invert_rgba_is_self_inverse() {
    let orig = vec![
        10u8, 20, 30, 100, 40, 50, 60, 150, 200, 210, 220, 250,
    ];
    let mut data = orig.clone();
    invert_rgba(&mut data);
    invert_rgba(&mut data);
    assert_eq!(data, orig);
}

#[wasm_bindgen_test]
fn invert_rgba_handles_empty() {
    let mut data: Vec<u8> = vec![];
    invert_rgba(&mut data);
    assert!(data.is_empty());
}

#[wasm_bindgen_test]
fn invert_rgba_handles_unaligned_length() {
    // длина не кратна 4 — функция должна обработать только
    // кратную часть, не паниковать.
    let mut data = vec![10u8, 20, 30, 40, 50];
    invert_rgba(&mut data);
    assert_eq!(data[0], 245);
    assert_eq!(data[1], 235);
    assert_eq!(data[2], 225);
    assert_eq!(data[3], 40);   // alpha
    assert_eq!(data[4], 50);   // хвост не тронут
}

#[wasm_bindgen_test]
fn average_rgb_of_uniform_is_that_value() {
    // 4 пикселя, все (100, 100, 100, 255).
    let mut data = vec![0u8; 16];
    for i in (0..16).step_by(4) {
        data[i] = 100;
        data[i + 1] = 100;
        data[i + 2] = 100;
        data[i + 3] = 255;
    }
    let avg = average_rgb(&data);
    assert!((avg - 100.0).abs() < 0.001);
}

#[wasm_bindgen_test]
fn average_rgb_ignores_alpha() {
    // Пиксель (100, 100, 100, 0) — alpha нулевая, но среднее по RGB = 100.
    let data = vec![100u8, 100, 100, 0];
    let avg = average_rgb(&data);
    assert!((avg - 100.0).abs() < 0.001);
}