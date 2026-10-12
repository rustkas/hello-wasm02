//! 23.3. VideoEncoder: распакованное → сжатое.
//!
//! Обёртки вокруг `VideoEncoder`:
//!
//! - конструктор с колбэками `output(chunk, metadata)` / `error`;
//! - `configure({ codec, width, height, bitrate, framerate })`;
//! - `encode(VideoFrame, { keyFrame })`, `flush()`, `close()`, `reset()`;
//! - статический `isConfigSupported(config)`;
//! - утилиты для чтения `EncodedVideoChunk` (type, byteLength, timestamp);
//! - утилиты для чтения `metadata.decoderConfig`
//!   (codec, codedWidth, codedHeight, description).
//!
//! ## Что нового по сравнению с 23.1/23.2
//!
//! 23.1 (`video_frame`) — что такое `VideoFrame` и `EncodedVideoChunk`.
//! 23.2 (`video_decoder`) — как подавать чанки в `VideoDecoder`.
//! 23.3 (`video_encoder`) — как `VideoEncoder` **производит** чанки:
//! размер key vs delta, влияние `bitrate`/`framerate`, содержимое
//! `metadata.decoderConfig`.
//!
//! ## Переиспользование хелперов
//!
//! Мини-хелперы для JsValue (`get_fn`, `construct`, `set_prop`,
//! `num`, `bool_of`) определены в `video_frame` (подглава 23.1) и
//! реэкспортируются здесь через `use`. Дублирования нет.

use js_sys::{Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::{
    bool_of, construct, get_fn, num, set_prop,
};

// =============================================================================
// Feature detection
// =============================================================================

/// `VideoEncoder` доступен в этом браузере.
pub fn is_video_encoder_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("VideoEncoder")),
        Ok(v) if v.is_function()
    ))
}

/// `VideoEncoder.isConfigSupported` доступен. Статический метод.
pub fn has_is_config_supported() -> bool {
    let global = js_sys::global();
    let Ok(ctor) = Reflect::get(&global, &JsValue::from_str("VideoEncoder")) else {
        return false;
    };
    bool_of(matches!(
        Reflect::get(&ctor, &JsValue::from_str("isConfigSupported")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// isConfigSupported
// =============================================================================

/// `await VideoEncoder.isConfigSupported(config)`.
///
/// Возвращает JS-объект `{ supported: bool, config: {...} }`.
/// Проверка до `configure` — браузер может отвергнуть кодек,
/// разрешение или bitrate, и это видно заранее.
pub async fn is_config_supported(config: &JsValue) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("VideoEncoder"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("VideoEncoder not available"));
    }
    let f = Reflect::get(&ctor, &JsValue::from_str("isConfigSupported"))?;
    if !f.is_function() {
        return Err(JsValue::from_str(
            "VideoEncoder.isConfigSupported not available",
        ));
    }
    let f = f.dyn_into::<js_sys::Function>()?;
    let promise = f.call1(&ctor, config)?.dyn_into::<Promise>()?;
    JsFuture::from(promise).await
}

// =============================================================================
// Конструктор
// =============================================================================

/// `new VideoEncoder({ output, error })`.
///
/// `output(chunk, metadata)` вызывается асинхронно для каждого
/// готового `EncodedVideoChunk`. `metadata.decoderConfig` есть
/// **только на первом key-кадре** — на delta он `undefined`.
pub fn make_video_encoder(
    on_output: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue, JsValue)>,
    on_error: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
) -> Result<JsValue, JsValue> {
    let init = Object::new();
    set_prop(&init, "output", on_output.as_ref())?;
    set_prop(&init, "error", on_error.as_ref())?;
    construct("VideoEncoder", &[init.into()])
}

// =============================================================================
// Configure / encode / flush / close / reset
// =============================================================================

/// `encoder.configure({ codec, width, height, bitrate, framerate })`.
///
/// Для avc1 encoder сам формирует `avcC`-box (SPS/PPS) и отдаёт его
/// в `metadata.decoderConfig.description` на первом key-кадре.
pub fn configure(
    encoder: &JsValue,
    codec: &str,
    width: u32,
    height: u32,
    bitrate: u32,
    framerate: f64,
) -> Result<(), JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "width", &num(width as f64))?;
    set_prop(&cfg, "height", &num(height as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;
    set_prop(&cfg, "framerate", &num(framerate))?;
    let f = get_fn(encoder, "configure")?;
    let _ = f.call1(encoder, &cfg)?;
    Ok(())
}

/// `encoder.encode(frame, { keyFrame })`.
///
/// `keyFrame: true` — принудительный IDR. Остальные кадры encoder
/// кодирует как delta (P-frame), если решает, что они помещаются в GOP.
pub fn encode(encoder: &JsValue, frame: &JsValue, key_frame: bool) -> Result<(), JsValue> {
    let opts = Object::new();
    if key_frame {
        set_prop(&opts, "keyFrame", &JsValue::TRUE)?;
    }
    let f = get_fn(encoder, "encode")?;
    let _ = f.call2(encoder, frame, &opts)?;
    Ok(())
}

/// `await encoder.flush()`.
///
/// Дожидается всех чанков в очереди и вызывает `output` для каждого.
/// После `flush()` все кадры гарантированно закодированы.
pub async fn flush(encoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(encoder, "flush")?;
    let promise = f.call0(encoder)?.dyn_into::<Promise>()?;
    JsFuture::from(promise).await.map(|_| ())
}

/// `encoder.close()`.
pub fn close(encoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(encoder, "close")?;
    let _ = f.call0(encoder)?;
    Ok(())
}

/// `encoder.reset()`.
///
/// Отменяет все pending `encode()` и `flush()`. Encoder возвращается
/// в состояние до `configure()`. Полезно, если пользователь
/// переключил bitrate на середине: вместо ожидания flush —
/// сбрасываем и начинаем заново.
pub fn reset(encoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(encoder, "reset")?;
    let _ = f.call0(encoder)?;
    Ok(())
}

// =============================================================================
// Чтение EncodedVideoChunk
// =============================================================================

/// `chunk.type` — `"key"` или `"delta"`.
///
/// Для не-`EncodedVideoChunk` возвращает пустую строку.
pub fn chunk_type(chunk: &JsValue) -> String {
    Reflect::get(chunk, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// `chunk.byteLength` — размер сжатых данных в байтах.
pub fn chunk_byte_length(chunk: &JsValue) -> usize {
    Reflect::get(chunk, &JsValue::from_str("byteLength"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as usize)
        .unwrap_or(0)
}

/// `chunk.timestamp` — временная метка в микросекундах.
pub fn chunk_timestamp_us(chunk: &JsValue) -> f64 {
    Reflect::get(chunk, &JsValue::from_str("timestamp"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// `chunk.duration` — длительность в микросекундах (или `null`).
pub fn chunk_duration_us(chunk: &JsValue) -> f64 {
    Reflect::get(chunk, &JsValue::from_str("duration"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

// =============================================================================
// Чтение metadata.decoderConfig
// =============================================================================

/// Извлекает `metadata.decoderConfig` из второго аргумента
/// `output`-колбэка encoder'а. Возвращает `None`, если поля
/// нет — это нормально для всех delta-кадров.
pub fn extract_decoder_config(metadata: &JsValue) -> Option<JsValue> {
    let cfg = Reflect::get(metadata, &JsValue::from_str("decoderConfig")).ok()?;
    if cfg.is_undefined() || cfg.is_null() {
        return None;
    }
    Some(cfg)
}

/// `decoderConfig.codec` — строка кодека, подтверждённая encoder'ом.
pub fn decoder_config_codec(cfg: &JsValue) -> String {
    Reflect::get(cfg, &JsValue::from_str("codec"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// `decoderConfig.codedWidth` — ширина в пикселях.
pub fn decoder_config_coded_width(cfg: &JsValue) -> u32 {
    Reflect::get(cfg, &JsValue::from_str("codedWidth"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `decoderConfig.codedHeight` — высота в пикселях.
pub fn decoder_config_coded_height(cfg: &JsValue) -> u32 {
    Reflect::get(cfg, &JsValue::from_str("codedHeight"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `decoderConfig.description` — `BufferSource` с `avcC`-box (SPS/PPS).
/// Возвращает `None`, если поле отсутствует или пустое.
pub fn decoder_config_description(cfg: &JsValue) -> Option<JsValue> {
    let d = Reflect::get(cfg, &JsValue::from_str("description")).ok()?;
    if d.is_undefined() || d.is_null() {
        return None;
    }
    Some(d)
}