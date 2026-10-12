//! Host-тесты модуля worker_core: конфигурация и модель протокола.

use hello_wasm02::worker_pipeline::{box_blur, grayscale, Filter, PipelineTask};

#[test]
fn grayscale_converts_rgba() {
    let pixels = vec![255, 0, 0, 255, 0, 255, 0, 255];
    let gray = grayscale(&pixels);
    assert_eq!(gray.len(), pixels.len());
    // Красный → серый (не 0 и не 255).
    assert!(gray[0] > 0 && gray[0] < 255);
    // Зелёный → серый.
    assert!(gray[4] > 0 && gray[4] < 255);
}

#[test]
fn grayscale_preserves_alpha() {
    let pixels = vec![100, 100, 100, 128];
    let gray = grayscale(&pixels);
    assert_eq!(gray[3], 128);
}

#[test]
fn box_blur_radius_zero_is_identity() {
    let pixels = vec![10, 20, 30, 40, 50, 60, 70, 80];
    let blurred = box_blur(&pixels, 2, 1, 0);
    assert_eq!(blurred, pixels);
}

#[test]
fn box_blur_smooths_edges() {
    // Изображение 3x1: три пикселя с разными значениями.
    let pixels = vec![0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255];
    let blurred = box_blur(&pixels, 3, 1, 1);
    // Средний пиксель должен стать средним между соседями.
    assert!(blurred[4] > 0 && blurred[4] < 255);
}

#[test]
fn filter_variants_are_distinct() {
    assert_ne!(Filter::Grayscale, Filter::BoxBlur);
}

#[test]
fn pipeline_task_construction() {
    let task = PipelineTask {
        filter: Filter::Grayscale,
        width: 100,
        height: 50,
    };
    assert_eq!(task.width, 100);
    assert_eq!(task.height, 50);
}