#![cfg(target_arch = "wasm32")]

use js_sys::Object;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use hello_wasm02::video_encoder::{
    chunk_byte_length, chunk_type, close, configure,
    has_is_config_supported, is_config_supported,
    is_video_encoder_supported, make_video_encoder, reset,
};
use hello_wasm02::video_frame::{num, set_prop};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn video_encoder_is_supported_in_browser() {
    assert!(
        is_video_encoder_supported(),
        "VideoEncoder should be available",
    );
}

#[wasm_bindgen_test]
fn is_config_supported_is_available() {
    let _ = has_is_config_supported();
}

#[wasm_bindgen_test]
async fn is_config_supported_avc1_baseline() {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str("avc1.42001E")).unwrap();
    set_prop(&cfg, "width", &num(320.0)).unwrap();
    set_prop(&cfg, "height", &num(240.0)).unwrap();
    set_prop(&cfg, "bitrate", &num(1_000_000.0)).unwrap();
    set_prop(&cfg, "framerate", &num(30.0)).unwrap();
    let result = is_config_supported(&cfg).await;
    assert!(result.is_ok(), "avc1.42001E 320x240 1Mbps should be supported");
}

#[wasm_bindgen_test]
fn make_encoder_does_not_throw() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_chunk: JsValue, _meta: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error);
    assert!(enc.is_ok(), "VideoEncoder should be constructible");
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn configure_and_close_do_not_throw() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_c: JsValue, _m: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");
    let result = configure(&enc, "avc1.42001E", 320, 240, 1_000_000, 30.0);
    assert!(result.is_ok(), "configure should not throw");
    let _ = close(&enc);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn reset_does_not_throw() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_c: JsValue, _m: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");
    configure(&enc, "avc1.42001E", 320, 240, 1_000_000, 30.0).unwrap();
    let _ = reset(&enc);
    let _ = close(&enc);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn chunk_helpers_return_empty_for_non_chunk() {
    let v = JsValue::from_str("not a chunk");
    assert!(chunk_type(&v).is_empty());
    assert_eq!(chunk_byte_length(&v), 0);
}