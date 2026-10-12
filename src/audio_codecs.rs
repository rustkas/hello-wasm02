//! 23.5. AudioCodecs: AudioData, AudioEncoder, AudioDecoder.
//!
//! Обёртки вокруг аудио-интерфейсов WebCodecs:
//!
//! - `AudioData` — конструктор из `Float32Array`, чтение метаданных,
//!   `copyTo` для извлечения сэмплов;
//! - `AudioEncoder` — конструктор, `configure`, `encode`, `flush`;
//! - `AudioDecoder` — конструктор, `configure`, `decode`, `flush`;
//! - `EncodedAudioChunk` — чтение `type`, `byteLength`, `timestamp`.
//!
//! ## Отличия от видео
//!
//! В 23.1–23.4 работали с `VideoFrame` / `VideoEncoder` /
//! `VideoDecoder`. Здесь — параллельный набор для аудио. API похожи,
//! но `AudioData` использует planar-форматы, а `copyTo` требует
//! явного `planeIndex` и целевого `format`.
//!
//! ## Переиспользование
//!
//! Хелперы `num`, `set_prop`, `get_fn`, `format_str` — из
//! `video_frame::*`. Дублирования нет.

use js_sys::{Array, Float32Array, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::{format_str, get_fn, num, set_prop};

// =============================================================================
// Feature detection
// =============================================================================

/// `AudioEncoder` доступен в браузере.
pub fn is_audio_encoder_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("AudioEncoder"))
        .map(|v| v.is_function())
        .unwrap_or(false)
}

/// `AudioDecoder` доступен в браузере.
pub fn is_audio_decoder_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("AudioDecoder"))
        .map(|v| v.is_function())
        .unwrap_or(false)
}

/// `AudioData` доступен в браузере.
pub fn is_audio_data_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("AudioData"))
        .map(|v| v.is_function())
        .unwrap_or(false)
}

// =============================================================================
// AudioData: конструктор и чтение
// =============================================================================

/// `new AudioData({ format, sampleRate, numberOfFrames,
/// numberOfChannels, timestamp, data })`.
///
/// Для planar-форматов `data` — один `Float32Array` со **всеми**
/// каналами подряд: сначала канал 0, потом канал 1, и т. д.
///
/// Возвращает `JsValue`-обёртку над `AudioData`.
pub fn make_audio_data(
    samples: &[f32],
    sample_rate: u32,
    number_of_channels: u32,
    timestamp_us: f64,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioData"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("AudioData not available"));
    }

    let number_of_frames = (samples.len() as u32) / number_of_channels;
    let array = Float32Array::from(samples);

    let init = Object::new();
    set_prop(&init, "format", &JsValue::from_str("f32-planar"))?;
    set_prop(&init, "sampleRate", &num(sample_rate as f64))?;
    set_prop(&init, "numberOfFrames", &num(number_of_frames as f64))?;
    set_prop(&init, "numberOfChannels", &num(number_of_channels as f64))?;
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    set_prop(&init, "data", &array)?;

    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    Reflect::construct(&ctor, &{
        let args = Array::new();
        args.push(&init);
        args
    })
}

/// `audioData.numberOfFrames`.
pub fn audio_data_number_of_frames(data: &JsValue) -> u32 {
    Reflect::get(data, &JsValue::from_str("numberOfFrames"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `audioData.numberOfChannels`.
pub fn audio_data_number_of_channels(data: &JsValue) -> u32 {
    Reflect::get(data, &JsValue::from_str("numberOfChannels"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `audioData.sampleRate`.
pub fn audio_data_sample_rate(data: &JsValue) -> u32 {
    Reflect::get(data, &JsValue::from_str("sampleRate"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `audioData.format` — строка формата (`"f32-planar"` и т. д.).
pub fn audio_data_format(data: &JsValue) -> String {
    Reflect::get(data, &JsValue::from_str("format"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// `audioData.timestamp` — микросекунды.
pub fn audio_data_timestamp(data: &JsValue) -> f64 {
    Reflect::get(data, &JsValue::from_str("timestamp"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// `audioData.close()`.
pub fn audio_data_close(data: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(data, "close")?;
    let _ = f.call0(data)?;
    Ok(())
}

/// Извлекает один канал (`planeIndex`) как `Vec<f32>`.
/// Целевой формат — всегда `f32-planar` (Web Audio API использует
/// только его; конвертация из любого формата обязательна по спеке).
pub fn audio_data_copy_channel(data: &JsValue, plane_index: u32) -> Vec<f32> {
    let n_frames = audio_data_number_of_frames(data);
    if n_frames == 0 {
        return Vec::new();
    }
    let dest = Float32Array::new_with_length(n_frames);
    let opts = Object::new();
    if set_prop(&opts, "planeIndex", &num(plane_index as f64)).is_err() {
        return Vec::new();
    }
    if set_prop(&opts, "format", &JsValue::from_str("f32-planar")).is_err() {
        return Vec::new();
    }
    let f = match get_fn(data, "copyTo") {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    if f.call2(data, &dest, &opts).is_err() {
        return Vec::new();
    }
    dest.to_vec()
}

// =============================================================================
// AudioEncoder
// =============================================================================

/// `new AudioEncoder({ output, error })`.
pub fn make_audio_encoder(
    on_output: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue, JsValue)>,
    on_error: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
) -> Result<JsValue, JsValue> {
    let init = Object::new();
    set_prop(&init, "output", on_output.as_ref())?;
    set_prop(&init, "error", on_error.as_ref())?;
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioEncoder"))?;
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let args = Array::new();
    args.push(&init);
    Reflect::construct(&ctor, &args)
}

/// `encoder.configure({ codec, sampleRate, numberOfChannels, bitrate })`.
pub fn configure_audio_encoder(
    encoder: &JsValue,
    codec: &str,
    sample_rate: u32,
    number_of_channels: u32,
    bitrate: u32,
) -> Result<(), JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "sampleRate", &num(sample_rate as f64))?;
    set_prop(&cfg, "numberOfChannels", &num(number_of_channels as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;
    let f = get_fn(encoder, "configure")?;
    let _ = f.call1(encoder, &cfg)?;
    Ok(())
}

/// `encoder.encode(audio_data)`.
pub fn encode_audio(encoder: &JsValue, audio_data: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(encoder, "encode")?;
    let _ = f.call1(encoder, audio_data)?;
    Ok(())
}

/// `await encoder.flush()`.
pub async fn flush_audio_encoder(encoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(encoder, "flush")?;
    let promise = f.call0(encoder)?.dyn_into::<Promise>()?;
    JsFuture::from(promise).await.map(|_| ())
}

/// `encoder.close()`.
pub fn close_audio_encoder(encoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(encoder, "close")?;
    let _ = f.call0(encoder)?;
    Ok(())
}

/// `AudioEncoder.isConfigSupported(config)` — статический метод.
pub async fn is_audio_config_supported(
    codec: &str,
    sample_rate: u32,
    number_of_channels: u32,
    bitrate: u32,
) -> Result<bool, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioEncoder"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("AudioEncoder not available"));
    }
    let f = Reflect::get(&ctor, &JsValue::from_str("isConfigSupported"))?;
    if !f.is_function() {
        return Err(JsValue::from_str(
            "AudioEncoder.isConfigSupported not available",
        ));
    }
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "sampleRate", &num(sample_rate as f64))?;
    set_prop(&cfg, "numberOfChannels", &num(number_of_channels as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;
    let f = f.dyn_into::<js_sys::Function>()?;
    let promise = f.call1(&ctor, &cfg)?.dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;
    Ok(Reflect::get(&result, &JsValue::from_str("supported"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

// =============================================================================
// AudioDecoder
// =============================================================================

/// `new AudioDecoder({ output, error })`.
pub fn make_audio_decoder(
    on_output: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
    on_error: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
) -> Result<JsValue, JsValue> {
    let init = Object::new();
    set_prop(&init, "output", on_output.as_ref())?;
    set_prop(&init, "error", on_error.as_ref())?;
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("AudioDecoder"))?;
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let args = Array::new();
    args.push(&init);
    Reflect::construct(&ctor, &args)
}

/// `decoder.configure({ codec, sampleRate, numberOfChannels })`.
pub fn configure_audio_decoder(
    decoder: &JsValue,
    codec: &str,
    sample_rate: u32,
    number_of_channels: u32,
) -> Result<(), JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "sampleRate", &num(sample_rate as f64))?;
    set_prop(&cfg, "numberOfChannels", &num(number_of_channels as f64))?;
    let f = get_fn(decoder, "configure")?;
    let _ = f.call1(decoder, &cfg)?;
    Ok(())
}

/// `decoder.decode(chunk)`.
pub fn decode_audio(decoder: &JsValue, chunk: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "decode")?;
    let _ = f.call1(decoder, chunk)?;
    Ok(())
}

/// `await decoder.flush()`.
pub async fn flush_audio_decoder(decoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "flush")?;
    let promise = f.call0(decoder)?.dyn_into::<Promise>()?;
    JsFuture::from(promise).await.map(|_| ())
}

/// `decoder.close()`.
pub fn close_audio_decoder(decoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "close")?;
    let _ = f.call0(decoder)?;
    Ok(())
}

// =============================================================================
// EncodedAudioChunk helpers
// =============================================================================

/// `chunk.type` — `"key"` или `"delta"`.
pub fn chunk_type(chunk: &JsValue) -> String {
    Reflect::get(chunk, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// `chunk.byteLength`.
pub fn chunk_byte_length(chunk: &JsValue) -> usize {
    Reflect::get(chunk, &JsValue::from_str("byteLength"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as usize)
        .unwrap_or(0)
}

/// `chunk.timestamp` — микросекунды.
pub fn chunk_timestamp(chunk: &JsValue) -> f64 {
    Reflect::get(chunk, &JsValue::from_str("timestamp"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Извлекает байты из `EncodedAudioChunk` для отладочного
/// отображения. `copyTo` требует Uint8Array нужной длины.
#[allow(dead_code)]
pub fn chunk_to_vec(chunk: &JsValue) -> Vec<u8> {
    let n = chunk_byte_length(chunk);
    if n == 0 {
        return Vec::new();
    }
    let dest = Uint8Array::new_with_length(n as u32);
    let f = match get_fn(chunk, "copyTo") {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    if f.call1(chunk, &dest).is_err() {
        return Vec::new();
    }
    dest.to_vec()
}

// =============================================================================
// Конструктор EncodedAudioChunk (для round-trip теста)
// =============================================================================

/// `new EncodedAudioChunk({ type, timestamp, duration, data })`.
pub fn make_encoded_audio_chunk(
    chunk_type: &str,
    timestamp_us: f64,
    duration_us: f64,
    data: &[u8],
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("EncodedAudioChunk"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("EncodedAudioChunk not available"));
    }
    let init = Object::new();
    set_prop(&init, "type", &JsValue::from_str(chunk_type))?;
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    set_prop(&init, "duration", &num(duration_us))?;
    let arr = Uint8Array::from(data);
    set_prop(&init, "data", &arr)?;
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let args = Array::new();
    args.push(&init);
    Reflect::construct(&ctor, &args)
}

// =============================================================================
// Вспомогательная утилита для форматирования
// =============================================================================

/// Форматирует «N байт» с килобайтами, если больше 1024.
pub fn bytes_human(n: usize) -> String {
    if n >= 1024 {
        format_str(format_args!(
            "{n} ({kb:.1} KB)",
            n = n,
            kb = n as f64 / 1024.0,
        ))
    } else {
        format_str(format_args!("{n}", n = n))
    }
}