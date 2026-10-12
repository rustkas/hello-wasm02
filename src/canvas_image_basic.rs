//! 21.Базовая обработка изображений: `<img>.decode()` + 5 фильтров + round-trip.
//!
//! Минимальный конвейер: `<img>` → canvas → `ImageData` → `Vec<u8>`
//! → Rust-фильтр → `Vec<u8>` → `ImageData` → canvas.
//!
//! Расширенная версия с EXIF-загрузкой и скачиванием — в
//! `canvas_image_pipeline` (глава 21.7).
//!
//! ## Сигнатуры getImageData / putImageData
//!
//! В web-sys 0.3.105 методы имеют две сигнатуры, переключаемые флагом
//! `web_sys_unstable_apis`: `f64` в stable, `i32` в unstable. Все вызовы
//! обёрнуты в cfg-хелперы `read_image_data_offscreen` и
//! `write_image_data_offscreen` — остальной код не знает про cfg.

use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{
    Blob, ImageBitmap, ImageData, OffscreenCanvas, OffscreenCanvasRenderingContext2d,
};

// =============================================================================
// Фильтры
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Original,
    Grayscale,
    Invert,
    Blur,
    Sobel,
}

impl Filter {
    pub fn label(self) -> &'static str {
        match self {
            Filter::Original => "Оригинал",
            Filter::Grayscale => "Оттенки серого",
            Filter::Invert => "Инверсия",
            Filter::Blur => "Размытие 3×3",
            Filter::Sobel => "Границы (Sobel)",
        }
    }

    pub fn apply(self, rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
        match self {
            Filter::Original => rgba.to_vec(),
            Filter::Grayscale => to_grayscale(rgba),
            Filter::Invert => invert(rgba),
            Filter::Blur => box_blur_3x3(rgba, w, h),
            Filter::Sobel => sobel_edge(rgba, w, h),
        }
    }
}

/// Luminance по Rec. 709. Альфа сохраняется без изменений.
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

/// Инверсия RGB. Альфа не трогается.
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

/// Синтетический паттерн — работает, когда файла нет.
pub fn make_sample_pixels(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let wf = w as f32;
    let hf = h as f32;
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / wf;
            let fy = y as f32 / hf;
            let i = ((y * w + x) * 4) as usize;

            let mut r = (40.0 + fx * 120.0) as u8;
            let mut g = (60.0 + fy * 120.0) as u8;
            let mut b = (200.0 - (fx + fy) * 60.0).max(40.0) as u8;

            let cx1 = wf * 0.30;
            let cy1 = hf * 0.35;
            let cx2 = wf * 0.72;
            let cy2 = hf * 0.55;
            let cx3 = wf * 0.50;
            let cy3 = hf * 0.80;
            let r1 = (wf * 0.12).powi(2);
            let r2 = (wf * 0.14).powi(2);
            let r3 = (wf * 0.10).powi(2);

            let d1 = (x as f32 - cx1).powi(2) + (y as f32 - cy1).powi(2);
            let d2 = (x as f32 - cx2).powi(2) + (y as f32 - cy2).powi(2);
            let d3 = (x as f32 - cx3).powi(2) + (y as f32 - cy3).powi(2);
            if d1 < r1 { r = 240; g = 80; b = 60; }
            if d2 < r2 { r = 60; g = 200; b = 140; }
            if d3 < r3 { r = 100; g = 100; b = 240; }

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

pub fn image_data_to_vec(data: &ImageData) -> Vec<u8> {
    data.data().0
}

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
// cfg-хелперы: getImageData / putImageData на OffscreenCanvas
// =============================================================================
//
// В web-sys 0.3.105 сигнатуры зависят от флага `web_sys_unstable_apis`:
//   stable   → getImageData(f64, f64, f64, f64); putImageData(_, f64, f64)
//   unstable → getImageData(i32, i32, i32, i32); putImageData(_, i32, i32)
// Оба варианта завёрнуты в хелперы, чтобы код компилировался в обоих режимах.

#[cfg(web_sys_unstable_apis)]
pub fn read_image_data_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(0, 0, w as i32, h as i32)
}

#[cfg(not(web_sys_unstable_apis))]
pub fn read_image_data_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(0.0, 0.0, w as f64, h as f64)
}

#[cfg(web_sys_unstable_apis)]
pub fn write_image_data_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    data: &ImageData,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, 0, 0)
}

#[cfg(not(web_sys_unstable_apis))]
pub fn write_image_data_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    data: &ImageData,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, 0.0, 0.0)
}

// =============================================================================
// Загрузка изображения → пиксели
// =============================================================================

/// Загружает Blob как `ImageBitmap` с EXIF-ориентацией, рисует на
/// OffscreenCanvas, читает `ImageData` и возвращает `(pixels, w, h)`.
///
/// Размеры клампятся до `max_dim` по длинной стороне. Это нужно,
/// чтобы 4000×3000 не превратился в 48 МБ буфера и не заморозил
/// вкладку на секунды.
pub async fn load_image_as_pixels(
    blob: &Blob,
    max_dim: u32,
) -> Result<(Vec<u8>, u32, u32), JsValue> {
    let bitmap = create_image_bitmap_with_exif(blob).await?;
    let iw = bitmap.width();
    let ih = bitmap.height();
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
    ctx.draw_image_with_image_bitmap_and_dw_and_dh(
        &bitmap, 0.0, 0.0, w as f64, h as f64,
    )?;
    bitmap.close();

    let data = read_image_data_offscreen(&ctx, w, h)?;
    Ok((image_data_to_vec(&data), w, h))
}

async fn create_image_bitmap_with_exif(blob: &Blob) -> Result<ImageBitmap, JsValue> {
    use js_sys::{Function, Object, Reflect};
    let global = js_sys::global();
    let func_value = Reflect::get(&global, &JsValue::from_str("createImageBitmap"))?;
    if !func_value.is_function() {
        return Err(JsValue::from_str("createImageBitmap not available"));
    }
    let func: Function = func_value.dyn_into()?;

    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("imageOrientation"),
        &JsValue::from_str("from-image"),
    )?;

    let promise: js_sys::Promise = func.call2(&global, blob.as_ref(), &opts)?.dyn_into()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    value.dyn_into::<ImageBitmap>()
}

// =============================================================================
// Пиксели → PNG Blob
// =============================================================================

/// Рисует `Vec<u8>` на временном canvas и конвертирует в PNG Blob.
pub async fn pixels_to_png_blob(pixels: &[u8], w: u32, h: u32) -> Result<Blob, JsValue> {
    let canvas = OffscreenCanvas::new(w, h)?;
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    let ctx: OffscreenCanvasRenderingContext2d = ctx_js.dyn_into()?;

    let data = vec_to_image_data(pixels, w, h)?;
    // ВАЖНО: используем cfg-хелпер, а не прямой вызов — сигнатура зависит
    // от `web_sys_unstable_apis`.
    write_image_data_offscreen(&ctx, &data)?;

    let promise = canvas.convert_to_blob()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    value.dyn_into::<Blob>()
}

// =============================================================================
// Скачивание — обёртка, чтобы demo не зависело от главы 20.3
// =============================================================================

/// Программный клик по невидимому `<a download>`.
pub fn trigger_anchor_download(blob: &Blob, filename: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let url = web_sys::Url::create_object_url_with_blob(blob)?;

    let anchor = document
        .create_element("a")?
        .dyn_into::<web_sys::HtmlAnchorElement>()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display: none;")?;

    let body = document.body().ok_or_else(|| JsValue::from_str("no body"))?;
    body.append_child(&anchor)?;
    anchor.click();
    body.remove_child(&anchor)?;

    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Проверка, доступен ли `showSaveFilePicker` в этом браузере.
///
/// Реализовано через явный `match`, а не `matches!` в tail-позиции:
/// rust-analyzer стабильно теряет тип `matches!` в этой позиции и
/// сообщает `expected bool, found ()`.
pub fn is_save_picker_supported() -> bool {
    let global = js_sys::global();
    match js_sys::Reflect::get(&global, &JsValue::from_str("showSaveFilePicker")) {
        Ok(v) => v.is_function(),
        Err(_) => false,
    }
}

/// «Сохранить как…» через File System Access API.
///
/// - `Ok(Some(name))` — сохранено, пользователь выбрал имя;
/// - `Ok(None)` — пользователь отменил;
/// - `Err(e)` — реальная ошибка.
pub async fn save_via_picker(
    blob: Blob,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    use js_sys::{Function, Object, Reflect};

    let global = js_sys::global();
    let picker = Reflect::get(&global, &JsValue::from_str("showSaveFilePicker"))?;
    let picker: Function = picker.dyn_into()?;

    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;

    let promise: js_sys::Promise = picker.call1(&global, &opts)?.dyn_into()?;
    let handle = match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(h) => h,
        Err(e) if is_abort_error(&e) => return Ok(None),
        Err(e) => return Err(e),
    };

    let writable_promise = call_method(&handle, "createWritable", &[])?;
    let writable = wasm_bindgen_futures::JsFuture::from(writable_promise).await?;

    let write_promise = call_method(&writable, "write", &[blob.as_ref()])?;
    wasm_bindgen_futures::JsFuture::from(write_promise).await?;

    let close_promise = call_method(&writable, "close", &[])?;
    wasm_bindgen_futures::JsFuture::from(close_promise).await?;

    let name = Reflect::get(&handle, &JsValue::from_str("name"))?;
    Ok(Some(name.as_string().unwrap_or_else(|| suggested_name.to_string())))
}

fn call_method(
    obj: &JsValue,
    name: &str,
    args: &[&JsValue],
) -> Result<js_sys::Promise, JsValue> {
    use js_sys::{Array, Function, Reflect};
    let func_js = Reflect::get(obj, &JsValue::from_str(name))?;
    if !func_js.is_function() {
        return Err(JsValue::from_str(&format!("{name} is not a function")));
    }
    let func: Function = func_js.dyn_into()?;
    let args_arr = Array::new();
    for a in args {
        args_arr.push(a);
    }
    let ret = func.apply(obj, &args_arr)?;
    ret.dyn_into::<js_sys::Promise>()
}

fn is_abort_error(e: &JsValue) -> bool {
    match js_sys::Reflect::get(e, &JsValue::from_str("name")) {
        Ok(v) => v.as_string().as_deref() == Some("AbortError"),
        Err(_) => false,
    }
}