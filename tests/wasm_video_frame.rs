#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::HtmlCanvasElement;

use hello_wasm02::video_frame::{
    chunk_byte_length, chunk_timestamp, chunk_type, close, copy_to_rgba,
    create_encoded_chunk, create_from_canvas, height, is_encoded_chunk_supported,
    is_video_frame_supported, support_summary, timestamp, width,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_canvas(w: u32, h: u32) -> HtmlCanvasElement {
    let doc = web_sys::window().unwrap().document().unwrap();
    let canvas: HtmlCanvasElement = doc
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(w);
    canvas.set_height(h);
    canvas
}

#[wasm_bindgen_test]
fn video_frame_is_supported_in_browser() {
    assert!(is_video_frame_supported(), "VideoFrame should be available");
}

#[wasm_bindgen_test]
fn encoded_chunk_is_supported_in_browser() {
    assert!(
        is_encoded_chunk_supported(),
        "EncodedVideoChunk should be available"
    );
}

#[wasm_bindgen_test]
fn support_summary_returns_string() {
    let s = support_summary();
    assert!(s.contains("VideoFrame"));
    assert!(s.contains("EncodedVideoChunk"));
}

#[wasm_bindgen_test]
fn create_frame_from_canvas_has_correct_size() {
    let canvas = make_canvas(320, 240);
    let frame = create_from_canvas(&canvas, 0.0).expect("create frame");
    assert_eq!(width(&frame), 320);
    assert_eq!(height(&frame), 240);
    close(&frame).unwrap();
}

#[wasm_bindgen_test]
fn frame_timestamp_is_preserved() {
    let canvas = make_canvas(64, 64);
    let frame = create_from_canvas(&canvas, 12345.0).expect("create frame");
    assert_eq!(timestamp(&frame), 12345.0);
    close(&frame).unwrap();
}

#[wasm_bindgen_test]
fn close_is_idempotent() {
    let canvas = make_canvas(64, 64);
    let frame = create_from_canvas(&canvas, 0.0).expect("create frame");
    // Первый close.
    close(&frame).expect("first close");
    // Второй close — не должен бросать.
    close(&frame).expect("second close must not throw");
}

#[wasm_bindgen_test]
fn copy_to_rgba_returns_correct_length() {
    let canvas = make_canvas(32, 16);
    let frame = create_from_canvas(&canvas, 0.0).expect("create frame");
    let buf = copy_to_rgba(&frame).expect("copyTo");
    assert_eq!(buf.length(), 32 * 16 * 4);
    close(&frame).unwrap();
}

#[wasm_bindgen_test]
fn encoded_chunk_key_type() {
    let data = vec![0u8; 1024];
    let chunk =
        create_encoded_chunk("key", 0.0, 33333.0, &data).expect("create chunk");
    assert_eq!(chunk_type(&chunk), "key");
    assert_eq!(chunk_timestamp(&chunk), 0.0);
    assert_eq!(chunk_byte_length(&chunk), 1024);
}

#[wasm_bindgen_test]
fn encoded_chunk_delta_type() {
    let data = vec![0u8; 256];
    let chunk =
        create_encoded_chunk("delta", 33333.0, 33333.0, &data).expect("create chunk");
    assert_eq!(chunk_type(&chunk), "delta");
    assert_eq!(chunk_timestamp(&chunk), 33333.0);
    assert_eq!(chunk_byte_length(&chunk), 256);
}