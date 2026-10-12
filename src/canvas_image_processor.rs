//! 21. Canvas 2D — image processor.
//!
//! Полный round-trip пикселей: `<img> → canvas → ImageData → Vec<u8>`
//! → Rust-обработка → `Vec<u8>` → `ImageData` → canvas.
//!
//! ## Про сигнатуры `getImageData` / `putImageData`
//!
//! В `web-sys 0.3.105` методы canvas имеют **две сигнатуры**,
//! переключаемые cfg-флагом `web_sys_unstable_apis`:
//!
//! | Режим | `getImageData` | `putImageData` |
//! |---|---|---|
//! | stable (по умолчанию) | `f64, f64, f64, f64` | `_, f64, f64` |
//! | unstable (`--cfg=web_sys_unstable_apis`) | `i32, i32, i32, i32` | `_, i32, i32` |
//!
//! Чтобы код компилировался в обоих режимах (host-тесты без флага,
//! wasm-сборка с флагом), вызовы завёрнуты в два хелпера:
//! `read_image_data` и `write_image_data`. Различие инкапсулировано
//! в одном месте — остальной код не знает про cfg.
//!
//! Модуль разделён на две части:
//! - **чистые функции** (`to_grayscale`, `invert`, `box_blur_3x3`, `sobel_edge`)
//!   работают с `&[u8]` / `Vec<u8>`, не знают про DOM, тестируются на хосте;
//! - **web-обвязка** (`load_image_from_blob`, `image_to_image_data`,
//!   `vec_to_image_data`, `draw_image_data_to_canvas`) — тонкий слой
//!   над Canvas API.

use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{
    Blob, CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement,
    ImageData, OffscreenCanvas, OffscreenCanvasRenderingContext2d, Url,
};

// =============================================================================
// Фильтры и чистые функции обработки пикселей
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Identity,
    Grayscale,
    Invert,
    Blur,
    Sobel,
}

impl Filter {
    pub fn label(self) -> &'static str {
        match self {
            Filter::Identity => "Оригинал",
            Filter::Grayscale => "Оттенки серого",
            Filter::Invert => "Инверсия",
            Filter::Blur => "Размытие",
            Filter::Sobel => "Границы (Sobel)",
        }
    }

    pub fn apply(self, rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
        match self {
            Filter::Identity => rgba.to_vec(),
            Filter::Grayscale => to_grayscale(rgba),
            Filter::Invert => invert(rgba),
            Filter::Blur => box_blur_3x3(rgba, w, h),
            Filter::Sobel => sobel_edge(rgba, w, h),
        }
    }
}

/// Luminance по Rec. 709: Y = 0.2126·R + 0.7152·G + 0.0722·B.
/// Коэффициенты умножены на 10000 для целочисленной арифметики.
pub fn to_grayscale(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let r = px[0] as u32;
        let g = px[1] as u32;
        let b = px[2] as u32;
        let y = ((r * 2126 + g * 7152 + b * 722) / 10000).min(255) as u8;
        out.push(y);
        out.push(y);
        out.push(y);
        out.push(px[3]);
    }
    out
}

/// Инверсия RGB, альфа не трогается.
pub fn invert(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.push(255 - px[0]);
        out.push(255 - px[1]);
        out.push(255 - px[2]);
        out.push(px[3]);
    }
    out
}

/// Box blur 3×3 с clamp'ом на краях.
pub fn box_blur_3x3(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let w = w as usize;
    let h = h as usize;
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                    let ny = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                    let i = (ny * w + nx) * 4;
                    r += rgba[i] as u32;
                    g += rgba[i + 1] as u32;
                    b += rgba[i + 2] as u32;
                    a += rgba[i + 3] as u32;
                    n += 1;
                }
            }
            let i = (y * w + x) * 4;
            out[i] = (r / n) as u8;
            out[i + 1] = (g / n) as u8;
            out[i + 2] = (b / n) as u8;
            out[i + 3] = (a / n) as u8;
        }
    }
    out
}

/// Sobel edge detection по яркости.
pub fn sobel_edge(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let w = w as usize;
    let h = h as usize;
    let mut out = vec![0u8; rgba.len()];

    let mut lum = vec![0u8; w * h];
    for i in 0..w * h {
        let p = i * 4;
        let r = rgba[p] as u32;
        let g = rgba[p + 1] as u32;
        let b = rgba[p + 2] as u32;
        lum[i] = ((r * 2126 + g * 7152 + b * 722) / 10000).min(255) as u8;
    }

    const GX: [i32; 9] = [-1, 0, 1, -2, 0, 2, -1, 0, 1];
    const GY: [i32; 9] = [-1, -2, -1, 0, 0, 0, 1, 2, 1];

    for y in 0..h {
        for x in 0..w {
            let (mut sx, mut sy) = (0i32, 0i32);
            let mut k = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                    let ny = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                    let l = lum[ny * w + nx] as i32;
                    sx += GX[k] * l;
                    sy += GY[k] * l;
                    k += 1;
                }
            }
            let mag = ((sx * sx + sy * sy) as f64).sqrt().min(255.0) as u8;
            let i = (y * w + x) * 4;
            out[i] = mag;
            out[i + 1] = mag;
            out[i + 2] = mag;
            out[i + 3] = 255;
        }
    }
    out
}

/// Встроенное «полотно» — цветные круги на градиенте.
pub fn make_sample_pixels(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    let cx1 = w as f32 * 0.30;
    let cy1 = h as f32 * 0.35;
    let cx2 = w as f32 * 0.72;
    let cy2 = h as f32 * 0.55;
    let cx3 = w as f32 * 0.50;
    let cy3 = h as f32 * 0.80;
    let r1 = (w as f32 * 0.12).powi(2);
    let r2 = (w as f32 * 0.14).powi(2);
    let r3 = (w as f32 * 0.10).powi(2);

    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let mut r = (40.0 + fx * 120.0) as u8;
            let mut g = (60.0 + fy * 120.0) as u8;
            let mut b = (200.0 - (fx + fy) * 60.0).max(40.0) as u8;

            let d1 = (x as f32 - cx1).powi(2) + (y as f32 - cy1).powi(2);
            let d2 = (x as f32 - cx2).powi(2) + (y as f32 - cy2).powi(2);
            let d3 = (x as f32 - cx3).powi(2) + (y as f32 - cy3).powi(2);
            if d1 < r1 { r = 240; g = 80; b = 60; }
            if d2 < r2 { r = 60; g = 200; b = 140; }
            if d3 < r3 { r = 100; g = 100; b = 240; }

            out.push(r);
            out.push(g);
            out.push(b);
            out.push(255);
        }
    }
    out
}

// =============================================================================
// Хелперы для несовместимости сигнатур: getImageData / putImageData
// =============================================================================

/// Обёртка над `OffscreenCanvasRenderingContext2d::get_image_data`.
///
/// При `--cfg=web_sys_unstable_apis` метод принимает `i32`,
/// без флага — `f64`. Хелпер скрывает это от вызывающего кода.
///
/// Всегда читает прямоугольник `(0, 0)..(w, h)` — именно так это
/// используется в нашем пайплайне.
#[cfg(web_sys_unstable_apis)]
fn read_image_data(
    ctx: &OffscreenCanvasRenderingContext2d,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(0, 0, w as i32, h as i32)
}

#[cfg(not(web_sys_unstable_apis))]
fn read_image_data(
    ctx: &OffscreenCanvasRenderingContext2d,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(0.0, 0.0, w as f64, h as f64)
}

/// Обёртка над `CanvasRenderingContext2d::put_image_data`.
///
/// При `--cfg=web_sys_unstable_apis` координаты `dx, dy` — `i32`,
/// без флага — `f64`. Всегда пишет в точку `(0, 0)`.
#[cfg(web_sys_unstable_apis)]
fn write_image_data(
    ctx: &CanvasRenderingContext2d,
    data: &ImageData,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, 0, 0)
}

#[cfg(not(web_sys_unstable_apis))]
fn write_image_data(
    ctx: &CanvasRenderingContext2d,
    data: &ImageData,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, 0.0, 0.0)
}

// =============================================================================
// Web-обвязка: <img> ↔ ImageData ↔ Vec<u8>
// =============================================================================

/// Загружает Blob как `<img>` через Object URL и ждёт `decode()`.
/// Object URL отзывается до возврата — картинка уже в памяти браузера.
pub async fn load_image_from_blob(blob: &Blob) -> Result<HtmlImageElement, JsValue> {
    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let img: HtmlImageElement = document
        .create_element("img")?
        .dyn_into::<HtmlImageElement>()?;

    let url = Url::create_object_url_with_blob(blob)?;
    img.set_src(&url);

    let result = wasm_bindgen_futures::JsFuture::from(img.decode()).await;
    let _ = Url::revoke_object_url(&url);
    result?;
    Ok(img)
}

/// Рисует `<img>` на OffscreenCanvas и возвращает ImageData + размеры.
/// Если сторона превышает `max_dim` — картинка масштабируется.
pub fn image_to_image_data(
    img: &HtmlImageElement,
    max_dim: u32,
) -> Result<(ImageData, u32, u32), JsValue> {
    let iw = img.natural_width();
    let ih = img.natural_height();
    if iw == 0 || ih == 0 {
        return Err(JsValue::from_str("image has zero size"));
    }
    let (w, h) = if iw.max(ih) > max_dim {
        let scale = max_dim as f64 / iw.max(ih) as f64;
        (
            (iw as f64 * scale).round().max(1.0) as u32,
            (ih as f64 * scale).round().max(1.0) as u32,
        )
    } else {
        (iw, ih)
    };

    let canvas = OffscreenCanvas::new(w, h)?;
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    let ctx: OffscreenCanvasRenderingContext2d = ctx_js.dyn_into()?;

    // drawImage — всегда f64 (не зависит от cfg-флага).
    ctx.draw_image_with_html_image_element_and_dw_and_dh(
        img, 0.0, 0.0, w as f64, h as f64,
    )?;

    // getImageData — тип аргументов зависит от cfg, скрыт в хелпере.
    let data = read_image_data(&ctx, w, h)?;
    Ok((data, w, h))
}

/// `ImageData::data()` в web-sys 0.3.105 возвращает `Clamped<Vec<u8>>` —
/// tuple struct с публичным полем `.0`. Просто забираем внутренний `Vec<u8>`.
pub fn image_data_to_vec(data: &ImageData) -> Vec<u8> {
    data.data().0
}

/// Обратный путь: `&[u8]` → `Clamped<&[u8]>` → `ImageData`.
///
/// `new_with_u8_clamped_array_and_sh` принимает `Clamped<&[u8]>` **по значению** —
/// без `&`. Сигнатура одинакова в обоих cfg-режимах.
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

/// Заменяет содержимое `HtmlCanvasElement` на `ImageData`.
/// Подгоняет размер холста под данные — иначе обрезка или растяжка.
pub fn draw_image_data_to_canvas(
    canvas: &HtmlCanvasElement,
    data: &ImageData,
) -> Result<(), JsValue> {
    canvas.set_width(data.width());
    canvas.set_height(data.height());
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d ctx"))?;
    let ctx: CanvasRenderingContext2d = ctx_js.dyn_into()?;
    // putImageData — тип dx/dy зависит от cfg, скрыт в хелпере.
    write_image_data(&ctx, data)
}