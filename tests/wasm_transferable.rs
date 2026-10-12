#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::transferable::{
    byte_length, create_message_channel, is_array_buffer_supported,
    is_message_channel_supported, make_buffer, port1, port2, start_port,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn message_channel_is_supported() {
    assert!(is_message_channel_supported(), "MessageChannel available");
}

#[wasm_bindgen_test]
fn array_buffer_is_supported() {
    assert!(is_array_buffer_supported(), "ArrayBuffer available");
}

#[wasm_bindgen_test]
fn make_buffer_returns_correct_size() {
    let (_buf, len) = make_buffer(1024);
    assert_eq!(len, 1024.0);
}

#[wasm_bindgen_test]
fn byte_length_of_fresh_buffer() {
    let (buf, len) = make_buffer(2048);
    assert_eq!(len, 2048.0);
    assert_eq!(byte_length(&buf), 2048.0);
}

#[wasm_bindgen_test]
fn byte_length_of_non_buffer_is_minus_one() {
    let v = wasm_bindgen::JsValue::from_str("not a buffer");
    assert_eq!(byte_length(&v), -1.0);
}

#[wasm_bindgen_test]
fn message_channel_has_two_ports() {
    let ch = create_message_channel().expect("create");
    let p1 = port1(&ch);
    let p2 = port2(&ch);
    // Порта — объекты; проверяем что start не паникует.
    start_port(&p1);
    start_port(&p2);
}