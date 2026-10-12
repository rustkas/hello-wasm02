#![cfg(target_arch = "wasm32")]

use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use hello_wasm02::media_stream::{
    capture_canvas_stream, close_image_bitmap, close_video_frame,
    create_video_element, format_frame_size, generator_ready_state,
    generator_track, generator_writable, has_active_service_worker,
    has_capture_stream, has_get_user_media, has_track_generator,
    has_track_processor, make_media_stream, make_track_generator,
    make_track_processor, make_video_frame_from_bitmap,
    processor_readable, read_frame_duration, read_frame_size,
    read_frame_timestamp, stop_stream,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Хелперы
// =============================================================================

/// Создаёт canvas 64×48 и запускает с него MediaStream.
fn make_canvas_stream() -> web_sys::MediaStream {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(64);
    canvas.set_height(48);
    capture_canvas_stream(&canvas, 30.0).expect("captureStream")
}

/// Создаёт VideoFrame из canvas 64×48 с timestamp 1234 µs.
fn make_canvas_frame() -> JsValue {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(64);
    canvas.set_height(48);
    let ctor = Reflect::get(&js_sys::global(), &JsValue::from_str("VideoFrame"))
        .unwrap()
        .dyn_into::<js_sys::Function>()
        .unwrap();
    let init = js_sys::Object::new();
    Reflect::set(&init, &JsValue::from_str("timestamp"), &JsValue::from_f64(1234.0))
        .unwrap();
    let args = js_sys::Array::new();
    args.push(&JsValue::from(canvas));
    args.push(&init);
    Reflect::construct(&ctor, &args).expect("VideoFrame")
}

/// Создаёт ImageBitmap из canvas 64×48.
async fn make_test_bitmap() -> JsValue {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(64);
    canvas.set_height(48);
    let ctor = Reflect::get(&js_sys::global(), &JsValue::from_str("createImageBitmap"))
        .unwrap()
        .dyn_into::<js_sys::Function>()
        .unwrap();
    let promise = ctor.call1(&JsValue::NULL, &JsValue::from(canvas)).unwrap();
    let promise: js_sys::Promise = promise.dyn_into().unwrap();
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap()
}

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
fn feature_detection_does_not_panic() {
    let _ = has_track_processor();
    let _ = has_track_generator();
    let _ = has_get_user_media();
    let _ = has_capture_stream();
    let _ = has_active_service_worker();
}

#[wasm_bindgen_test]
fn capture_stream_is_available_in_browser() {
    // canvas.captureStream — baseline. Должен работать в любом
    // headless-браузере.
    assert!(
        has_capture_stream(),
        "canvas.captureStream должен быть доступен",
    );
}

// =============================================================================
// canvas.captureStream → MediaStream
// =============================================================================

#[wasm_bindgen_test]
fn canvas_capture_stream_produces_one_video_track() {
    if !has_capture_stream() {
        return;
    }
    let stream = make_canvas_stream();
    assert_eq!(stream.get_video_tracks().length(), 1);
}

#[wasm_bindgen_test]
fn stop_stream_is_idempotent() {
    if !has_capture_stream() {
        return;
    }
    let stream = make_canvas_stream();
    stop_stream(&stream);
    // Повторный вызов — не паникует.
    stop_stream(&stream);

    let tracks = stream.get_video_tracks();
    let track = tracks
        .get(0)
        .dyn_into::<web_sys::MediaStreamTrack>()
        .expect("track 0");
    assert_eq!(track.ready_state(), web_sys::MediaStreamTrackState::Ended);
}

// =============================================================================
// MediaStreamTrackProcessor
// =============================================================================

#[wasm_bindgen_test]
fn track_processor_constructs_from_canvas_track() {
    if !has_track_processor() || !has_capture_stream() {
        return;
    }
    let stream = make_canvas_stream();
    let track = stream
        .get_video_tracks()
        .get(0)
        .dyn_into::<web_sys::MediaStreamTrack>()
        .expect("track 0");

    let processor = make_track_processor(&track);
    assert!(
        processor.is_ok(),
        "MediaStreamTrackProcessor: {:?}",
        processor.err(),
    );

    let readable = processor_readable(&processor.unwrap());
    assert!(
        readable.is_ok(),
        "processor.readable должен быть доступен",
    );
    let readable = readable.unwrap();
    assert!(!readable.is_null() && !readable.is_undefined());
}

// =============================================================================
// MediaStreamTrackGenerator
// =============================================================================

#[wasm_bindgen_test]
fn track_generator_constructs_with_live_state() {
    if !has_track_generator() {
        return;
    }
    let generator = make_track_generator();
    assert!(
        generator.is_ok(),
        "MediaStreamTrackGenerator: {:?}",
        generator.err(),
    );

    let generator = generator.unwrap();
    assert_eq!(generator_ready_state(&generator), "live");

    let writable = generator_writable(&generator);
    assert!(
        writable.is_ok(),
        "generator.writable должен быть доступен",
    );

    // MediaStreamTrackGenerator **сам является** MediaStreamTrack —
    // наследует его. `generator_track` учитывает это и приводит
    // сам generator, если поля `.track` нет.
    let track = generator_track(&generator);
    assert!(
        track.is_ok(),
        "generator_track: {:?}",
        track.err(),
    );
}

#[wasm_bindgen_test]
fn media_stream_from_generator_track() {
    if !has_track_generator() {
        return;
    }
    let generator = make_track_generator().expect("generator");
    let track = generator_track(&generator).expect("track");

    let stream = make_media_stream(&track);
    assert!(stream.is_ok(), "MediaStream: {:?}", stream.err());

    let stream = stream.unwrap();
    assert_eq!(stream.get_video_tracks().length(), 1);
}

// =============================================================================
// VideoFrame: чтение размеров и метаданных
// =============================================================================

#[wasm_bindgen_test]
fn read_frame_size_from_canvas_frame() {
    let frame = make_canvas_frame();
    let (w, h) = read_frame_size(&frame);
    assert_eq!(w, 64, "ширина canvas-based VideoFrame");
    assert_eq!(h, 48, "высота canvas-based VideoFrame");
    let _ = close_video_frame(&frame);
}

#[wasm_bindgen_test]
fn format_frame_size_matches_read_frame_size() {
    let frame = make_canvas_frame();
    let s = format_frame_size(&frame);
    assert_eq!(s, "64×48");
    let _ = close_video_frame(&frame);
}

#[wasm_bindgen_test]
fn read_frame_timestamp_returns_value() {
    let frame = make_canvas_frame();
    let ts = read_frame_timestamp(&frame);
    assert_eq!(ts, 1234.0);
    let _ = close_video_frame(&frame);
}

#[wasm_bindgen_test]
fn read_frame_duration_optional() {
    let frame = make_canvas_frame();
    // Мы не задавали duration — должно вернуться None.
    let dur = read_frame_duration(&frame);
    assert!(dur.is_none(), "duration не задавался → None, got {:?}", dur);
    let _ = close_video_frame(&frame);
}

#[wasm_bindgen_test]
fn close_video_frame_is_idempotent() {
    let frame = make_canvas_frame();
    assert!(close_video_frame(&frame).is_ok());
    // Повторный close: InvalidStateError в JS, но не паника.
    let _ = close_video_frame(&frame);
}

// =============================================================================
// VideoFrame из ImageBitmap
// =============================================================================

#[wasm_bindgen_test]
async fn make_video_frame_from_bitmap_with_metadata() {
    let bitmap = make_test_bitmap().await;

    let frame = make_video_frame_from_bitmap(&bitmap, 999.0, Some(33_333.0));
    assert!(frame.is_ok(), "VideoFrame: {:?}", frame.err());

    let frame = frame.unwrap();

    let ts = read_frame_timestamp(&frame);
    assert_eq!(ts, 999.0);

    let (w, h) = read_frame_size(&frame);
    assert_eq!(w, 64);
    assert_eq!(h, 48);

    let _ = close_video_frame(&frame);
    let _ = close_image_bitmap(&bitmap);
}

#[wasm_bindgen_test]
async fn make_video_frame_from_bitmap_without_duration() {
    let bitmap = make_test_bitmap().await;

    let frame = make_video_frame_from_bitmap(&bitmap, 0.0, None);
    assert!(frame.is_ok());
    let frame = frame.unwrap();

    let dur = read_frame_duration(&frame);
    assert!(dur.is_none());

    let _ = close_video_frame(&frame);
    let _ = close_image_bitmap(&bitmap);
}

// =============================================================================
// create_video_element: атрибуты
// =============================================================================

#[wasm_bindgen_test]
fn create_video_element_sets_required_attributes() {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let video = create_video_element(&document, "test-video");
    assert!(video.is_ok(), "create_video_element: {:?}", video.err());

    let video = video.unwrap();
    assert_eq!(video.id(), "test-video");
    assert!(video.has_attribute("autoplay"));
    assert!(video.has_attribute("muted"));
    assert!(video.has_attribute("playsinline"));
}

// =============================================================================
// getUserMedia — требует user gesture
// =============================================================================

#[wasm_bindgen_test]
#[ignore = "требует user gesture — проверяется вручную через демо"]
async fn get_user_media_requires_gesture() {
    let result = hello_wasm02::media_stream::get_camera_stream().await;
    assert!(result.is_err());
}