//! Host-тесты чистых функций обработки пикселей.
//! Запускаются `cargo test` — без браузера, без wasm, за миллисекунды.

use hello_wasm02::canvas_image_processor::{
    box_blur_3x3, invert, make_sample_pixels, sobel_edge, to_grayscale, Filter,
};

/// Создать однотонный RGBA-буфер `w × h` одного цвета.
fn solid(w: u32, h: u32, r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        v.extend_from_slice(&[r, g, b, a]);
    }
    v
}

#[test]
fn grayscale_of_pure_red_is_expected_luminance() {
    // 0.2126 * 255 ≈ 54
    let px = vec![255, 0, 0, 255];
    let out = to_grayscale(&px);
    assert_eq!(out.len(), 4);
    assert_eq!(out[0], out[1]);
    assert_eq!(out[1], out[2]);
    assert_eq!(out[3], 255, "alpha unchanged");
    assert!(out[0] >= 53 && out[0] <= 55, "got {}", out[0]);
}

#[test]
fn grayscale_of_pure_green_is_brighter_than_red() {
    let red = to_grayscale(&[255, 0, 0, 255])[0];
    let green = to_grayscale(&[0, 255, 0, 255])[0];
    let blue = to_grayscale(&[0, 0, 255, 255])[0];
    assert!(green > red, "green {} should beat red {}", green, red);
    assert!(red > blue, "red {} should beat blue {}", red, blue);
}

#[test]
fn invert_flips_rgb_keeps_alpha() {
    let px = vec![10, 20, 30, 200];
    let out = invert(&px);
    assert_eq!(out, vec![245, 235, 225, 200]);
}

#[test]
fn invert_is_its_own_inverse() {
    let px = solid(4, 4, 11, 22, 33, 44);
    let twice = invert(&invert(&px));
    assert_eq!(twice, px);
}

#[test]
fn blur_of_solid_image_is_identity() {
    // Размытие однотонного — тот же цвет.
    let px = solid(8, 8, 100, 150, 200, 255);
    let out = box_blur_3x3(&px, 8, 8);
    assert_eq!(out, px);
}

#[test]
fn blur_softens_a_single_bright_pixel() {
    // 3×3 с белой точкой в центре.
    let mut px = vec![0u8; 3 * 3 * 4];
    for i in 0..9 {
        px[i * 4 + 3] = 255; // alpha
    }
    let center = (1 * 3 + 1) * 4;
    px[center] = 255;
    px[center + 1] = 255;
    px[center + 2] = 255;
    let out = box_blur_3x3(&px, 3, 3);
    // Центр размылся: 255/9 ≈ 28.
    assert!(out[center] < 255, "center should be dimmed");
    assert!(out[center] > 0, "but not zero");
}

#[test]
fn sobel_on_uniform_image_is_black() {
    // Однотонная — нет градиента, все значения нулевые.
    let px = solid(6, 6, 128, 128, 128, 255);
    let out = sobel_edge(&px, 6, 6);
    for i in 0..6 * 6 {
        assert_eq!(out[i * 4], 0, "pixel {} should be black", i);
    }
}

#[test]
fn sobel_on_vertical_edge_lights_up_boundary() {
    // Левые 3 колонки — чёрные, правые 3 — белые.
    let mut px = vec![0u8; 6 * 6 * 4];
    for y in 0..6 {
        for x in 3..6 {
            let i = (y * 6 + x) * 4;
            px[i] = 255;
            px[i + 1] = 255;
            px[i + 2] = 255;
        }
    }
    for i in 0..6 * 6 {
        px[i * 4 + 3] = 255;
    }
    let out = sobel_edge(&px, 6, 6);
    // Граница — колонки 2 и 3. Там должно быть ярче, чем в однотонных зонах.
    let edge_px = (3 * 6 + 3) * 4;
    let flat_px = (3 * 6 + 0) * 4;
    assert!(out[edge_px] > out[flat_px], "edge should be brighter than flat");
}

#[test]
fn identity_filter_returns_copy() {
    let px = solid(4, 4, 1, 2, 3, 4);
    let out = Filter::Identity.apply(&px, 4, 4);
    assert_eq!(out, px);
}

#[test]
fn sample_has_correct_size_and_alpha() {
    let px = make_sample_pixels(64, 48);
    assert_eq!(px.len(), 64 * 48 * 4);
    // Все альфы = 255.
    for chunk in px.chunks_exact(4) {
        assert_eq!(chunk[3], 255);
    }
}

#[test]
fn sample_is_not_uniform() {
    // Встроенная картинка должна содержать разные цвета — иначе фильтры нечего показывать.
    let px = make_sample_pixels(64, 48);
    let first = &px[0..3];
    let mut distinct = 0;
    for chunk in px.chunks_exact(4).step_by(37) {
        if &chunk[0..3] != first {
            distinct += 1;
        }
    }
    assert!(distinct > 10, "sample should have many distinct colors");
}