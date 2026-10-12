#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use hello_wasm02::video_decoder::{
    close, configure, has_is_config_supported, hex_dump,
    is_video_decoder_supported, make_video_decoder,
};
use hello_wasm02::video_frame::{
    create_encoded_chunk, is_encoded_chunk_supported,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn video_decoder_is_supported_in_browser() {
    assert!(
        is_video_decoder_supported(),
        "VideoDecoder should be available",
    );
}

#[wasm_bindgen_test]
fn encoded_chunk_is_supported_in_browser() {
    assert!(is_encoded_chunk_supported());
}

#[wasm_bindgen_test]
fn is_config_supported_is_available() {
    let _ = has_is_config_supported();
}

#[wasm_bindgen_test]
fn make_decoder_with_callbacks_does_not_throw() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            |_frame: wasm_bindgen::JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            |_e: wasm_bindgen::JsValue| {},
        );
    let decoder = make_video_decoder(&on_output, &on_error);
    assert!(decoder.is_ok(), "VideoDecoder should be constructible");
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn configure_without_description_does_not_throw_sync() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            |_frame: wasm_bindgen::JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            |_e: wasm_bindgen::JsValue| {},
        );
    let decoder =
        make_video_decoder(&on_output, &on_error).expect("construct");
    let result = configure(&decoder, "avc1.42001E", 320, 240, None);
    assert!(
        result.is_ok(),
        "configure without description does not throw synchronously",
    );
    let _ = close(&decoder);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn close_does_not_throw() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            |_frame: wasm_bindgen::JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            |_e: wasm_bindgen::JsValue| {},
        );
    let decoder = make_video_decoder(&on_output, &on_error).expect("construct");
    close(&decoder).expect("close");
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn hex_dump_renders_uint8array() {
    let data = vec![0x01u8, 0x42, 0x00, 0x1E, 0xFF];
    let arr = js_sys::Uint8Array::from(&data[..]);
    let s = hex_dump(&arr, 64);
    assert!(s.contains("01"));
    assert!(s.contains("42"));
    assert!(s.contains("1e"));
    assert!(s.contains("ff"));
}

#[wasm_bindgen_test]
fn hex_dump_renders_array_buffer() {
    // Практическая находка: description в Chrome приходит как
    // ArrayBuffer, а не Uint8Array. hex_dump должен его понимать.
    let data = vec![0x67u8, 0x42, 0x00, 0x1E]; // SPS start
    let u8 = js_sys::Uint8Array::from(&data[..]);
    let ab: js_sys::ArrayBuffer = u8.buffer();
    let s = hex_dump(&ab, 64);
    assert!(s.contains("67"));
    assert!(s.contains("42"));
    assert!(s.contains("1e"));
}

#[wasm_bindgen_test]
fn hex_dump_truncates_at_max() {
    let data: Vec<u8> = (0..100).map(|i| (i & 0xFF) as u8).collect();
    let arr = js_sys::Uint8Array::from(&data[..]);
    let s = hex_dump(&arr, 10);
    assert!(s.contains("всего 100"), "got: {s}");
}

#[wasm_bindgen_test]
fn hex_dump_handles_non_buffer() {
    let v = wasm_bindgen::JsValue::from_str("not bytes");
    let s = hex_dump(&v, 16);
    assert!(
        s.contains("BufferSource") || s.contains("пуст"),
        "expected fallback message, got: {s}",
    );
}

#[wasm_bindgen_test]
fn encoded_chunk_can_be_created() {
    let data = vec![0u8; 128];
    let chunk = create_encoded_chunk("key", 0.0, 33333.0, &data)
        .expect("create chunk");
    assert!(!chunk.is_null());
    assert!(!chunk.is_undefined());
    let _ = chunk.dyn_into::<js_sys::Object>();
}