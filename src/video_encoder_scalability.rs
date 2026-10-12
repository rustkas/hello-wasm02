//! 23.14. Temporal scalability: L1T2, L1T3.
//!
//! Обёртки для VideoEncoder с `scalabilityMode`:
//!
//! - `check_scalability_config` — isConfigSupported с scalabilityMode.
//! - `configure_scalability` — configure с scalabilityMode.
//! - `extract_temporal_layer_id` — чтение metadata.svc.temporalLayerId.
//!
//! ## Что нового по сравнению с 23.3/23.4
//!
//! 23.3 (`video_encoder`) — базовые обёртки.
//! 23.4 (`video_encoder_tuning`) — подсказки кодеку.
//! 23.14 (`video_encoder_scalability`) — временные слои и дроп лучей.
//!
//! ## Переиспользование
//!
//! Базовые обёртки `make_video_encoder`, `encode`, `flush`, `close`
//! из `video_encoder::*`. Хелперы `num`, `set_prop`, `get_fn`,
//! `format_str` из `video_frame::*`.

use js_sys::{Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::{format_str, get_fn, num, set_prop};

// =============================================================================
// Константы
// =============================================================================

/// Все доступные режимы temporal scalability для L1.
pub const SCALABILITY_MODES: &[(&str, u32)] = &[
    ("L1T1", 1),
    ("L1T2", 2),
    ("L1T3", 3),
];

/// Человекочитаемое имя режима.
pub fn mode_label(mode: &str) -> String {
    match mode {
        "L1T1" => format_str(format_args!("L1T1 (no scaling)")),
        "L1T2" => format_str(format_args!("L1T2 (2 layers)")),
        "L1T3" => format_str(format_args!("L1T3 (3 layers)")),
        _ => format_str(format_args!("{mode}")),
    }
}

/// Количество временных слоёв в режиме.
pub fn mode_layers(mode: &str) -> u32 {
    match mode {
        "L1T1" => 1,
        "L1T2" => 2,
        "L1T3" => 3,
        _ => 1,
    }
}

// =============================================================================
// isConfigSupported
// =============================================================================

/// `await VideoEncoder.isConfigSupported({ codec, width, height,
/// bitrate, framerate, scalabilityMode })`.
pub async fn check_scalability_config(
    codec: &str,
    width: u32,
    height: u32,
    bitrate: u32,
    framerate: f64,
    scalability_mode: &str,
) -> Result<bool, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("VideoEncoder"))?;
    if !ctor.is_function() {
        return Ok(false);
    }
    let is_cs = Reflect::get(&ctor, &JsValue::from_str("isConfigSupported"))?;
    if !is_cs.is_function() {
        return Ok(false);
    }
    let is_cs = is_cs.dyn_into::<js_sys::Function>()?;

    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "width", &num(width as f64))?;
    set_prop(&cfg, "height", &num(height as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;
    set_prop(&cfg, "framerate", &num(framerate))?;
    set_prop(&cfg, "scalabilityMode", &JsValue::from_str(scalability_mode))?;

    let promise = is_cs.call1(&ctor, &cfg)?.dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;
    Ok(Reflect::get(&result, &JsValue::from_str("supported"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

// =============================================================================
// configure
// =============================================================================

/// `encoder.configure({ codec, width, height, bitrate, framerate,
/// scalabilityMode })`.
pub fn configure_scalability(
    encoder: &JsValue,
    codec: &str,
    width: u32,
    height: u32,
    bitrate: u32,
    framerate: f64,
    scalability_mode: &str,
) -> Result<(), JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "width", &num(width as f64))?;
    set_prop(&cfg, "height", &num(height as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;
    set_prop(&cfg, "framerate", &num(framerate))?;
    set_prop(&cfg, "scalabilityMode", &JsValue::from_str(scalability_mode))?;
    let f = get_fn(encoder, "configure")?;
    let _ = f.call1(encoder, &cfg)?;
    Ok(())
}

// =============================================================================
// Чтение metadata.svc
// =============================================================================

/// Извлекает `metadata.svc.temporalLayerId` из второго аргумента
/// output-колбэка. Возвращает `None`, если поля нет — это нормально
/// для L1T1 (один слой).
pub fn extract_temporal_layer_id(metadata: &JsValue) -> Option<u32> {
    let svc = Reflect::get(metadata, &JsValue::from_str("svc")).ok()?;
    if svc.is_undefined() || svc.is_null() {
        return None;
    }
    let id = Reflect::get(&svc, &JsValue::from_str("temporalLayerId")).ok()?;
    id.as_f64().map(|n| n as u32)
}

/// Форматирует распределение чанков по слоям для UI.
pub fn format_layer_distribution(counts: &[u32; 3]) -> String {
    return format_str(format_args!(
        "T0: {} · T1: {} · T2: {}",
        counts[0], counts[1], counts[2],
    ));
}

/// Проверяет, поддерживается ли temporal scalability вообще в браузере.
/// Возвращает true, если L1T3 доступен с заданным кодеком.
pub async fn is_temporal_scalability_supported(
    codec: &str,
    width: u32,
    height: u32,
    bitrate: u32,
    framerate: f64,
) -> Result<bool, JsValue> {
    return check_scalability_config(codec, width, height, bitrate, framerate, "L1T3").await;
}