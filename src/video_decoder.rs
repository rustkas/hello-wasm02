//! 23.2. VideoDecoder: сжатое → распакованное.
//!
//! Обёртки вокруг `VideoDecoder`:
//!
//! - конструктор с колбэками `output` / `error`;
//! - `configure` с опциональным `description` (SPS/PPS);
//! - `decode(EncodedVideoChunk)`, `flush()`, `close()`;
//! - утилита `hex_dump` для показа description в hex.
//!
//! ## BufferSource
//!
//! `metadata.decoderConfig.description` в Chrome приходит как
//! `ArrayBuffer`, а не `Uint8Array`. Оба варианта, а также typed
//! arrays над буфером, разбирает `buffer_source_to_vec`. Прямой
//! `dyn_into::<Uint8Array>()` на `ArrayBuffer` не работает.
//!
//! ## Переиспользование хелперов
//!
//! Мини-хелперы для JsValue (`get_fn`, `invoke`, `construct`,
//! `set_prop`, `get_str`, `get_f64`, `num`, `format_str`, `bool_of`)
//! определены в `video_frame` (подглава 23.1) и реэкспортируются
//! здесь через `pub use`. Дублирования нет.

use js_sys::{ArrayBuffer, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::{
    bool_of, construct, format_str, get_fn, num, set_prop,
};

// =============================================================================
// BufferSource → Vec<u8>
// =============================================================================

/// Приводит BufferSource к `Vec<u8>`.
///
/// Понимает три случая:
///
/// - `ArrayBuffer` — оборачивает в `Uint8Array::new(&ab)` и копирует.
/// - `Uint8Array` — копирует через `to_vec()`.
/// - Любой `ArrayBufferView` (typed array, DataView) — читает
///   поле `.buffer` и обрабатывает его как `ArrayBuffer`.
///
/// Для не-BufferSource возвращает пустой `Vec`.
///
/// ## Зачем
///
/// `metadata.decoderConfig.description` в Chrome — `ArrayBuffer`.
/// В других браузерах и версиях — может быть `Uint8Array`. Прямое
/// `dyn_into::<Uint8Array>()` на `ArrayBuffer` даёт `Err`, и
/// дальнейшая логика молча получает пустой буфер. Универсальный
/// хелпер снимает этот класс ошибок.
pub fn buffer_source_to_vec(v: &JsValue) -> Vec<u8> {
    // 1. Чистый ArrayBuffer.
    if let Ok(ab) = v.clone().dyn_into::<ArrayBuffer>() {
        return Uint8Array::new(&ab).to_vec();
    }

    // 2. Uint8Array напрямую.
    if let Ok(u8) = v.clone().dyn_into::<Uint8Array>() {
        return u8.to_vec();
    }

    // 3. Любой ArrayBufferView: читаем .buffer.
    if let Ok(buf) = Reflect::get(v, &JsValue::from_str("buffer")) {
        if !buf.is_undefined() && !buf.is_null() {
            if let Ok(ab) = buf.dyn_into::<ArrayBuffer>() {
                return Uint8Array::new(&ab).to_vec();
            }
        }
    }

    Vec::new()
}

// =============================================================================
// Feature detection
// =============================================================================

/// `VideoDecoder` доступен в этом браузере.
pub fn is_video_decoder_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("VideoDecoder")),
        Ok(v) if v.is_function()
    ))
}

/// `VideoDecoder.isConfigSupported` доступен. Статический метод.
pub fn has_is_config_supported() -> bool {
    let global = js_sys::global();
    let Ok(ctor) = Reflect::get(&global, &JsValue::from_str("VideoDecoder")) else {
        return false;
    };
    bool_of(matches!(
        Reflect::get(&ctor, &JsValue::from_str("isConfigSupported")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// Конструктор
// =============================================================================

/// `new VideoDecoder({ output, error })`.
///
/// Оба колбэка обязательны. `output` вызывается асинхронно для
/// каждого декодированного кадра; `error` — при ошибке.
pub fn make_video_decoder(
    on_output: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
    on_error: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
) -> Result<JsValue, JsValue> {
    let init = Object::new();
    set_prop(&init, "output", on_output.as_ref())?;
    set_prop(&init, "error", on_error.as_ref())?;
    construct("VideoDecoder", &[init.into()])
}

// =============================================================================
// Configure / decode / flush / close
// =============================================================================

/// `decoder.configure({ codec, codedWidth, codedHeight, description })`.
///
/// `description` — `BufferSource` с `avcC`-box (SPS/PPS). Для avc1
/// обязателен. Для avc3 (Annex B) не нужен.
pub fn configure(
    decoder: &JsValue,
    codec: &str,
    coded_width: u32,
    coded_height: u32,
    description: Option<&JsValue>,
) -> Result<(), JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "codedWidth", &num(coded_width as f64))?;
    set_prop(&cfg, "codedHeight", &num(coded_height as f64))?;
    if let Some(d) = description {
        set_prop(&cfg, "description", d)?;
    }
    let f = get_fn(decoder, "configure")?;
    let _ = f.call1(decoder, &cfg)?;
    Ok(())
}

/// `decoder.decode(chunk)`.
///
/// Чанки подаются в порядке timestamp. Первым должен идти key-кадр.
pub fn decode_chunk(decoder: &JsValue, chunk: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "decode")?;
    let _ = f.call1(decoder, chunk)?;
    Ok(())
}

/// `await decoder.flush()`.
///
/// Дожидается всех кадров, декодирует оставшиеся, резолвится когда
/// очередь пуста.
pub async fn flush(decoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "flush")?;
    let promise = f.call0(decoder)?.dyn_into::<Promise>()?;
    JsFuture::from(promise).await.map(|_| ())
}

/// `decoder.close()`.
pub fn close(decoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "close")?;
    let _ = f.call0(decoder)?;
    Ok(())
}

// =============================================================================
// Утилита: hex dump для description
// =============================================================================

/// Возвращает hex-представление первых `max_bytes` байт значения.
///
/// Понимает `ArrayBuffer`, `Uint8Array` и любой `ArrayBufferView`
/// через `buffer_source_to_vec`. Для не-BufferSource возвращает
/// `"(не BufferSource или пустой буфер)"`.
///
/// Полезно для показа `description` (SPS/PPS) — там видно
/// характерные байты `0x67` (SPS) и `0x68` (PPS).
pub fn hex_dump(value: &JsValue, max_bytes: usize) -> String {
    let bytes = buffer_source_to_vec(value);
    if bytes.is_empty() {
        return String::from("(не BufferSource или пустой буфер)");
    }

    let len = bytes.len();
    let shown = len.min(max_bytes);

    // Оценка: 3 символа на байт + пробелы + переводы строк.
    let mut s = String::with_capacity(shown * 3 + 32);
    for (i, b) in bytes.iter().take(shown).enumerate() {
        if i > 0 {
            if i % 16 == 0 {
                s.push('\n');
            } else {
                s.push(' ');
            }
        }
        s.push_str(&format_str(format_args!("{:02x}", b)));
    }
    if len > max_bytes {
        s.push_str(&format_str(format_args!(
            "\n… (всего {len} байт)",
            len = len,
        )));
    }
    s
}