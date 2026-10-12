//! Тесты чистых функций обработки пикселей.
//!
//! Не требуют браузера — запускаются через `cargo test`.

use hello_wasm02::pixel_ops::{average_rgb, invert_rgba};

// =============================================================================
// invert_rgba: базовые случаи
// =============================================================================

#[test]
fn invert_rgba_is_self_inverse() {
    let original: Vec<u8> = (0..64).map(|i| (i * 3) as u8).collect();
    let mut bytes = original.clone();
    invert_rgba(&mut bytes);
    assert_ne!(bytes, original, "первое invert должно изменить данные");
    invert_rgba(&mut bytes);
    assert_eq!(bytes, original, "второе invert должно вернуть оригинал");
}

#[test]
fn invert_rgba_preserves_alpha() {
    // 4 пикселя: alpha разная, RGB разные.
    let mut bytes = vec![
        10, 20, 30, 200,   // alpha 200
        40, 50, 60, 100,   // alpha 100
        70, 80, 90, 255,   // alpha 255
        100, 110, 120, 0,  // alpha 0
    ];
    invert_rgba(&mut bytes);
    assert_eq!(bytes[3], 200);
    assert_eq!(bytes[7], 100);
    assert_eq!(bytes[11], 255);
    assert_eq!(bytes[15], 0);
}

#[test]
fn invert_rgba_handles_non_multiple_of_4() {
    // 10 байт: 2 полных пикселя + 2 «хвостовых».
    let mut bytes = vec![10, 20, 30, 200, 40, 50, 60, 100, 99, 88];
    invert_rgba(&mut bytes);
    // Первые два пикселя инвертированы.
    assert_eq!(bytes[0], 245);
    assert_eq!(bytes[1], 235);
    assert_eq!(bytes[2], 225);
    assert_eq!(bytes[3], 200);
    assert_eq!(bytes[4], 215);
    // Хвостовые байты не тронуты.
    assert_eq!(bytes[8], 99);
    assert_eq!(bytes[9], 88);
}

#[test]
fn invert_rgba_empty_does_not_panic() {
    let mut bytes: Vec<u8> = Vec::new();
    invert_rgba(&mut bytes);
    assert!(bytes.is_empty());
}

#[test]
fn invert_rgba_black_to_white() {
    let mut bytes = vec![0, 0, 0, 255];
    invert_rgba(&mut bytes);
    assert_eq!(bytes, vec![255, 255, 255, 255]);
}

#[test]
fn invert_rgba_white_to_black() {
    let mut bytes = vec![255, 255, 255, 255];
    invert_rgba(&mut bytes);
    assert_eq!(bytes, vec![0, 0, 0, 255]);
}

// =============================================================================
// average_rgb
// =============================================================================

#[test]
fn average_rgb_ignores_alpha() {
    // Один красный (255, 0, 0), alpha 0; один красный, alpha 255.
    // Среднее должно быть (255, 0, 0) — alpha не влияет.
    let bytes = vec![255, 0, 0, 0, 255, 0, 0, 255];
    let (r, g, b) = average_rgb(&bytes);
    assert!((r - 255.0).abs() < 0.01);
    assert!(g.abs() < 0.01);
    assert!(b.abs() < 0.01);
}

#[test]
fn average_rgb_simple() {
    // Два пикселя: (0, 100, 200) и (100, 200, 0).
    // Среднее: (50, 150, 100).
    let bytes = vec![0, 100, 200, 255, 100, 200, 0, 255];
    let (r, g, b) = average_rgb(&bytes);
    assert!((r - 50.0).abs() < 0.01);
    assert!((g - 150.0).abs() < 0.01);
    assert!((b - 100.0).abs() < 0.01);
}

#[test]
fn average_rgb_empty_returns_zero() {
    let (r, g, b) = average_rgb(&[]);
    assert_eq!((r, g, b), (0.0, 0.0, 0.0));
}

#[test]
fn average_rgb_non_multiple_of_4() {
    // 6 байт: 1 полный пиксель + 2 «хвостовых».
    let bytes = vec![50, 60, 70, 255, 99, 88];
    let (r, g, b) = average_rgb(&bytes);
    assert!((r - 50.0).abs() < 0.01);
    assert!((g - 60.0).abs() < 0.01);
    assert!((b - 70.0).abs() < 0.01);
}

#[test]
fn invert_and_average_consistency() {
    // Средний RGB инвертированного буфера — это 255 минус средний
    // RGB оригинала (по каждому каналу отдельно).
    let mut bytes = vec![
        10, 20, 30, 255,
        200, 100, 50, 128,
        40, 60, 80, 64,
    ];
    let (r0, g0, b0) = average_rgb(&bytes);
    invert_rgba(&mut bytes);
    let (r1, g1, b1) = average_rgb(&bytes);
    assert!((r1 - (255.0 - r0)).abs() < 0.01);
    assert!((g1 - (255.0 - g0)).abs() < 0.01);
    assert!((b1 - (255.0 - b0)).abs() < 0.01);
}