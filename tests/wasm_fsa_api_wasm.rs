//! Тесты `src/fsa_api_wasm.rs`.
//!
//! **Что проверяем.**
//! * Генерацию тестового PNG (детерминированный паттерн).
//! * `check_grayscale` — принимает grayscale, отвергает цвет.
//! * Roundtrip PNG через WASM-функцию `grayscale_wasm`.
//! * Что все три движка дают grayscale-результат.
//!
//! **Что не проверяем.**
//! * Попиксельное совпадение результатов разных движков — они
//!   используют разные цветовые пространства и алгоритмы сведения.
//!   Инвариант grayscale — то, что действительно должно совпадать.

use wasm_bindgen_test::*;

use hello_wasm02::fsa_api_wasm::{
    check_grayscale, generate_test_png, grayscale_native, grayscale_wasm, grayscale_worker,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Генерация и базовые проверки
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_generate_test_png_returns_bytes() {
    let bytes = generate_test_png(16, 16,0.0).expect("generate");
    assert!(!bytes.is_empty(), "PNG не может быть пустым");
    // PNG всегда начинается с сигнатуры \x89PNG\r\n\x1a\n.
    assert_eq!(&bytes[..4], b"\x89PNG", "PNG-сигнатура");
}

#[wasm_bindgen_test]
fn test_check_grayscale_rejects_color() {
    // Цветной паттерн (R по X, G по Y) — не grayscale.
    let color = generate_test_png(16, 16,0.0).expect("generate");
    let result = check_grayscale(&color);
    assert!(result.is_err(), "цветной PNG не должен проходить check_grayscale");
}

// ---------------------------------------------------------------------------
// WASM движок (синхронный)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_grayscale_wasm_roundtrip() {
    let input = generate_test_png(32, 32,0.0).expect("generate");
    let output = grayscale_wasm(&input).expect("grayscale_wasm");
    assert!(!output.is_empty(), "результат не пустой");
    assert_eq!(&output[..4], b"\x89PNG", "PNG-сигнатура результата");
    check_grayscale(&output).expect("результат grayscale");
}

#[wasm_bindgen_test]
fn test_grayscale_wasm_rejects_garbage() {
    // Мусор на входе — Err, не паника.
    let garbage: Vec<u8> = vec![0, 1, 2, 3, 4, 5];
    let result = grayscale_wasm(&garbage);
    assert!(result.is_err(), "мусор должен дать Err");
}

// ---------------------------------------------------------------------------
// Native Canvas движок
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_grayscale_native_roundtrip() {
    let input = generate_test_png(32, 32,0.0).expect("generate");
    let output = grayscale_native(&input).await.expect("grayscale_native");
    assert!(!output.is_empty(), "результат не пустой");
    check_grayscale(&output).expect("результат grayscale");
}

// ---------------------------------------------------------------------------
// Worker движок
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_grayscale_worker_roundtrip() {
    let input = generate_test_png(32, 32,0.0).expect("generate");
    let output = grayscale_worker(&input).await.expect("grayscale_worker");
    assert!(!output.is_empty(), "результат не пустой");
    check_grayscale(&output).expect("результат grayscale");
}

// ---------------------------------------------------------------------------
// Все три движка дают grayscale
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_all_engines_produce_grayscale() {
    let input = generate_test_png(24, 24,0.0).expect("generate");

    let a = grayscale_wasm(&input).expect("wasm");
    check_grayscale(&a).expect("wasm — grayscale");

    let b = grayscale_native(&input).await.expect("native");
    check_grayscale(&b).expect("native — grayscale");

    let c = grayscale_worker(&input).await.expect("worker");
    check_grayscale(&c).expect("worker — grayscale");
}
#[wasm_bindgen_test]
fn test_generate_different_angles_produce_different_images() {
    let a = generate_test_png(64, 64, 0.0).expect("a");
    let b = generate_test_png(64, 64, std::f64::consts::FRAC_PI_4).expect("b");
    assert_ne!(a, b, "разные углы дают разные PNG");
}