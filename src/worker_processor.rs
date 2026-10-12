//! 23.8-W. WASM-функции для Dedicated Worker.
//!
//! Модуль без DOM — чистые вычисления над байтами RGBA.
//! Загружается в worker через `import ... from '../pkg/hello_wasm02.js'`.
//!
//! ## API
//!
//! - `process_grayscale(input, width, height) -> Vec<u8>` — grayscale
//!   по Rec. 709: `0.299*R + 0.587*G + 0.114*B`. Alpha сохраняется.
//! - `worker_ready() -> bool` — маркер: WASM инициализирован.

use wasm_bindgen::prelude::*;

/// Grayscale RGBA → RGBA.
///
/// Вход и выход — `Uint8Array` длиной `width * height * 4`.
/// Alpha сохраняется без изменений.
#[wasm_bindgen]
pub fn process_grayscale(input: &[u8], width: u32, height: u32) -> Vec<u8> {
    let expected = (width * height * 4) as usize;
    let len = input.len().min(expected);
    let mut output = vec![0u8; len];

    let n = len / 4;
    for i in 0..n {
        let base = i * 4;
        let r = input[base] as f32;
        let g = input[base + 1] as f32;
        let b = input[base + 2] as f32;
        let a = input[base + 3];
        let gray = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
        output[base] = gray;
        output[base + 1] = gray;
        output[base + 2] = gray;
        output[base + 3] = a;
    }
    output
}

/// Маркер готовности WASM в worker'е.
#[wasm_bindgen]
pub fn worker_ready() -> bool {
    true
}