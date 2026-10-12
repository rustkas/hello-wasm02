#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::wasm_in_worker::{
    box_blur_rgba, instance_id, is_image_bitmap_supported,
    is_module_worker_supported, is_worker_supported,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn worker_is_supported_in_browser() {
    assert!(is_worker_supported(), "Worker should be available");
}

#[wasm_bindgen_test]
fn module_worker_is_supported() {
    assert!(
        is_module_worker_supported(),
        "module workers should be available",
    );
}

#[wasm_bindgen_test]
fn image_bitmap_is_supported() {
    assert!(
        is_image_bitmap_supported(),
        "ImageBitmap should be available",
    );
}

#[wasm_bindgen_test]
fn instance_id_is_stable_per_instance() {
    let a = instance_id();
    let b = instance_id();
    assert_eq!(a, b, "instance_id must be stable");
    assert!(a.starts_with("wasm-"), "instance_id has prefix");
    assert_eq!(a.len(), 13, "wasm-XXXXXXXX — 13 chars");
}

#[wasm_bindgen_test]
fn box_blur_preserves_size() {
    let w = 8u32;
    let h = 8u32;
    let data = vec![100u8; (w * h * 4) as usize];
    let out = box_blur_rgba(&data, w, h, 1);
    assert_eq!(out.len(), data.len());
}

#[wasm_bindgen_test]
fn box_blur_of_uniform_is_uniform() {
    // Если все пиксели одинаковы, blur не меняет результат.
    let w = 4u32;
    let h = 4u32;
    let data = vec![128u8; (w * h * 4) as usize];
    let out = box_blur_rgba(&data, w, h, 3);
    assert!(out.iter().all(|&b| b == 128));
}

#[wasm_bindgen_test]
fn box_blur_smooths_an_edge() {
    // Создаём резкую границу: левая половина 0, правая 255.
    let w = 8u32;
    let h = 4u32;
    let mut data = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in (w / 2)..w {
            let idx = ((y * w + x) * 4) as usize;
            data[idx] = 255;
        }
    }
    let out = box_blur_rgba(&data, w, h, 1);
    // На границе (x = w/2 - 1 → x = w/2) значение должно быть промежуточным.
    let left_edge_idx = ((0 * w + (w / 2 - 1)) * 4) as usize;
    let right_edge_idx = ((0 * w + (w / 2)) * 4) as usize;
    assert!(out[left_edge_idx] > 0, "left edge receives some value");
    assert!(out[right_edge_idx] < 255, "right edge loses pure value");
}

#[wasm_bindgen_test]
fn box_blur_passes_zero_returns_input() {
    let w = 4u32;
    let h = 4u32;
    let mut data = vec![0u8; (w * h * 4) as usize];
    data[0] = 100;
    data[5] = 200;
    let out = box_blur_rgba(&data, w, h, 0);
    assert_eq!(out, data);
}