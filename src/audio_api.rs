//! 23.13. AudioWorklet + WebCodecs audio API wrappers.
//!
//! Всё общение с JsValue — через Reflect. Мини-хелперы — из `video_frame::*`.

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioWorkletNode, MessagePort};

use crate::video_frame::{format_str, get_fn, num, set_prop};

// =============================================================================
// Feature detection
// =============================================================================

pub fn has_audio_worklet(ctx: &AudioContext) -> bool {
    let ctx_js: &JsValue = ctx.as_ref();
    let Ok(aw) = Reflect::get(ctx_js, &JsValue::from_str("audioWorklet")) else {
        return false;
    };
    !aw.is_undefined() && !aw.is_null()
}

pub fn is_audio_encoder_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("AudioEncoder"))
        .map(|v| v.is_function()).unwrap_or(false)
}

pub fn is_audio_decoder_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("AudioDecoder"))
        .map(|v| v.is_function()).unwrap_or(false)
}

pub fn is_audio_data_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("AudioData"))
        .map(|v| v.is_function()).unwrap_or(false)
}

pub fn has_get_user_media() -> bool {
    let global = js_sys::global();
    let Ok(nav) = Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    let Ok(md) = Reflect::get(&nav, &JsValue::from_str("mediaDevices")) else {
        return false;
    };
    Reflect::get(&md, &JsValue::from_str("getUserMedia"))
        .map(|v| v.is_function()).unwrap_or(false)
}

// =============================================================================
// AudioWorklet
// =============================================================================

/// `await ctx.audioWorklet.addModule(url)`.
pub async fn add_worklet_module(ctx: &AudioContext, url: &str) -> Result<(), JsValue> {
    let ctx_js: &JsValue = ctx.as_ref();
    let aw = Reflect::get(ctx_js, &JsValue::from_str("audioWorklet"))?;
    let add = get_fn(&aw, "addModule")?;
    let promise = add.call1(&aw, &JsValue::from_str(url))?
        .dyn_into::<Promise>()?;
    JsFuture::from(promise).await.map(|_| ())
}

/// Создаёт AudioWorkletNode с разумными опциями для 1 канала.
pub fn create_worklet_node(
    ctx: &AudioContext,
    name: &str,
    channels: u32,
) -> Result<AudioWorkletNode, JsValue> {
    let opts = Object::new();
    set_prop(&opts, "numberOfInputs", &num(1.0))?;
    set_prop(&opts, "numberOfOutputs", &num(1.0))?;
    let ch_arr = Array::new();
    ch_arr.push(&num(channels as f64));
    set_prop(&opts, "outputChannelCount", &ch_arr)?;
    set_prop(&opts, "channelCount", &num(channels as f64))?;
    set_prop(&opts, "channelCountMode", &JsValue::from_str("explicit"))?;
    set_prop(&opts, "channelInterpretation", &JsValue::from_str("speakers"))?;

    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioWorkletNode"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("AudioWorkletNode not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let args = Array::new();
    args.push(ctx.as_ref());
    args.push(&JsValue::from_str(name));
    args.push(&opts);
    let node = Reflect::construct(&ctor, &args)?;
    node.dyn_into::<AudioWorkletNode>()
}

/// Порт для управления worklet'ом. Возвращает `Result`, потому что
/// в web-sys 0.3.105+ `AudioWorkletNode::port()` возвращает
/// `Result<MessagePort, JsValue>`.
pub fn node_port(node: &AudioWorkletNode) -> Result<MessagePort, JsValue> {
    return node.port();
}

/// Отправляет `{type: "ratio", value: X}` в worklet.
pub fn post_ratio(port: &MessagePort, ratio: f32) -> Result<(), JsValue> {
    let msg = Object::new();
    set_prop(&msg, "type", &JsValue::from_str("ratio"))?;
    set_prop(&msg, "value", &num(ratio as f64))?;
    let msg_js: JsValue = msg.into();
    port.post_message(&msg_js)?;
    Ok(())
}

// =============================================================================
// WebCodecs audio
// =============================================================================

/// `AudioEncoder.isConfigSupported({codec:"opus", ...})`.
pub async fn is_opus_supported(
    sample_rate: u32,
    channels: u32,
    bitrate: u32,
) -> Result<bool, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioEncoder"))?;
    if !ctor.is_function() {
        return Ok(false);
    }
    let is_cs = Reflect::get(&ctor, &JsValue::from_str("isConfigSupported"))?;
    if !is_cs.is_function() {
        return Ok(false);
    }
    let is_cs = is_cs.dyn_into::<js_sys::Function>()?;

    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str("opus"))?;
    set_prop(&cfg, "sampleRate", &num(sample_rate as f64))?;
    set_prop(&cfg, "numberOfChannels", &num(channels as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;

    let promise = is_cs.call1(&ctor, &cfg)?.dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;
    Ok(Reflect::get(&result, &JsValue::from_str("supported"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

/// `new AudioData({ format:"f32-planar", ..., data: Float32Array })`.
pub fn make_audio_data(
    samples: &[f32],
    sample_rate: u32,
    channels: u32,
    timestamp_us: f64,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioData"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("AudioData not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let n_frames = (samples.len() as u32) / channels.max(1);

    let arr = js_sys::Float32Array::from(samples);
    let init = Object::new();
    set_prop(&init, "format", &JsValue::from_str("f32-planar"))?;
    set_prop(&init, "sampleRate", &num(sample_rate as f64))?;
    set_prop(&init, "numberOfFrames", &num(n_frames as f64))?;
    set_prop(&init, "numberOfChannels", &num(channels as f64))?;
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    set_prop(&init, "data", &arr)?;

    let args = Array::new();
    args.push(&init);
    Reflect::construct(&ctor, &args)
}

/// Извлекает один канал AudioData как `Vec<f32>`.
pub fn audio_data_channel(data: &JsValue, plane: u32) -> Vec<f32> {
    let n = Reflect::get(data, &JsValue::from_str("numberOfFrames"))
        .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    if n == 0 {
        return Vec::new();
    }
    let dest = js_sys::Float32Array::new_with_length(n);
    let opts = Object::new();
    if set_prop(&opts, "planeIndex", &num(plane as f64)).is_err() {
        return Vec::new();
    }
    if set_prop(&opts, "format", &JsValue::from_str("f32-planar")).is_err() {
        return Vec::new();
    }
    let Ok(copy_to) = get_fn(data, "copyTo") else { return Vec::new(); };
    if copy_to.call2(data, &dest, &opts).is_err() {
        return Vec::new();
    }
    dest.to_vec()
}

pub fn audio_data_number_of_channels(data: &JsValue) -> u32 {
    Reflect::get(data, &JsValue::from_str("numberOfChannels"))
        .ok().and_then(|v| v.as_f64()).map(|n| n as u32).unwrap_or(0)
}

pub fn close_audio_data(data: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(data, "close")?;
    let _ = f.call0(data)?;
    Ok(())
}

pub fn chunk_byte_length(chunk: &JsValue) -> usize {
    Reflect::get(chunk, &JsValue::from_str("byteLength"))
        .ok().and_then(|v| v.as_f64()).map(|n| n as usize).unwrap_or(0)
}

/// Человекочитаемое сообщение об ошибке.
pub fn format_audio_error(e: &JsValue) -> String {
    let name = Reflect::get(e, &JsValue::from_str("name"))
        .ok().and_then(|v| v.as_string()).unwrap_or_default();
    let message = Reflect::get(e, &JsValue::from_str("message"))
        .ok().and_then(|v| v.as_string()).unwrap_or_default();
    match name.as_str() {
        "NotAllowedError" => "Доступ к микрофону запрещён. Разрешите в браузере.".to_string(),
        "NotFoundError" => "Микрофон не найден. Используйте «Тестовый тон».".to_string(),
        "NotReadableError" => "Микрофон занят другим приложением.".to_string(),
        _ => format_str(format_args!("{name}: {message}")),
    }
}

pub fn bytes_human(n: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let f = n as f64;
    if f < KB { return format_str(format_args!("{n} B")); }
    if f < MB { return format_str(format_args!("{:.1} KB", f / KB)); }
    format_str(format_args!("{:.2} MB", f / MB))
}

pub fn pcm_bytes(frames: u64, channels: u32) -> u64 {
    frames * channels as u64 * 4
}