//! 23. WebCodecs.
//!
//! Демо: покадровая обработка видео. Main thread кодирует
//! синтетические кадры в H.264, Worker декодирует их, Rust
//! инвертирует цвета, Worker кодирует обратно.
//!
//! ## Где Rust
//!
//! WebCodecs — это JS API. Rust не может вызвать VideoDecoder
//! напрямую. Но Rust может обрабатывать VideoFrame между
//! декодером и энкодером — и это то, где WASM даёт выигрыш
//! по сравнению с JS-циклом по пикселям.
//!
//! ## Что экспортируется
//!
//! - `invert_rgba` — инверсия RGBA-байтов. Вызывается из Worker'а
//!   для каждого декодированного кадра.
//! - `is_webcodecs_supported` — feature detection.
//! - `post_*` — helpers для управления Worker'ом.
//!
//! ## Утилита `bool_of`
//!
//! rust-analyzer теряет тип `matches!` в tail-позиции функции
//! и выдаёт `expected bool, found ()`. Обёртка возвращает
//! типу конкретность.

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::prelude::*;
use web_sys::Worker;

// =============================================================================
// Утилиты для обхода багов rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `matches!` для tail-позиций.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

/// Обёртка над `format!` для tail-позиций.
#[inline]
fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
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

/// `VideoEncoder` доступен.
pub fn is_video_encoder_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("VideoEncoder")),
        Ok(v) if v.is_function()
    ))
}

/// `VideoDecoder` доступен.
pub fn is_video_decoder_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("VideoDecoder")),
        Ok(v) if v.is_function()
    ))
}

/// `VideoFrame` доступен.
pub fn is_video_frame_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("VideoFrame")),
        Ok(v) if v.is_function()
    ))
}

/// Полная поддержка WebCodecs (видео).
pub fn is_video_webcodecs_supported() -> bool {
    is_video_encoder_supported()
        && is_video_decoder_supported()
        && is_video_frame_supported()
}

/// Сводка поддержки — для статистики в UI.
pub fn support_summary() -> String {
    format_str(format_args!(
        "Worker: {w} · VideoEncoder: {e} · VideoDecoder: {d} · VideoFrame: {f}",
        w = if is_worker_supported() { "да" } else { "нет" },
        e = if is_video_encoder_supported() { "да" } else { "нет" },
        d = if is_video_decoder_supported() { "да" } else { "нет" },
        f = if is_video_frame_supported() { "да" } else { "нет" },
    ))
}

// =============================================================================
// Обработка пикселей — то, ради чего здесь Rust
// =============================================================================

/// Инвертирует RGB-каналы RGBA-изображения. Альфа не трогается.
///
/// Вызывается из Worker'а для каждого декодированного кадра.
/// JS-цикл по 2M пикселей занимает десятки миллисекунд;
/// Rust/WASM — единицы.
#[wasm_bindgen]
pub fn invert_rgba(data: &mut [u8]) {
    let n = data.len() & !3; // кратно 4
    let mut i = 0;
    while i < n {
        data[i] = 255 - data[i];
        data[i + 1] = 255 - data[i + 1];
        data[i + 2] = 255 - data[i + 2];
        // data[i + 3] — alpha, не трогаем
        i += 4;
    }
}

/// Считает средний цвет изображения — простая метрика
/// «кадр тёмный/светлый» для статистики.
#[wasm_bindgen]
pub fn average_rgb(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut sum: u64 = 0;
    let mut count: u64 = 0;
    let n = data.len() & !3;
    let mut i = 0;
    while i < n {
        sum += data[i] as u64 + data[i + 1] as u64 + data[i + 2] as u64;
        count += 3;
        i += 4;
    }
    if count == 0 {
        0.0
    } else {
        (sum as f64) / (count as f64)
    }
}

// =============================================================================
// Worker helpers
// =============================================================================

/// Создаёт module worker для обработки кадров.
pub fn create_worker(url: &str) -> Result<Worker, JsValue> {
    let opts = web_sys::WorkerOptions::new();
    opts.set_type(web_sys::WorkerType::Module);
    Worker::new_with_options(url, &opts)
}

/// Отправляет Worker'у «начать обработку»: конфиг кодека + режим.
///
/// `mode` — `"cpu"` (Rust invert) или `"gpu"` (WebGL2 shader).
pub fn post_start(
    worker: &Worker,
    codec: &str,
    width: u32,
    height: u32,
    frame_count: u32,
    mode: &str,
) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("start"))?;
    Reflect::set(&msg, &JsValue::from_str("codec"), &JsValue::from_str(codec))?;
    Reflect::set(&msg, &JsValue::from_str("width"), &JsValue::from_f64(width as f64))?;
    Reflect::set(&msg, &JsValue::from_str("height"), &JsValue::from_f64(height as f64))?;
    Reflect::set(
        &msg,
        &JsValue::from_str("frameCount"),
        &JsValue::from_f64(frame_count as f64),
    )?;
    Reflect::set(&msg, &JsValue::from_str("mode"), &JsValue::from_str(mode))?;
    worker.post_message(&msg)
}

/// Отправляет Worker'у закодированный чанк (transfer).
///
/// `buffer` — transferable. Владелец меняется: main thread теряет
/// доступ к буферу.
pub fn post_encoded_chunk(
    worker: &Worker,
    chunk_type: &str,
    timestamp_us: f64,
    buffer: &JsValue,
) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(
        &msg,
        &JsValue::from_str("type"),
        &JsValue::from_str("encoded_chunk"),
    )?;
    Reflect::set(
        &msg,
        &JsValue::from_str("chunkType"),
        &JsValue::from_str(chunk_type),
    )?;
    Reflect::set(
        &msg,
        &JsValue::from_str("timestamp"),
        &JsValue::from_f64(timestamp_us),
    )?;
    Reflect::set(&msg, &JsValue::from_str("buffer"), buffer)?;

    let transfer = Array::new();
    transfer.push(buffer);
    worker.post_message_with_transfer(&msg, &transfer)
}

/// Команда «финализировать» — Worker шлёт последний flush().
pub fn post_finish(worker: &Worker) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("finish"))?;
    worker.post_message(&msg)
}