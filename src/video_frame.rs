//! 23.1. VideoFrame и EncodedVideoChunk.
//!
//! Два представления кадра в WebCodecs:
//!
//! - `VideoFrame` — распакованный кадр, память GPU. Обязательно
//!   закрывать через `close()`.
//! - `EncodedVideoChunk` — сжатый кадр, память CPU. В 10–100 раз
//!   меньше по объёму.
//!
//! ## Доступ к WebCodecs
//!
//! web-sys не экспортирует эти типы с нужными сигнатурами. Все
//! вызовы идут через `Reflect::get` + `Function::call*` +
//! `Reflect::construct`.
//!
//! ## Мини-хелперы
//!
//! Всё общение с JsValue сосредоточено в шести функциях:
//! `get_fn`, `invoke`, `construct`, `set_prop`, `get_str`, `get_f64`.
//! В `app.rs` они импортируются и не дублируются.

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

// =============================================================================
// Утилиты для обхода багов rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `matches!` для tail-позиций.
#[inline]
pub fn bool_of(value: bool) -> bool {
    value
}

/// Обёртка над `format!` для tail-позиций.
#[inline]
pub fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

// =============================================================================
// Мини-хелперы для JsValue
// =============================================================================

/// `target[name]` — как `Function`.
pub fn get_fn(target: &JsValue, name: &str) -> Result<Function, JsValue> {
    Reflect::get(target, &JsValue::from_str(name))?
        .dyn_into::<Function>()
        .map_err(|_| JsValue::from_str("not a function"))
}

/// `target.name(args)` — вызов метода.
pub fn invoke(target: &JsValue, name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f = get_fn(target, name)?;
    let arr = Array::new();
    for a in args {
        arr.push(a);
    }
    f.apply(target, &arr)
}

/// `new Ctor(...)` из глобального конструктора.
pub fn construct(ctor_name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str(ctor_name))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str(ctor_name));
    }
    let ctor = ctor.dyn_into::<Function>()?;
    let arr = Array::new();
    for a in args {
        arr.push(a);
    }
    Reflect::construct(&ctor, &arr)
}

/// `target[key] = value`.
pub fn set_prop(target: &JsValue, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(target, &JsValue::from_str(key), value).map(|_| ())
}

/// `target[key]` как `String`. Пустая строка, если нет.
pub fn get_str(target: &JsValue, key: &str) -> String {
    Reflect::get(target, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// `target[key]` как `f64`. `0.0`, если нет.
pub fn get_f64(target: &JsValue, key: &str) -> f64 {
    Reflect::get(target, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Короткий конструктор `JsValue::from_f64`.
pub fn num(v: f64) -> JsValue {
    JsValue::from_f64(v)
}

// =============================================================================
// Feature detection
// =============================================================================

/// `VideoFrame` доступен в этом браузере.
pub fn is_video_frame_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("VideoFrame")),
        Ok(v) if v.is_function()
    ))
}

/// `EncodedVideoChunk` доступен.
pub fn is_encoded_chunk_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("EncodedVideoChunk")),
        Ok(v) if v.is_function()
    ))
}

/// Сводка поддержки — для UI.
pub fn support_summary() -> String {
    format_str(format_args!(
        "VideoFrame: {f} · EncodedVideoChunk: {c}",
        f = if is_video_frame_supported() { "да" } else { "нет" },
        c = if is_encoded_chunk_supported() { "да" } else { "нет" },
    ))
}

// =============================================================================
// VideoFrame API
// =============================================================================

/// `new VideoFrame(canvas, { timestamp })`.
///
/// `canvas` приводится к `JsValue` явно — `HtmlCanvasElement`
/// реализует несколько `AsRef`, неявное `as_ref()` не работает.
pub fn create_from_canvas(
    canvas: &HtmlCanvasElement,
    timestamp_us: f64,
) -> Result<JsValue, JsValue> {
    let canvas_val = JsValue::from(canvas.clone());
    let init = Object::new();
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    construct("VideoFrame", &[canvas_val, init.into()])
}

/// Ширина буфера кадра (`codedWidth`).
pub fn width(frame: &JsValue) -> u32 {
    get_f64(frame, "codedWidth") as u32
}

/// Высота буфера кадра (`codedHeight`).
pub fn height(frame: &JsValue) -> u32 {
    get_f64(frame, "codedHeight") as u32
}

/// Timestamp в микросекундах.
pub fn timestamp(frame: &JsValue) -> f64 {
    get_f64(frame, "timestamp")
}

/// `frame.close()` — освобождает память GPU.
///
/// Идемпотентен: второй вызов не бросает.
pub fn close(frame: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(frame, "close")?;
    let _ = f.call0(frame)?;
    Ok(())
}

/// `frame.copyTo(buffer, { format: "RGBA" })` — извлечение пикселей.
///
/// Возвращает `Uint8Array` длины `width * height * 4`.
pub fn copy_to_rgba(frame: &JsValue) -> Result<Uint8Array, JsValue> {
    let size = (width(frame) * height(frame) * 4) as u32;
    let u8 = Uint8Array::new_with_length(size);
    let copy_to = get_fn(frame, "copyTo")?;
    let opts = Object::new();
    set_prop(&opts, "format", &JsValue::from_str("RGBA"))?;
    let _ = copy_to.call2(frame, u8.as_ref(), opts.as_ref())?;
    Ok(u8)
}

// =============================================================================
// EncodedVideoChunk API
// =============================================================================

/// `new EncodedVideoChunk({ type, timestamp, duration, data })`.
pub fn create_encoded_chunk(
    chunk_type: &str,
    timestamp_us: f64,
    duration_us: f64,
    data: &[u8],
) -> Result<JsValue, JsValue> {
    let u8 = Uint8Array::new_with_length(data.len() as u32);
    u8.copy_from(data);

    let init = Object::new();
    set_prop(&init, "type", &JsValue::from_str(chunk_type))?;
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    set_prop(&init, "duration", &num(duration_us))?;
    set_prop(&init, "data", u8.as_ref())?;

    construct("EncodedVideoChunk", &[init.into()])
}

/// Тип чанка: `"key"` или `"delta"`.
pub fn chunk_type(chunk: &JsValue) -> String {
    get_str(chunk, "type")
}

/// Timestamp чанка в микросекундах.
pub fn chunk_timestamp(chunk: &JsValue) -> f64 {
    get_f64(chunk, "timestamp")
}

/// Размер чанка в байтах.
pub fn chunk_byte_length(chunk: &JsValue) -> u32 {
    get_f64(chunk, "byteLength") as u32
}