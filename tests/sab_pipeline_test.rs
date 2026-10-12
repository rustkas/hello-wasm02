//! Wasm-тесты pipeline: typed arrays, bulk vs elementwise, SAB-проверки.
//!
//! Соответствие с `sab_pipeline.rs`:
//!   • `process_audio_bulk`       — bulk через Vec<f32>, рекомендуемый путь.
//!   • `process_audio_elementwise` — поэлементно через JS typed array,
//!     анти-пример (2N пересечений границы JS↔WASM).
//!   • `bench_bulk` / `bench_elementwise` — замеры для сравнения.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::sab_core;
use hello_wasm02::sab_pipeline;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- Typed arrays: копирование ----------------

#[wasm_bindgen_test]
fn uint8_roundtrip_via_copy() {
    let data = vec![1u8, 2, 3, 4, 5];
    let view = sab_core::bytes_to_copy(&data);
    assert_eq!(view.length(), 5);
    let back = sab_core::copy_from_uint8(&view);
    assert_eq!(back, data);
}

#[wasm_bindgen_test]
fn float32_roundtrip_via_copy() {
    let data = vec![1.0f32, -2.5, 3.25];
    let view = sab_core::f32_to_copy(&data);
    assert_eq!(view.length(), 3);
    let back = sab_core::copy_from_f32(&view);
    assert_eq!(back, data);
}

// ---------------- Bulk path (рекомендуемый) ----------------

#[wasm_bindgen_test]
fn bulk_returns_new_vec() {
    let data = vec![1.0f32, -2.0, 3.0];
    let out = sab_pipeline::process_audio_bulk(data.clone(), 0); // Invert
    assert_eq!(out, vec![-1.0, 2.0, -3.0]);
    // Исходный data не изменён.
    assert_eq!(data, vec![1.0, -2.0, 3.0]);
}

#[wasm_bindgen_test]
fn bulk_attenuate() {
    let data = vec![2.0f32, -4.0, 0.0];
    let out = sab_pipeline::process_audio_bulk(data, 1); // Attenuate(0.5)
    assert_eq!(out, vec![1.0, -2.0, 0.0]);
}

#[wasm_bindgen_test]
fn bulk_offset() {
    let data = vec![0.0f32, 1.0];
    let out = sab_pipeline::process_audio_bulk(data, 2); // Offset(0.1)
    for (a, b) in out.iter().zip([0.1f32, 1.1]) {
        assert!((a - b).abs() < 1e-6);
    }
}

// ---------------- Elementwise path (анти-пример) ----------------

#[wasm_bindgen_test]
fn elementwise_modifies_view_in_place() {
    let data = vec![1.0f32, -2.0, 3.0];
    let view = sab_core::f32_to_copy(&data);
    sab_pipeline::process_audio_elementwise(&view, 0); // Invert
    // JS видит изменения сразу — буфер тот же.
    assert_eq!(view.get_index(0), -1.0);
    assert_eq!(view.get_index(1), 2.0);
    assert_eq!(view.get_index(2), -3.0);
}

// ---------------- Бенчмарки ----------------

#[wasm_bindgen_test]
fn bench_bulk_returns_positive_ms() {
    let data = vec![0.5f32; 1024];
    let ms = sab_pipeline::bench_bulk(data, 10);
    assert!(ms >= 0.0, "bench_bulk вернул отрицательное время: {ms}");
}

#[wasm_bindgen_test]
fn bench_elementwise_returns_positive_ms() {
    let view = sab_pipeline::make_sine_view(1024, 3.0);
    let ms = sab_pipeline::bench_elementwise(&view, 10);
    assert!(ms >= 0.0, "bench_elementwise вернул отрицательное время: {ms}");
}

// ---------------- Прочее ----------------

#[wasm_bindgen_test]
fn make_shared_or_plain_works_without_coop() {
    // Если COOP/COEP не настроены — вернётся ArrayBuffer-backed Uint8Array.
    // Если настроены — SharedArrayBuffer-backed. Оба варианта валидны.
    let view = sab_core::make_shared_or_plain(16).expect("make buffer");
    assert_eq!(view.length(), 16);
}

#[wasm_bindgen_test]
fn env_info_reports_isolation() {
    let info = sab_core::env_info();
    // Просто проверяем, что вызов не падает и значения консистентны.
    assert_eq!(
        info.cross_origin_isolated,
        info.shared_array_buffer_available
    );
}

#[wasm_bindgen_test]
fn sine_view_has_correct_length() {
    let view = sab_pipeline::make_sine_view(100, 5.0);
    assert_eq!(view.length(), 100);
}