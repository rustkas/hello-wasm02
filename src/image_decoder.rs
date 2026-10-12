//! 23.6. ImageDecoder: статика и animated GIF.
//!
//! Обёртки вокруг `ImageDecoder`:
//!
//! - конструктор `new ImageDecoder({ data, type })`;
//! - чтение `tracks` (ImageTrackList) — метаданные без декодирования;
//! - `decode({ frameIndex, completeFramesOnly })` → VideoFrame;
//! - `close()`;
//! - статический `ImageDecoder.isTypeSupported(type)`.
//!
//! ## Отличия от видео/аудио
//!
//! `VideoDecoder` / `AudioDecoder` — потоковые, принимают чанки по
//! одному. `ImageDecoder` получает **весь файл** при создании и
//! сразу умеет читать метаданные. Это API для статических и
//! анимированных изображений, не для видео.
//!
//! ## Fallback для размеров кадра
//!
//! Chrome для кадров анимированного GIF иногда возвращает
//! `displayWidth`/`displayHeight` = 0. `frame_display_width` /
//! `frame_display_height` пробуют несколько полей и возвращают
//! первое ненулевое.
//!
//! ## Пиксельная копия
//!
//! `ImageDecoder` в Chrome агрессивно инвалидирует свои VideoFrame.
//! `copy_frame_to_rgba` через `frame.copyTo()` даёт независимый
//! RGBA-буфер до того, как decoder пометит frame как broken.
//! `blit_rgba_to_canvas` рисует буфер через `putImageData` —
//! синхронно, без зависимости от жизненного цикла frame.
//!
//! ## Переиспользование
//!
//! Хелперы `num`, `set_prop`, `get_fn`, `format_str`, `invoke` —
//! из `video_frame::*`.

use js_sys::{Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::Clamped;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use crate::video_frame::{format_str, get_fn, invoke, num, set_prop};

// =============================================================================
// Feature detection
// =============================================================================

/// `ImageDecoder` доступен в браузере.
pub fn is_image_decoder_supported() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &JsValue::from_str("ImageDecoder"))
        .map(|v| v.is_function())
        .unwrap_or(false)
}

/// `await ImageDecoder.isTypeSupported(type)` — статический метод.
pub async fn is_type_supported(mime: &str) -> Result<bool, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("ImageDecoder"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("ImageDecoder not available"));
    }
    let f = Reflect::get(&ctor, &JsValue::from_str("isTypeSupported"))?;
    if !f.is_function() {
        return Err(JsValue::from_str(
            "ImageDecoder.isTypeSupported not available",
        ));
    }
    let f = f.dyn_into::<js_sys::Function>()?;
    let promise = f.call1(&ctor, &JsValue::from_str(mime))?
        .dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;
    Ok(result.as_bool().unwrap_or(false))
}

// =============================================================================
// Конструктор
// =============================================================================

/// `new ImageDecoder({ data, type })`.
///
/// `data` — байты файла (BufferSource), `type` — MIME-строка.
pub fn make_image_decoder(data: &[u8], mime: &str) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("ImageDecoder"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("ImageDecoder not available"));
    }
    let init = Object::new();
    let arr = Uint8Array::from(data);
    set_prop(&init, "data", &arr)?;
    set_prop(&init, "type", &JsValue::from_str(mime))?;
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let args = js_sys::Array::new();
    args.push(&init);
    Reflect::construct(&ctor, &args)
}

// =============================================================================
// Чтение метаданных трека
// =============================================================================

/// Ждёт `decoder.tracks.ready`.
pub async fn wait_tracks_ready(decoder: &JsValue) -> Result<(), JsValue> {
    let tracks = Reflect::get(decoder, &JsValue::from_str("tracks"))?;
    let ready = Reflect::get(&tracks, &JsValue::from_str("ready"))?;
    if !ready.is_undefined() {
        let promise = ready.dyn_into::<Promise>()?;
        let _ = JsFuture::from(promise).await?;
    }
    Ok(())
}

/// `decoder.tracks.length`.
pub fn tracks_length(decoder: &JsValue) -> u32 {
    Reflect::get(decoder, &JsValue::from_str("tracks"))
        .ok()
        .and_then(|t| Reflect::get(&t, &JsValue::from_str("length")).ok())
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `decoder.tracks[0]` — первый трек.
pub fn first_track(decoder: &JsValue) -> Result<JsValue, JsValue> {
    let tracks = Reflect::get(decoder, &JsValue::from_str("tracks"))?;
    let idx = JsValue::from_f64(0.0);
    let track = Reflect::get(&tracks, &idx)?;
    if track.is_undefined() || track.is_null() {
        return Err(JsValue::from_str("no tracks in decoder"));
    }
    Ok(track)
}

/// `track.animated`.
pub fn track_animated(track: &JsValue) -> bool {
    Reflect::get(track, &JsValue::from_str("animated"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// `track.frameCount`.
pub fn track_frame_count(track: &JsValue) -> u32 {
    Reflect::get(track, &JsValue::from_str("frameCount"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// `track.repetitionCount`. Может быть `Infinity` — для бесконечной
/// анимации. Возвращает `f64`, потому что Infinity валидно.
pub fn track_repetition_count(track: &JsValue) -> f64 {
    Reflect::get(track, &JsValue::from_str("repetitionCount"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// `track.selected` — выбран ли трек для декодирования.
pub fn track_selected(track: &JsValue) -> bool {
    Reflect::get(track, &JsValue::from_str("selected"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

// =============================================================================
// Декодирование кадра
// =============================================================================

/// Результат одного вызова `decode()`: сам кадр и флаг полноты.
pub struct DecodeResult {
    pub frame: JsValue,
    pub complete: bool,
}

/// `await decoder.decode({ frameIndex, completeFramesOnly })`.
///
/// Возвращает `{ image: VideoFrame, complete: boolean }`.
pub async fn decode_frame(
    decoder: &JsValue,
    frame_index: u32,
    complete_frames_only: bool,
) -> Result<DecodeResult, JsValue> {
    let opts = Object::new();
    set_prop(&opts, "frameIndex", &num(frame_index as f64))?;
    set_prop(
        &opts,
        "completeFramesOnly",
        &JsValue::from_bool(complete_frames_only),
    )?;

    let f = get_fn(decoder, "decode")?;
    let promise = f.call1(decoder, &opts)?.dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;

    let frame = Reflect::get(&result, &JsValue::from_str("image"))?;
    let complete = Reflect::get(&result, &JsValue::from_str("complete"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(DecodeResult { frame, complete })
}

/// Читает числовое поле. Для ключей с точкой
/// (`visibleRect.width`) спускается на один уровень.
fn read_numeric(obj: &JsValue, key: &str) -> Result<f64, JsValue> {
    if let Some((head, tail)) = key.split_once('.') {
        let sub = Reflect::get(obj, &JsValue::from_str(head))?;
        if sub.is_null() || sub.is_undefined() {
            return Err(JsValue::from_str("missing nested field"));
        }
        read_numeric(&sub, tail)
    } else {
        let v = Reflect::get(obj, &JsValue::from_str(key))?;
        v.as_f64().ok_or_else(|| JsValue::from_str("not a number"))
    }
}

/// `frame.displayWidth`, с fallback на `codedWidth` и `visibleRect.width`.
///
/// Chrome для кадров анимированного GIF иногда возвращает
/// `displayWidth = 0` в момент, когда метаданные кадра ещё не
/// заполнены. Пробуем несколько полей и возвращаем первое ненулевое.
/// Если все нулевые — 0; вызывающий код должен использовать fallback
/// из метаданных трека.
pub fn frame_display_width(frame: &JsValue) -> u32 {
    for key in ["displayWidth", "codedWidth", "visibleRect.width"] {
        if let Ok(v) = read_numeric(frame, key) {
            if v > 0.0 {
                return v as u32;
            }
        }
    }
    0
}

/// `frame.displayHeight`, с fallback на `codedHeight` и
/// `visibleRect.height`.
pub fn frame_display_height(frame: &JsValue) -> u32 {
    for key in ["displayHeight", "codedHeight", "visibleRect.height"] {
        if let Ok(v) = read_numeric(frame, key) {
            if v > 0.0 {
                return v as u32;
            }
        }
    }
    0
}

/// `frame.duration` — микросекунды. Для статических кадров `0` или `null`.
pub fn frame_duration_us(frame: &JsValue) -> f64 {
    Reflect::get(frame, &JsValue::from_str("duration"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// `frame.close()`.
pub fn frame_close(frame: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(frame, "close")?;
    let _ = f.call0(frame)?;
    Ok(())
}

// =============================================================================
// Закрытие
// =============================================================================

/// `decoder.close()`.
pub fn close_image_decoder(decoder: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(decoder, "close")?;
    let _ = f.call0(decoder)?;
    Ok(())
}

// =============================================================================
// Утилита: чтение файла как ArrayBuffer
// =============================================================================

/// Читает `File` через `FileReader` и возвращает байты.
pub async fn read_file_bytes(file: &web_sys::File) -> Result<Vec<u8>, JsValue> {
    let reader = web_sys::FileReader::new()?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let reader_c = reader.clone();
        let onload = Closure::once(move || {
            let result = reader_c.result().unwrap_or(JsValue::UNDEFINED);
            let _ = resolve.call1(&JsValue::NULL, &result);
        });
        let onerror = Closure::once(move || {
            let _ = reject.call1(
                &JsValue::NULL,
                &JsValue::from_str("FileReader failed"),
            );
        });
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        reader.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onload.forget();
        onerror.forget();
        let _ = reader.read_as_array_buffer(file);
    });
    let result = JsFuture::from(promise).await?;
    let array = Uint8Array::new(&result);
    Ok(array.to_vec())
}

/// Форматирует `repetitionCount` для UI.
pub fn format_repetition(count: f64) -> String {
    if count.is_infinite() {
        String::from("∞ (бесконечно)")
    } else if count == 0.0 {
        String::from("0 (один раз)")
    } else {
        format_str(format_args!("{c:.0}", c = count))
    }
}

// =============================================================================
// VideoFrame → canvas
// =============================================================================

/// Рисует RGBA-буфер на canvas через `putImageData`.
///
/// Синхронная операция, не зависит от жизненного цикла VideoFrame.
/// Вызов через `invoke` — сигнатура `dx`/`dy` меняется между
/// stable и unstable-сборками web-sys.
pub fn draw_frame_on_canvas(
    ctx: &web_sys::CanvasRenderingContext2d,
    frame: &JsValue,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), JsValue> {
    if frame.is_null() || frame.is_undefined() {
        return Err(JsValue::from_str(
            "draw_frame_on_canvas: frame is null or undefined",
        ));
    }
    if w <= 0.0 || h <= 0.0 {
        return Err(JsValue::from_str(
            "draw_frame_on_canvas: zero dimensions",
        ));
    }
    let ctx_js: &JsValue = ctx.as_ref();
    let _ = invoke(
        ctx_js,
        "drawImage",
        &[frame.clone(), num(x), num(y), num(w), num(h)],
    )?;
    Ok(())
}

// =============================================================================
// Snapshot VideoFrame → ImageBitmap
// =============================================================================

/// `await createImageBitmap(videoFrame, opts)` — материализует
/// пиксели VideoFrame в независимый ImageBitmap.
///
/// Chrome для кадров анимированного GIF возвращает `displayWidth`
/// и `codedWidth` = 0. `createImageBitmap(frame)` без опций падает
/// с `Passed-in video frame has width 0`. Передаём явные размеры
/// (`resize_w`, `resize_h` — из метаданных трека).
pub async fn snapshot_video_frame(
    frame: &JsValue,
    resize_w: u32,
    resize_h: u32,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let f = Reflect::get(&global, &JsValue::from_str("createImageBitmap"))?;
    if !f.is_function() {
        return Err(JsValue::from_str("createImageBitmap not available"));
    }
    let f = f.dyn_into::<js_sys::Function>()?;

    if resize_w > 0 && resize_h > 0 {
        let opts = Object::new();
        set_prop(&opts, "resizeWidth", &num(resize_w as f64))?;
        set_prop(&opts, "resizeHeight", &num(resize_h as f64))?;
        set_prop(&opts, "resizeQuality", &JsValue::from_str("high"))?;
        let promise = f
            .call2(&JsValue::NULL, frame, &opts)?
            .dyn_into::<Promise>()?;
        JsFuture::from(promise).await
    } else {
        let promise = f
            .call1(&JsValue::NULL, frame)?
            .dyn_into::<Promise>()?;
        JsFuture::from(promise).await
    }
}

/// `bitmap.close()`.
pub fn close_image_bitmap(bitmap: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(bitmap, "close")?;
    let _ = f.call0(bitmap)?;
    Ok(())
}

// =============================================================================
// Пиксельная копия VideoFrame
// =============================================================================

/// Копирует пиксели `VideoFrame` в RGBA-буфер через
/// `frame.copyTo(dest, { format: "RGBA" })`.
///
/// Возвращает `Vec<u8>` размером `width * height * 4`. Данные
/// полностью независимы от исходного VideoFrame — decoder может
/// пометить frame как broken в любой момент, наш буфер уже наш.
pub async fn copy_frame_to_rgba(
    frame: &JsValue,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, JsValue> {
    if width == 0 || height == 0 {
        return Err(JsValue::from_str(
            "copy_frame_to_rgba: zero dimensions",
        ));
    }
    let size = (width as usize) * (height as usize) * 4;
    let buffer = Uint8Array::new_with_length(size as u32);
    let opts = Object::new();
    set_prop(&opts, "format", &JsValue::from_str("RGBA"))?;
    let f = get_fn(frame, "copyTo")?;
    let promise = f.call2(frame, &buffer, &opts)?
        .dyn_into::<Promise>()?;
    JsFuture::from(promise).await?;
    Ok(buffer.to_vec())
}

/// Клонирует VideoFrame через `frame.clone()`.
///
/// Clone — независимый handle на тот же media resource. Если decoder
/// пометит оригинал как broken, clone остаётся валидным. Fallback,
/// когда `copyTo` не работает.
pub fn clone_video_frame(frame: &JsValue) -> Result<JsValue, JsValue> {
    let f = get_fn(frame, "clone")?;
    f.call0(frame)
}

/// Рисует RGBA-буфер на canvas через `putImageData`.
///
/// Синхронная операция, не зависит от жизненного цикла VideoFrame.
/// `put_image_data` принимает `i32` — координаты в пикселях.
pub fn blit_rgba_to_canvas(
    canvas: &HtmlCanvasElement,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(), JsValue> {
    if width == 0 || height == 0 {
        return Err(JsValue::from_str("blit_rgba_to_canvas: zero dimensions"));
    }
    if canvas.width() != width {
        canvas.set_width(width);
    }
    if canvas.height() != height {
        canvas.set_height(height);
    }
    let clamped = Clamped(&rgba[..]);
    let img = ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height)?;
    let ctx = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()?;
        // putImageData вызывается через invoke: сигнатура dx/dy
    // зависит от cfg web_sys_unstable_apis (i32/f64) — typed-метод
    // ломается при смене конфигурации.
    invoke(
        ctx.as_ref(),
        "putImageData",
        &[img.into(), num(0.0), num(0.0)],
    )?;
    Ok(())
}