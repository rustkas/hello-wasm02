//! 22.6. Mandelbrot fractal renderer.
//!
//! Экспортирует `render_mandelbrot_chunk` — считает одну горизонтальную
//! полосу фрактала. Worker вызывает её по чанкам, между вызовами
//! отдаёт управление event loop'у.
//!
//! ## Почему `&mut [u8]`
//!
//! wasm-bindgen копирует срез из JS в WASM-память и обратно. Для
//! чанка 12 строк × 480 px × 4 байта это ~23 КБ — копейки.
//! Chunked-обработка выигрывает за счёт yield'ов, а не за счёт
//! отсутствия копирования.

use js_sys::Reflect;
use wasm_bindgen::prelude::*;

/// Обёртка над `matches!` для tail-позиций. rust-analyzer теряет тип
/// `matches!` в хвостовой позиции функции и выдаёт `expected bool, found ()`.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Feature detection
// =============================================================================

/// `Worker` доступен в этом браузере.
pub fn is_worker_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("Worker")),
        Ok(v) if v.is_function()
    ))
}


/// `OffscreenCanvas` доступен.
pub fn is_offscreen_canvas_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("OffscreenCanvas")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// Ядро: escape-time для одной точки
// =============================================================================

/// Возвращает число итераций до выхода за радиус 2.
///
/// Если точка внутри множества — возвращает `max_iter`.
pub fn escape(cx: f64, cy: f64, max_iter: u32) -> u32 {
    let mut zx: f64 = 0.0;
    let mut zy: f64 = 0.0;
    let mut i: u32 = 0;
    while i < max_iter {
        let zx2 = zx * zx;
        let zy2 = zy * zy;
        if zx2 + zy2 > 4.0 {
            break;
        }
        // Вычисляем новые значения через временные, чтобы не затирать zx, zy.
        let nzx = zx2 - zy2 + cx;
        let nzy = 2.0 * zx * zy + cy;
        zx = nzx;
        zy = nzy;
        i += 1;
    }
    i
}

// =============================================================================
// Палитры
// =============================================================================

/// Палитра для одного пикселя. `iter` — число итераций, `max_iter` —
/// максимум (граница между «внутри» и «снаружи»).
fn palette_color(iter: u32, max_iter: u32, palette: u32) -> (u8, u8, u8) {
    if iter >= max_iter {
        // Внутри множества — чёрный.
        return (0, 0, 0);
    }

    // Нормализуем [0, 1).
    let t = (iter as f64) / (max_iter as f64);

    match palette {
        // Fire: тёмно-красный → жёлтый.
        0 => {
            let r = 255.0_f64.min(255.0 * (t * 3.0));
            let g = 255.0_f64.min(255.0 * (t * 3.0 - 0.5).max(0.0));
            let b = 0.0_f64.max(255.0 * (t * 3.0 - 2.0));
            (r as u8, g as u8, b as u8)
        }
        // Ocean: тёмно-синий → голубой.
        1 => {
            let r = 255.0_f64.min(255.0 * (t * 2.0 - 1.0).max(0.0));
            let g = 255.0_f64.min(255.0 * (t * 1.5));
            let b = 255.0_f64.min(255.0 * (1.0 - t * 0.3));
            (r as u8, g as u8, b as u8)
        }
        // Grayscale: чёрный → белый.
        _ => {
            let v = (255.0 * t) as u8;
            (v, v, v)
        }
    }
}

// =============================================================================
// Рендер чанка
// =============================================================================

/// Считает одну горизонтальную полосу фрактала.
///
/// - `out` — RGBA-буфер чанка, размер = `chunk_h * w * 4`.
/// - `w` — ширина всего изображения.
/// - `h_total` — высота всего изображения (нужна для центра).
/// - `y_start` — глобальный y первой строки чанка.
/// - `cx`, `cy` — центр вида в комплексной плоскости.
/// - `pixel_size` — размер одного пикселя в комплексной плоскости.
/// - `max_iter` — максимум итераций.
/// - `palette` — 0 (fire) / 1 (ocean) / 2 (grayscale).
#[wasm_bindgen]
pub fn render_mandelbrot_chunk(
    out: &mut [u8],
    w: u32,
    h_total: u32,
    y_start: u32,
    cx: f64,
    cy: f64,
    pixel_size: f64,
    max_iter: u32,
    palette: u32,
) {
    let w_usize = w as usize;
    let h_total_f = h_total as f64;
    let w_f = w as f64;
    let y_start_f = y_start as f64;

    // Сколько строк в этом чанке.
    let row_bytes = w_usize * 4;
    if row_bytes == 0 || out.len() < row_bytes {
        return;
    }
    let chunk_h = out.len() / row_bytes;

    for ly in 0..chunk_h {
        let gy = y_start_f + ly as f64;
        let world_y = cy + (gy - h_total_f / 2.0) * pixel_size;
        let row_base = ly * row_bytes;

        for x in 0..w_usize {
            let world_x = cx + (x as f64 - w_f / 2.0) * pixel_size;
            let iters = escape(world_x, world_y, max_iter);
            let (r, g, b) = palette_color(iters, max_iter, palette);

            let idx = row_base + x * 4;
            out[idx] = r;
            out[idx + 1] = g;
            out[idx + 2] = b;
            out[idx + 3] = 255;
        }
    }
}

// =============================================================================
// Worker helpers
// =============================================================================

/// Создаёт Worker из URL. Для простоты — классический (не module).
pub fn create_worker(url: &str) -> Result<web_sys::Worker, JsValue> {
    web_sys::Worker::new(url)
}

/// Отправляет `OffscreenCanvas` Worker'у.
pub fn post_init_canvas(
    worker: &web_sys::Worker,
    canvas: &web_sys::OffscreenCanvas,
    width: u32,
    height: u32,
) -> Result<(), JsValue> {
    let msg = js_sys::Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("init_canvas"))?;
    Reflect::set(&msg, &JsValue::from_str("canvas"), canvas.as_ref())?;
    Reflect::set(&msg, &JsValue::from_str("width"), &JsValue::from_f64(width as f64))?;
    Reflect::set(&msg, &JsValue::from_str("height"), &JsValue::from_f64(height as f64))?;

    let transfer = js_sys::Array::new();
    transfer.push(canvas.as_ref());
    worker.post_message_with_transfer(&msg, &transfer)
}

/// Отправляет запрос на рендер.
pub fn post_render(
    worker: &web_sys::Worker,
    task_id: u32,
    cx: f64,
    cy: f64,
    pixel_size: f64,
    max_iter: u32,
    palette: u32,
) -> Result<(), JsValue> {
    let msg = js_sys::Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("render"))?;
    Reflect::set(&msg, &JsValue::from_str("task_id"), &JsValue::from_f64(task_id as f64))?;
    Reflect::set(&msg, &JsValue::from_str("cx"), &JsValue::from_f64(cx))?;
    Reflect::set(&msg, &JsValue::from_str("cy"), &JsValue::from_f64(cy))?;
    Reflect::set(&msg, &JsValue::from_str("pixel_size"), &JsValue::from_f64(pixel_size))?;
    Reflect::set(&msg, &JsValue::from_str("max_iter"), &JsValue::from_f64(max_iter as f64))?;
    Reflect::set(&msg, &JsValue::from_str("palette"), &JsValue::from_f64(palette as f64))?;
    worker.post_message(&msg)
}

/// Отправляет запрос на отмену рендера.
pub fn post_cancel(worker: &web_sys::Worker, task_id: u32) -> Result<(), JsValue> {
    let msg = js_sys::Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("cancel"))?;
    Reflect::set(&msg, &JsValue::from_str("task_id"), &JsValue::from_f64(task_id as f64))?;
    worker.post_message(&msg)
}