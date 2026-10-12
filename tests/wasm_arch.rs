//! 50. Тесты архитектурных решений.
//!
//! ## Что тестируется
//!
//! - Чистые алгоритмы: `fib_recursive`, `fib_iterative`.
//! - Сортировка: порядок, длина, устойчивость к дубликатам.
//! - CRC-32 и CRC-16/CCITT-FALSE на эталонных векторах.
//! - UTF-8 code points: ASCII, кириллица, emoji.
//! - Протокол кадров: build/parse roundtrip, обрезанные данные.
//! - Генераторы данных: детерминированность.
//! - JS-обёртки: `compile_*` возвращает функцию, вызов даёт
//!   то же значение, что Rust-версия.
//!
//! ## Имя крейта
//!
//! `hello_wasm02` — соответствует `[package] name = "hello-wasm02"`
//! в вашем `Cargo.toml`. Если у вас другое имя, поменяйте во всех
//! `use`-строках этого файла (и в остальных `tests/wasm_*.rs`).

use wasm_bindgen_test::*;

use hello_wasm02::arch_js_bench::{
    call_count_code_points, call_crc32, call_fib, call_parse_frames,
    compile_count_code_points, compile_crc32, compile_fib, compile_parse_frames,
};
use hello_wasm02::arch_pure::{
    build_frame, count_code_points, crc16_ccitt, crc32, fib_iterative,
    fib_recursive, gen_bytes, gen_text, gen_u32_array, parse_frames,
    simulate_stream, sort_u32,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// 1. Fibonacci
// =============================================================================

#[wasm_bindgen_test]
fn test_fib_small() {
    assert_eq!(fib_recursive(0), 0);
    assert_eq!(fib_recursive(1), 1);
    assert_eq!(fib_recursive(2), 1);
    assert_eq!(fib_recursive(10), 55);
    assert_eq!(fib_recursive(20), 6765);
}

#[wasm_bindgen_test]
fn test_fib_recursive_matches_iterative() {
    for n in 0..25 {
        assert_eq!(
            fib_recursive(n),
            fib_iterative(n),
            "fib({n}) mismatch"
        );
    }
}

// =============================================================================
// 2. Сортировка
// =============================================================================

#[wasm_bindgen_test]
fn test_sort_basic() {
    let mut data = vec![5, 1, 4, 2, 8, 0, 3];
    sort_u32(&mut data);
    assert_eq!(data, vec![0, 1, 2, 3, 4, 5, 8]);
}

#[wasm_bindgen_test]
fn test_sort_with_duplicates() {
    let mut data = vec![3, 1, 3, 1, 3, 1];
    sort_u32(&mut data);
    assert_eq!(data, vec![1, 1, 1, 3, 3, 3]);
}

#[wasm_bindgen_test]
fn test_sort_empty() {
    let mut data: Vec<u32> = Vec::new();
    sort_u32(&mut data);
    assert!(data.is_empty());
}

#[wasm_bindgen_test]
fn test_sort_generated_data() {
    let mut data = gen_u32_array(1000, 0xDEAD_BEEF);
    sort_u32(&mut data);
    for i in 1..data.len() {
        assert!(data[i - 1] <= data[i], "not sorted at index {i}");
    }
}

// =============================================================================
// 3. CRC-32
// =============================================================================

#[wasm_bindgen_test]
fn test_crc32_empty() {
    // CRC-32 от пустых данных = 0 (после финального XOR с 0xFFFFFFFF).
    assert_eq!(crc32(&[]), 0);
}

#[wasm_bindgen_test]
fn test_crc32_known_vector_quick_fox() {
    // Эталон: "The quick brown fox jumps over the lazy dog" → 0x414FA339
    let data = b"The quick brown fox jumps over the lazy dog";
    assert_eq!(crc32(data), 0x414F_A339);
}

#[wasm_bindgen_test]
fn test_crc32_known_vector_123456789() {
    // Эталон CRC-32/IEEE: "123456789" → 0xCBF43926
    let data = b"123456789";
    assert_eq!(crc32(data), 0xCBF4_3926);
}

// =============================================================================
// 4. Code points
// =============================================================================

#[wasm_bindgen_test]
fn test_count_code_points_ascii() {
    assert_eq!(count_code_points("hello"), 5);
    assert_eq!(count_code_points(""), 0);
}

#[wasm_bindgen_test]
fn test_count_code_points_cyrillic() {
    // 6 code points, каждый в UTF-8 занимает 2 байта.
    assert_eq!(count_code_points("Привет"), 6);
}

#[wasm_bindgen_test]
fn test_count_code_points_emoji() {
    // 🦀 — 1 code point, 4 байта в UTF-8.
    assert_eq!(count_code_points("🦀"), 1);
    // "a🦀b" — 3 code points.
    assert_eq!(count_code_points("a🦀b"), 3);
}

// =============================================================================
// 5. CRC-16/CCITT-FALSE
// =============================================================================

#[wasm_bindgen_test]
fn test_crc16_known_vector() {
    // Эталон CRC-16/CCITT-FALSE: "123456789" → 0x29B1
    let data = b"123456789";
    assert_eq!(crc16_ccitt(data), 0x29B1);
}

#[wasm_bindgen_test]
fn test_crc16_empty() {
    // init = 0xFFFF, нет данных — возвращается init.
    assert_eq!(crc16_ccitt(&[]), 0xFFFF);
}

// =============================================================================
// 6. Протокол кадров
// =============================================================================

#[wasm_bindgen_test]
fn test_build_frame_layout() {
    let frame = build_frame(b"hi");
    // 55 AA 02 'h' 'i' crc_lo crc_hi
    assert_eq!(frame.len(), 7);
    assert_eq!(frame[0], 0x55);
    assert_eq!(frame[1], 0xAA);
    assert_eq!(frame[2], 2);
    assert_eq!(frame[3], b'h');
    assert_eq!(frame[4], b'i');
}

#[wasm_bindgen_test]
fn test_parse_frames_roundtrip() {
    let stream = simulate_stream(10, 16, 42);
    let frames = parse_frames(&stream);
    assert_eq!(frames.len(), 10);
    for f in &frames {
        assert_eq!(f.payload.len(), 16);
        assert!(f.crc_ok, "CRC должен быть валиден в сгенерированном потоке");
    }
}

#[wasm_bindgen_test]
fn test_parse_frames_empty() {
    assert!(parse_frames(&[]).is_empty());
}

#[wasm_bindgen_test]
fn test_parse_frames_truncated() {
    let mut stream = simulate_stream(3, 8, 7);
    // Обрезаем последние 3 байта — третий кадр станет неполным.
    stream.truncate(stream.len() - 3);
    let frames = parse_frames(&stream);
    assert_eq!(frames.len(), 2);
}

#[wasm_bindgen_test]
fn test_parse_frames_bad_header() {
    let mut stream = simulate_stream(1, 4, 1);
    stream[0] = 0x00;
    let frames = parse_frames(&stream);
    assert!(frames.is_empty());
}

#[wasm_bindgen_test]
fn test_parse_frames_crc_mismatch() {
    let mut stream = simulate_stream(1, 8, 3);
    // Портим последний байт — старший байт CRC.
    let n = stream.len();
    stream[n - 1] ^= 0xFF;
    let frames = parse_frames(&stream);
    assert_eq!(frames.len(), 1);
    assert!(!frames[0].crc_ok, "CRC должен не совпасть после порчи байта");
}

// =============================================================================
// 7. Генераторы данных
// =============================================================================

#[wasm_bindgen_test]
fn test_gen_bytes_deterministic() {
    let a = gen_bytes(100, 12345);
    let b = gen_bytes(100, 12345);
    assert_eq!(a, b);
}

#[wasm_bindgen_test]
fn test_gen_bytes_different_seeds() {
    let a = gen_bytes(100, 111);
    let b = gen_bytes(100, 222);
    assert_ne!(a, b);
}

#[wasm_bindgen_test]
fn test_gen_u32_array_deterministic() {
    let a = gen_u32_array(50, 999);
    let b = gen_u32_array(50, 999);
    assert_eq!(a, b);
}

#[wasm_bindgen_test]
fn test_gen_text_code_points() {
    let s = gen_text(1000, 7);
    assert_eq!(count_code_points(&s), 1000);
}

// =============================================================================
// 8. JS-обёртки совпадают с Rust-версиями
// =============================================================================

#[wasm_bindgen_test]
fn test_js_fib_matches_rust() {
    let f = compile_fib();
    for n in 0..25 {
        let js = call_fib(&f, n).expect("js fib failed");
        assert_eq!(js, fib_iterative(n), "fib({n}) mismatch");
    }
}

#[wasm_bindgen_test]
fn test_js_crc32_matches_rust() {
    let f = compile_crc32();
    let data = gen_bytes(1024, 42);
    let rust = crc32(&data);
    let js = call_crc32(&f, &data).expect("js crc32 failed");
    assert_eq!(js, rust);
}

#[wasm_bindgen_test]
fn test_js_count_code_points_matches_rust() {
    let f = compile_count_code_points();
    for text in ["hello", "Привет", "🦀", "a🦀b", ""] {
        let rust = count_code_points(text) as u32;
        let js = call_count_code_points(&f, text).expect("js count failed");
        assert_eq!(js, rust, "count mismatch for {text:?}");
    }
}

#[wasm_bindgen_test]
fn test_js_parse_frames_matches_rust() {
    let f = compile_parse_frames();
    let stream = simulate_stream(20, 16, 100);
    let rust_count = parse_frames(&stream).len() as u32;
    let js_count = call_parse_frames(&f, &stream).expect("js parse failed");
    assert_eq!(js_count, rust_count);
}