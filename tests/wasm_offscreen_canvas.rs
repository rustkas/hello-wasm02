#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::HtmlCanvasElement;

use hello_wasm02::offscreen_canvas::{
    get_2d, has_convert_to_blob, has_transfer_to_image_bitmap, height_of,
    is_offscreen_canvas_supported, is_transfer_control_supported, new_offscreen,
    transfer_control, width_of,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_canvas() -> HtmlCanvasElement {
    let doc = web_sys::window().unwrap().document().unwrap();
    doc.create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap()
}

#[wasm_bindgen_test]
fn offscreen_canvas_is_supported_in_browser() {
    assert!(
        is_offscreen_canvas_supported(),
        "OffscreenCanvas should be available"
    );
}

#[wasm_bindgen_test]
fn transfer_control_is_supported_in_browser() {
    assert!(
        is_transfer_control_supported(),
        "transferControlToOffscreen should be available"
    );
}

#[wasm_bindgen_test]
fn new_offscreen_has_correct_dimensions() {
    let off = new_offscreen(100, 50).expect("new OffscreenCanvas");
    assert_eq!(width_of(&off), 100);
    assert_eq!(height_of(&off), 50);
}

#[wasm_bindgen_test]
fn transfer_control_returns_same_dimensions() {
    let canvas = make_canvas();
    canvas.set_width(320);
    canvas.set_height(240);
    let off = transfer_control(&canvas).expect("transfer");
    assert_eq!(width_of(&off), 320);
    assert_eq!(height_of(&off), 240);
}

#[wasm_bindgen_test]
fn second_transfer_control_fails() {
    let canvas = make_canvas();
    canvas.set_width(64);
    canvas.set_height(64);
    let _ = transfer_control(&canvas).expect("first transfer");
    let second = transfer_control(&canvas);
    assert!(second.is_err(), "second transfer must fail");
}

#[wasm_bindgen_test]
fn get_2d_on_transferred_works() {
    let canvas = make_canvas();
    canvas.set_width(64);
    canvas.set_height(64);
    let off = transfer_control(&canvas).expect("transfer");
    let _ctx = get_2d(&off).expect("2d context on transferred");
}

#[wasm_bindgen_test]
fn get_2d_on_new_works() {
    let off = new_offscreen(64, 64).expect("new");
    let _ctx = get_2d(&off).expect("2d context on new");
}

#[wasm_bindgen_test]
fn optional_apis_are_detected() {
    let off = new_offscreen(32, 32).expect("new");
    // Наличие методов зависит от браузера; тест проверяет только то,
    // что detection не паникует и возвращает bool.
    let _ = has_convert_to_blob(&off);
    let _ = has_transfer_to_image_bitmap(&off);
}