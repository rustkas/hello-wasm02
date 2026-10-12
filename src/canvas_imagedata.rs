//! 21.4. ImageData: пиксельный буфер и его границы.
//!
//! `ImageData` — плоский RGBA-буфер: `width × height × 4` байт.
//! Каждый пиксель — четыре байта в порядке `R, G, B, A`. Индекс
//! конкретного канала: `(y * w + x) * 4 + channel`, где `channel`:
//! 0 — R, 1 — G, 2 — B, 3 — A.
//!
//! В web-sys `ImageData::data()` возвращает `Clamped<Vec<u8>>` —
//! обёртку над `Vec<u8>`, которую можно взять через `.0`.
//!
//! ## Почему clamped
//!
//! `Uint8ClampedArray` (в отличие от `Uint8Array`) при переполнении
//! **насыщает** значение: `256 → 255`, `-1 → 0`. Не заворачивает по
//! модулю 256, а клампит. Это соответствует семантике цвета: пиксель
//! не может быть «ярче белого». В Rust мы работаем с `u8`, а
//! клампинг делаем вручную через `.clamp(0, 255)` или `.min(255)`.
//!
//! ## getImageData / putImageData
//!
//! Обе функции имеют **две сигнатуры**, переключаемые cfg-флагом
//! `web_sys_unstable_apis`: `f64` в стабильном режиме, `i32` в unstable.
//! Различие инкапсулировано в хелперах `read_image_data_cfg` и
//! `write_image_data_cfg` — остальной код их вызывает одинаково.
//!
//! ## putImageData игнорирует трансформации
//!
//! В отличие от `drawImage` и всех остальных методов рисования,
//! `putImageData` пишет **сырые пиксели** в буфер холста напрямую.
//! Он не смотрит ни на `translate`/`scale`/`rotate`, ни на
//! `globalAlpha`, ни на `globalCompositeOperation`. Это работает на
//! уровне «вот эти байты — туда», без всякой математики. Демо
//! показывает это через `scale(2, 2)`: `fillRect` увеличивается,
//! `putImageData` — нет.

use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use web_sys::{CanvasRenderingContext2d, ImageData};

// =============================================================================
// Индексация и чтение
// =============================================================================

/// Индекс байта канала в плоском RGBA-буфере.
///
/// `channel`: 0 — R, 1 — G, 2 — B, 3 — A.
/// Для `channel > 3` берётся 3 (альфа) — защита от опечаток.
pub fn pixel_index(x: u32, y: u32, w: u32, channel: u8) -> usize {
    let ch = (channel as u32).min(3);
    ((y * w + x) * 4 + ch) as usize
}

/// Читает RGBA в точке `(x, y)`. Возвращает `(r, g, b, a)`.
/// За пределами буфера — `(0, 0, 0, 0)`.
pub fn read_pixel(pixels: &[u8], x: u32, y: u32, w: u32) -> (u8, u8, u8, u8) {
    let i = pixel_index(x, y, w, 0);
    if i + 3 >= pixels.len() {
        return (0, 0, 0, 0);
    }
    (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3])
}

/// Читает один канал. Используется в «пипетке», когда нужен
/// конкретный байт, а не весь пиксель.
pub fn read_channel(pixels: &[u8], x: u32, y: u32, w: u32, channel: u8) -> u8 {
    let i = pixel_index(x, y, w, channel);
    *pixels.get(i).unwrap_or(&0)
}

// =============================================================================
// Преобразования каналов над плоским буфером
// =============================================================================

/// Инвертирует один канал: `v → 255 - v`. Остальные не трогает.
/// Применяется на месте — не выделяет новый `Vec`.
pub fn invert_channel_inplace(pixels: &mut [u8], channel: u8) {
    let ch = (channel as u32).min(3) as usize;
    let mut i = ch;
    while i < pixels.len() {
        pixels[i] = 255 - pixels[i];
        i += 4;
    }
}

/// Обнуляет один канал. Используется для демонстрации «что будет,
/// если R = 0» — цвет смещается в сторону cyan.
pub fn zero_channel_inplace(pixels: &mut [u8], channel: u8) {
    let ch = (channel as u32).min(3) as usize;
    let mut i = ch;
    while i < pixels.len() {
        pixels[i] = 0;
        i += 4;
    }
}

/// Заполняет буфер одним цветом.
pub fn fill_rgba(pixels: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    for px in pixels.chunks_exact_mut(4) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
        px[3] = a;
    }
}

// =============================================================================
// Генерация тестового паттерна
// =============================================================================

/// Демонстрационный паттерн `w × h`: плавный градиент с тремя
/// цветными квадратами по углам — чтобы «пипетка» показывала
/// разнообразные цвета при клике в разных точках.
pub fn make_pattern(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let wf = w as f32;
    let hf = h as f32;
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / wf;
            let fy = y as f32 / hf;
            let i = pixel_index(x, y, w, 0);

            // База — плавный градиент.
            let mut r = (30.0 + fx * 180.0) as u8;
            let mut g = (30.0 + fy * 180.0) as u8;
            let mut b = (200.0 - (fx + fy) * 60.0).max(40.0) as u8;

            // Красный квадрат в левом верхнем углу.
            if x < w / 5 && y < h / 5 {
                r = 240; g = 60; b = 60;
            }
            // Зелёный — в правом верхнем.
            if x > 4 * w / 5 && y < h / 5 {
                r = 60; g = 220; b = 80;
            }
            // Синий — в правом нижнем.
            if x > 4 * w / 5 && y > 4 * h / 5 {
                r = 60; g = 80; b = 240;
            }

            out[i] = r;
            out[i + 1] = g;
            out[i + 2] = b;
            out[i + 3] = 255;
        }
    }
    out
}

// =============================================================================
// ImageData ↔ Vec<u8>
// =============================================================================

/// `ImageData::data()` в web-sys 0.3.105 возвращает `Clamped<Vec<u8>>` —
/// tuple struct с публичным полем `.0`. Забираем внутренний `Vec<u8>`.
pub fn image_data_to_vec(data: &ImageData) -> Vec<u8> {
    data.data().0
}

/// Обратный путь: `&[u8]` → `Clamped<&[u8]>` → `ImageData`.
/// `Clamped` передаётся по значению, без `&`.
pub fn vec_to_image_data(pixels: &[u8], w: u32, h: u32) -> Result<ImageData, JsValue> {
    let expected = (w as usize) * (h as usize) * 4;
    if pixels.len() != expected {
        return Err(JsValue::from_str(&format!(
            "pixel buffer size {} != {} for {}x{}",
            pixels.len(), expected, w, h,
        )));
    }
    ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixels), w, h)
}

// =============================================================================
// Обёртки над getImageData / putImageData (cfg-гейт)
// =============================================================================
//
// В web-sys 0.3.105 методы имеют две сигнатуры:
//   stable  → getImageData(f64, f64, f64, f64); putImageData(_, f64, f64)
//   unstable → getImageData(i32, i32, i32, i32); putImageData(_, i32, i32)
// Хелперы скрывают различие. Остальной код их вызывает одинаково.

#[cfg(web_sys_unstable_apis)]
pub fn read_image_data_cfg(
    ctx: &CanvasRenderingContext2d,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(x as i32, y as i32, w as i32, h as i32)
}

#[cfg(not(web_sys_unstable_apis))]
pub fn read_image_data_cfg(
    ctx: &CanvasRenderingContext2d,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(x as f64, y as f64, w as f64, h as f64)
}

#[cfg(web_sys_unstable_apis)]
pub fn write_image_data_cfg(
    ctx: &CanvasRenderingContext2d,
    data: &ImageData,
    dx: u32,
    dy: u32,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, dx as i32, dy as i32)
}

#[cfg(not(web_sys_unstable_apis))]
pub fn write_image_data_cfg(
    ctx: &CanvasRenderingContext2d,
    data: &ImageData,
    dx: u32,
    dy: u32,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, dx as f64, dy as f64)
}