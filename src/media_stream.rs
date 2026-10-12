//! 23.8. MediaStream: real-time camera pipeline.
//!
//! Обёртки вокруг Insertable Streams for MediaStreamTrack:
//!
//! - `MediaStreamTrackProcessor` — `MediaStreamTrack` → `ReadableStream<VideoFrame>`.
//! - `MediaStreamTrackGenerator` — `WritableStream<VideoFrame>` → `MediaStreamTrack`.
//! - `canvas.captureStream()` — fallback без камеры.
//! - `getUserMedia` — доступ к камере.
//!
//! ## Что делает pipeline
//!
//! ```text
//! getUserMedia → MediaStreamTrack
//!   → MediaStreamTrackProcessor.readable → ReadableStream<VideoFrame>
//!   → WebGL2 grayscale (texImage2D + fragment shader)
//!   → new VideoFrame(bitmap, { timestamp, duration })
//!   → MediaStreamTrackGenerator.writable → MediaStreamTrack
//!   → MediaStream → video.srcObject
//! ```
//!
//! ## Наследование `MediaStreamTrackGenerator`
//!
//! `MediaStreamTrackGenerator` **сам является** `MediaStreamTrack`
//! (спецификация: `interface MediaStreamTrackGenerator : MediaStreamTrack`).
//! Отдельного поля `.track` у него нет — это отличает его от
//! нового API `VideoTrackGenerator`, у которого есть `.track`.
//!
//! `generator_track` поддерживает оба варианта: сначала пробует
//! `.track`, при отсутствии — приводит сам generator к
//! `MediaStreamTrack`.
//!
//! ## Метрики
//!
//! FPS и latency считаются в `app.rs` по обработанным кадрам.
//!
//! ## Переиспользование
//!
//! Хелперы `bool_of`, `format_str`, `get_fn`, `invoke`, `num`,
//! `set_prop` — из `video_frame::*`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, HtmlVideoElement, MediaStream, MediaStreamTrack};

use crate::video_frame::{bool_of, format_str, get_fn, invoke, num, set_prop};

// =============================================================================
// Feature detection
// =============================================================================

/// `MediaStreamTrackProcessor` доступен в этом контексте.
///
/// В Chrome — и в Window, и в Worker. В Firefox и Safari — только
/// в DedicatedWorker. Проверяйте перед использованием.
pub fn has_track_processor() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        js_sys::Reflect::get(&global, &JsValue::from_str("MediaStreamTrackProcessor")),
        Ok(v) if v.is_function()
    ))
}

/// `MediaStreamTrackGenerator` доступен в этом контексте.
pub fn has_track_generator() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        js_sys::Reflect::get(&global, &JsValue::from_str("MediaStreamTrackGenerator")),
        Ok(v) if v.is_function()
    ))
}

/// `navigator.mediaDevices.getUserMedia` доступен.
pub fn has_get_user_media() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(nav) = js_sys::Reflect::get(
        window.as_ref(),
        &JsValue::from_str("navigator"),
    ) else {
        return false;
    };
    let Ok(md) = js_sys::Reflect::get(&nav, &JsValue::from_str("mediaDevices")) else {
        return false;
    };
    bool_of(matches!(
        js_sys::Reflect::get(&md, &JsValue::from_str("getUserMedia")),
        Ok(v) if v.is_function()
    ))
}

/// `HTMLCanvasElement.captureStream` доступен.
pub fn has_capture_stream() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Some(document) = window.document() else {
        return false;
    };
    let Ok(el) = document.create_element("canvas") else {
        return false;
    };
    let Ok(canvas) = el.dyn_into::<HtmlCanvasElement>() else {
        return false;
    };
    bool_of(matches!(
        js_sys::Reflect::get(canvas.as_ref(), &JsValue::from_str("captureStream")),
        Ok(v) if v.is_function()
    ))
}

/// Документ обслуживается активным Service Worker'ом.
///
/// Media APIs (особенно `getUserMedia` и `captureStream`) могут
/// конфликтовать со SW, который перехватывает stream-запросы.
pub fn has_active_service_worker() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(nav) = js_sys::Reflect::get(
        window.as_ref(),
        &JsValue::from_str("navigator"),
    ) else {
        return false;
    };
    let Ok(sw) = js_sys::Reflect::get(&nav, &JsValue::from_str("serviceWorker")) else {
        return false;
    };
    if sw.is_undefined() || sw.is_null() {
        return false;
    }
    let Ok(ctrl) = js_sys::Reflect::get(&sw, &JsValue::from_str("controller")) else {
        return false;
    };
    !ctrl.is_null() && !ctrl.is_undefined()
}

// =============================================================================
// getUserMedia
// =============================================================================

/// `await navigator.mediaDevices.getUserMedia({ video: true })`.
///
/// Требует user gesture — в headless-тестах не воспроизводится.
/// При отказе пользователя бросает `NotAllowedError`.
pub async fn get_camera_stream() -> Result<MediaStream, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let nav = js_sys::Reflect::get(
        window.as_ref(),
        &JsValue::from_str("navigator"),
    )?;
    let md = js_sys::Reflect::get(&nav, &JsValue::from_str("mediaDevices"))?;
    let get_um = js_sys::Reflect::get(&md, &JsValue::from_str("getUserMedia"))?;
    let get_um = get_um.dyn_into::<js_sys::Function>()?;

    let constraints = js_sys::Object::new();
    js_sys::Reflect::set(
        &constraints,
        &JsValue::from_str("video"),
        &JsValue::TRUE,
    )?;
    let promise = get_um
        .call1(&md, &constraints)?
        .dyn_into::<js_sys::Promise>()?;
    let stream = wasm_bindgen_futures::JsFuture::from(promise).await?;
    stream.dyn_into::<MediaStream>()
}

// =============================================================================
// canvas.captureStream — fallback
// =============================================================================

/// `canvas.captureStream(fps)` — синтетический видеопоток.
///
/// Идеален для тестов и окружений без камеры. Возвращает
/// `MediaStream` с одним видео-треком.
pub fn capture_canvas_stream(
    canvas: &HtmlCanvasElement,
    fps: f64,
) -> Result<MediaStream, JsValue> {
    let result = invoke(canvas.as_ref(), "captureStream", &[num(fps)])?;
    result.dyn_into::<MediaStream>()
}

// =============================================================================
// MediaStreamTrackProcessor
// =============================================================================

/// `new MediaStreamTrackProcessor({ track })`.
///
/// Возвращает JS-обёртку над процессором. `.readable` даст
/// `ReadableStream<VideoFrame>`.
pub fn make_track_processor(track: &MediaStreamTrack) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(
        &global,
        &JsValue::from_str("MediaStreamTrackProcessor"),
    )?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("MediaStreamTrackProcessor not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let init = js_sys::Object::new();
    set_prop(&init, "track", track.as_ref())?;
    let args = js_sys::Array::new();
    args.push(&init);
    js_sys::Reflect::construct(&ctor, &args)
}

/// `processor.readable` — `ReadableStream<VideoFrame>`.
pub fn processor_readable(processor: &JsValue) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(processor, &JsValue::from_str("readable"))
}

// =============================================================================
// MediaStreamTrackGenerator
// =============================================================================

/// `new MediaStreamTrackGenerator({ kind: "video" })`.
///
/// Возвращает JS-обёртку над генератором. `.writable` даст
/// `WritableStream<VideoFrame>`, `.readyState` — строку состояния.
pub fn make_track_generator() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(
        &global,
        &JsValue::from_str("MediaStreamTrackGenerator"),
    )?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("MediaStreamTrackGenerator not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let init = js_sys::Object::new();
    set_prop(&init, "kind", &JsValue::from_str("video"))?;
    let args = js_sys::Array::new();
    args.push(&init);
    js_sys::Reflect::construct(&ctor, &args)
}

/// `generator.writable` — `WritableStream<VideoFrame>`.
pub fn generator_writable(generator: &JsValue) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(generator, &JsValue::from_str("writable"))
}

/// Возвращает `MediaStreamTrack` для генератора.
///
/// ## Два варианта API
///
/// - `MediaStreamTrackGenerator` **сам является**
///   `MediaStreamTrack` — наследует его
///   (`interface MediaStreamTrackGenerator : MediaStreamTrack`).
/// - `VideoTrackGenerator` (новый API в Chrome/Safari) **не**
///   наследует — у него есть отдельное поле `.track`.
///
/// Сначала пробуем `.track` (для `VideoTrackGenerator`), при
/// отсутствии — приводим сам generator к `MediaStreamTrack`.
pub fn generator_track(generator: &JsValue) -> Result<MediaStreamTrack, JsValue> {
    // Ветка для VideoTrackGenerator.
    if let Ok(v) = js_sys::Reflect::get(generator, &JsValue::from_str("track")) {
        if !v.is_undefined() && !v.is_null() {
            if let Ok(track) = v.dyn_into::<MediaStreamTrack>() {
                return Ok(track);
            }
        }
    }
    // Fallback: MediaStreamTrackGenerator сам является track'ом.
    generator
        .clone()
        .dyn_into::<MediaStreamTrack>()
        .map_err(|_| {
            JsValue::from_str(
                "generator не является MediaStreamTrack и не имеет .track",
            )
        })
}

/// `generator.readyState` — `"live"` или `"ended"`.
///
/// Публичная утилита для UI и тестов: показывает, можно ли
/// ещё писать в `writable` (после `stop()` — нельзя).
pub fn generator_ready_state(generator: &JsValue) -> String {
    js_sys::Reflect::get(generator, &JsValue::from_str("readyState"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// `generator.stop()` — завершает трек. После этого писать нельзя.
pub fn generator_stop(generator: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(generator, "stop")?;
    let _ = f.call0(generator)?;
    Ok(())
}

// =============================================================================
// MediaStream
// =============================================================================

/// `new MediaStream([track])`.
///
/// Обёртка для превращения отдельного трека в готовый поток,
/// который принимает `video.srcObject`.
pub fn make_media_stream(track: &MediaStreamTrack) -> Result<MediaStream, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("MediaStream"))?;
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let tracks_arr = js_sys::Array::new();
    tracks_arr.push(track.as_ref());
    let args = js_sys::Array::new();
    args.push(&tracks_arr);
    let stream = js_sys::Reflect::construct(&ctor, &args)?;
    stream.dyn_into::<MediaStream>()
}

/// Назначает `MediaStream` в `video.srcObject`.
///
/// Пытается вызвать `play()`, но не блокируется: если автоплей
/// запрещён политикой браузера — просто игнорируем. Атрибуты
/// `autoplay` / `muted` в HTML обычно решают проблему.
pub fn attach_stream_to_video(
    video: &HtmlVideoElement,
    stream: &MediaStream,
) -> Result<(), JsValue> {
    set_prop(video.as_ref(), "srcObject", stream.as_ref())?;
    let play = js_sys::Reflect::get(video.as_ref(), &JsValue::from_str("play"))?;
    if let Ok(play) = play.dyn_into::<js_sys::Function>() {
        if let Ok(promise) = play.call0(video.as_ref()) {
            if let Ok(promise) = promise.dyn_into::<js_sys::Promise>() {
                wasm_bindgen_futures::spawn_local(async move {
                    // play() может отклониться, если нет user gesture.
                    // Автоплей-muted обычно разрешён.
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                });
            }
        }
    }
    Ok(())
}

/// Останавливает все видеотреки в `MediaStream`.
///
/// Идемпотентно: повторный вызов не паникует.
pub fn stop_stream(stream: &MediaStream) {
    let tracks = stream.get_video_tracks();
    for i in 0..tracks.length() {
        if let Some(t) = tracks.get(i).dyn_into::<MediaStreamTrack>().ok() {
            t.stop();
        }
    }
}

// =============================================================================
// Утилиты для VideoFrame
// =============================================================================

/// Читает фактические размеры `VideoFrame`.
///
/// Пробует `displayWidth` / `displayHeight` — они дают видимую
/// область. Если поле отсутствует или равно нулю — fallback на
/// `codedWidth` / `codedHeight`. Возвращает `(0, 0)`, если оба
/// источника недоступны.
pub fn read_frame_size(frame: &JsValue) -> (u32, u32) {
    let read = |key: &str| -> f64 {
        js_sys::Reflect::get(frame, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };

    let mut w = read("displayWidth");
    if w <= 0.0 {
        w = read("codedWidth");
    }
    let mut h = read("displayHeight");
    if h <= 0.0 {
        h = read("codedHeight");
    }
    (w.max(0.0) as u32, h.max(0.0) as u32)
}

/// `frame.timestamp` — микросекунды. `0.0`, если поля нет.
pub fn read_frame_timestamp(frame: &JsValue) -> f64 {
    js_sys::Reflect::get(frame, &JsValue::from_str("timestamp"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// `frame.duration` — микросекунды. `None`, если поля нет.
pub fn read_frame_duration(frame: &JsValue) -> Option<f64> {
    js_sys::Reflect::get(frame, &JsValue::from_str("duration"))
        .ok()
        .and_then(|v| {
            if v.is_null() || v.is_undefined() {
                None
            } else {
                v.as_f64()
            }
        })
}

/// Форматирует размер `VideoFrame` как `320×240`.
///
/// Возвращает `"0×0"` при отсутствии размеров.
pub fn format_frame_size(frame: &JsValue) -> String {
    let (w, h) = read_frame_size(frame);
    format_str(format_args!("{w}×{h}"))
}

/// `frame.close()`. Идемпотентно — повторный вызов безопасен.
pub fn close_video_frame(frame: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(frame, "close")?;
    let _ = f.call0(frame)?;
    Ok(())
}

/// Создаёт `VideoFrame` из `ImageBitmap` с заданными метаданными.
///
/// `ImageBitmap` становится источником пикселей; `timestamp` и
/// `duration` задаются явно, чтобы сохранить временную
/// последовательность исходного потока.
pub fn make_video_frame_from_bitmap(
    bitmap: &JsValue,
    timestamp_us: f64,
    duration_us: Option<f64>,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("VideoFrame"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("VideoFrame not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let init = js_sys::Object::new();
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    if let Some(d) = duration_us {
        set_prop(&init, "duration", &num(d))?;
    }
    let args = js_sys::Array::new();
    args.push(bitmap);
    args.push(&init);
    js_sys::Reflect::construct(&ctor, &args)
}

/// `bitmap.close()`. Освобождает ресурсы ImageBitmap.
pub fn close_image_bitmap(bitmap: &JsValue) -> Result<(), JsValue> {
    let f = get_fn(bitmap, "close")?;
    let _ = f.call0(bitmap)?;
    Ok(())
}

// =============================================================================
// DOM
// =============================================================================

/// Утилита для app.rs: создаёт `<video>` с правильными атрибутами.
///
/// `autoplay` + `muted` + `playsinline` — единственная комбинация,
/// которая гарантирует автоплей без user gesture. Возвращает
/// типизированный `HtmlVideoElement`.
pub fn create_video_element(
    document: &Document,
    id: &str,
) -> Result<HtmlVideoElement, JsValue> {
    let el = document.create_element("video")?;
    el.set_id(id);
    el.set_attribute("autoplay", "")?;
    el.set_attribute("muted", "")?;
    el.set_attribute("playsinline", "")?;
    // create_element("video") возвращает Element. dyn_into в
    // HtmlVideoElement даёт ошибку типа Element, а сигнатура
    // функции ждёт JsValue — конвертируем.
    el.dyn_into::<HtmlVideoElement>()
        .map_err(|_| JsValue::from_str("create_element(\"video\") returned non-video element"))
}