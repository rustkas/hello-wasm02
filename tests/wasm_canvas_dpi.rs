#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Document, HtmlCanvasElement};

use hello_wasm02::canvas_dpi::{
    draw_test_pattern, get_device_pixel_ratio, setup_canvas_hidpi, setup_canvas_naive,
};

wasm_bindgen_test_configure!(run_in_browser);

fn create_canvas() -> HtmlCanvasElement {
    let document: Document = web_sys::window().unwrap().document().unwrap();
    document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap()
}

#[wasm_bindgen_test]
fn dpr_is_at_least_one_in_browser() {
    let dpr = get_device_pixel_ratio();
    assert!(dpr >= 1.0, "DPR should be >= 1.0, got {dpr}");
    assert!(dpr.is_finite());
}

#[wasm_bindgen_test]
fn naive_sets_buffer_equal_to_css_size() {
    let canvas = create_canvas();
    let _ctx = setup_canvas_naive(&canvas, 400.0, 200.0).expect("setup naive");
    assert_eq!(canvas.width(), 400);
    assert_eq!(canvas.height(), 200);
    // CSS-размер через style совпадает с буфером.
    assert_eq!(canvas.style().get_property_value("width").unwrap(), "400px");
    assert_eq!(canvas.style().get_property_value("height").unwrap(), "200px");
}

#[wasm_bindgen_test]
fn hidpi_multiplies_buffer_by_dpr() {
    let canvas = create_canvas();
    let _ctx = setup_canvas_hidpi(&canvas, 400.0, 200.0, 2.0).expect("setup hidpi");
    assert_eq!(canvas.width(), 800);
    assert_eq!(canvas.height(), 400);
    // CSS-размер остаётся логическим.
    assert_eq!(canvas.style().get_property_value("width").unwrap(), "400px");
    assert_eq!(canvas.style().get_property_value("height").unwrap(), "200px");
}

#[wasm_bindgen_test]
fn hidpi_at_dpr_1_equals_naive() {
    let c1 = create_canvas();
    let c2 = create_canvas();
    let _ = setup_canvas_naive(&c1, 300.0, 150.0).unwrap();
    let _ = setup_canvas_hidpi(&c2, 300.0, 150.0, 1.0).unwrap();
    assert_eq!(c1.width(), c2.width());
    assert_eq!(c1.height(), c2.height());
}

#[wasm_bindgen_test]
fn draw_pattern_smoke() {
    // Функция должна выполняться без паники на обоих setup-вариантах.
    let c1 = create_canvas();
    let ctx1 = setup_canvas_naive(&c1, 200.0, 100.0).unwrap();
    draw_test_pattern(&ctx1, 200.0, 100.0).expect("naive draw");

    let c2 = create_canvas();
    let ctx2 = setup_canvas_hidpi(&c2, 200.0, 100.0, 2.0).unwrap();
    draw_test_pattern(&ctx2, 200.0, 100.0).expect("hidpi draw");
}