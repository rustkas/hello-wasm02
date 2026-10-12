//! Host-тесты модуля sab_core: чистая логика обработки аудио.
//! Никаких JsValue/ArrayBuffer здесь быть не должно.

use hello_wasm02::sab_pipeline::{apply_effect_copy, AudioEffect};

#[test]
fn invert_negates_values() {
    let data = vec![1.0, -2.0, 0.5];
    let out = apply_effect_copy(&data, AudioEffect::Invert);
    assert_eq!(out, vec![-1.0, 2.0, -0.5]);
}

#[test]
fn attenuate_scales_values() {
    let data = vec![1.0, 2.0, -4.0];
    let out = apply_effect_copy(&data, AudioEffect::Attenuate(0.5));
    assert_eq!(out, vec![0.5, 1.0, -2.0]);
}

#[test]
fn offset_shifts_values() {
    let data = vec![0.0, 1.0, -1.0];
    let out = apply_effect_copy(&data, AudioEffect::Offset(0.1));
    for (a, b) in out.iter().zip([0.1, 1.1, -0.9]) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
fn empty_input_returns_empty() {
    let out = apply_effect_copy(&[], AudioEffect::Invert);
    assert!(out.is_empty());
}

#[test]
fn effect_variants_are_distinct() {
    assert_ne!(AudioEffect::Invert, AudioEffect::Offset(0.0));
    assert_ne!(AudioEffect::Attenuate(0.5), AudioEffect::Offset(0.5));
}