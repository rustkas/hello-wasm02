//! 23.12. Video filters for the MP4 processing pipeline.
//!
//! Три фильтра над RGBA-байтами: grayscale, invert, sepia.
//! Плюс `None` — passthrough, полезен для round-trip проверки.
//!
//! ## Публичный API
//!
//! - `apply_filter(input, width, height, filter_id) -> Vec<u8>`
//! - Константы `FILTER_NONE`, `FILTER_GRAYSCALE`, `FILTER_INVERT`, `FILTER_SEPIA`
//!
//! ## Форматы
//!
//! Вход и выход — RGBA (4 байта на пиксель). Alpha сохраняется
//! без изменений ни одним из фильтров.

use wasm_bindgen::prelude::*;

// =============================================================================
// Константы фильтров
// =============================================================================

pub const FILTER_NONE: u32 = 0;
pub const FILTER_GRAYSCALE: u32 = 1;
pub const FILTER_INVERT: u32 = 2;
pub const FILTER_SEPIA: u32 = 3;

// =============================================================================
// Публичный API
// =============================================================================

/// Применяет фильтр к RGBA-буферу. Возвращает новый буфер.
///
/// ## Фильтры
///
/// - `FILTER_NONE` — возвращает копию входа.
/// - `FILTER_GRAYSCALE` — luma по Rec. 709: `0.299 R + 0.587 G + 0.114 B`.
/// - `FILTER_INVERT` — `255 - channel` для R, G, B.
/// - `FILTER_SEPIA` — классическая sepia-матрица.
///
/// ## Alpha
///
/// Ни один из фильтров не меняет alpha-канал. Это важно:
/// pipeline не работает с полупрозрачными кадрами (видео — всегда
/// непрозрачно), но `VideoFrame` может содержать alpha из соображений
/// формата, и мы обязаны его сохранить.
#[wasm_bindgen]
pub fn apply_filter(input: &[u8], width: u32, height: u32, filter_id: u32) -> Vec<u8> {
    let expected = (width * height * 4) as usize;
    let len = input.len().min(expected);
    let mut out = vec![0u8; len];

    let n = len / 4;
    for i in 0..n {
        let base = i * 4;
        let r = input[base] as f32;
        let g = input[base + 1] as f32;
        let b = input[base + 2] as f32;
        let a = input[base + 3];

        let (nr, ng, nb) = match filter_id {
            FILTER_GRAYSCALE => {
                let y = 0.299 * r + 0.587 * g + 0.114 * b;
                (y, y, y)
            }
            FILTER_INVERT => (255.0 - r, 255.0 - g, 255.0 - b),
            FILTER_SEPIA => {
                let nr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
                let ng = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
                let nb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
                (nr, ng, nb)
            }
            _ => (r, g, b),
        };

        out[base] = nr as u8;
        out[base + 1] = ng as u8;
        out[base + 2] = nb as u8;
        out[base + 3] = a;
    }

    return out;
}