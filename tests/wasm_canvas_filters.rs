#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, ImageData};

use hello_wasm02::canvas_filters::{
    has_simd, invert, make_pattern, to_grayscale, Filter,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_canvas(w: u32, h: u32) -> (HtmlCanvasElement, CanvasRenderingContext2d) {
    let doc: Document = web_sys::window().unwrap().document().unwrap();
    let c: HtmlCanvasElement = doc.create_element("canvas").unwrap().dyn_into().unwrap();
    c.set_width(w);
    c.set_height(h);
    let ctx: CanvasRenderingContext2d = c
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    (c, ctx)
}

/// Читает RGBA в точке `(x, y)`. Скрывает разницу между
/// `f64`- и `i32`-версиями `getImageData`.
fn read_pixel(ctx: &CanvasRenderingContext2d, x: f64, y: f64) -> Vec<u8> {
    #[cfg(web_sys_unstable_apis)]
    let data = ctx.get_image_data(x as i32, y as i32, 1, 1).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    let data = ctx.get_image_data(x, y, 1.0, 1.0).unwrap();

    data.data().0
}

/// Полный round-trip: canvas → ImageData → Vec → фильтр → Vec → ImageData → canvas.
fn apply_filter_to_canvas(
    ctx: &CanvasRenderingContext2d,
    filter: Filter,
    w: u32,
    h: u32,
) {
    #[cfg(web_sys_unstable_apis)]
    let data = ctx.get_image_data(0, 0, w as i32, h as i32).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    let data = ctx.get_image_data(0.0, 0.0, w as f64, h as f64).unwrap();

    let pixels = data.data().0;
    let processed = filter.apply(&pixels, w, h);
    let clamped = wasm_bindgen::Clamped(processed.as_slice());
    let new_data = ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h).unwrap();

    #[cfg(web_sys_unstable_apis)]
    ctx.put_image_data(&new_data, 0, 0).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    ctx.put_image_data(&new_data, 0.0, 0.0).unwrap();
}

#[wasm_bindgen_test]
fn invert_filter_on_canvas_roundtrip() {
    let (_c, ctx) = make_canvas(4, 4);
    // Красный фон.
    ctx.set_fill_style_str("#ff0000");
    ctx.fill_rect(0.0, 0.0, 4.0, 4.0);

    apply_filter_to_canvas(&ctx, Filter::Invert, 4, 4);

    // После инверсии красный (255,0,0) → cyan (0,255,255).
    let px = read_pixel(&ctx, 2.0, 2.0);
    assert_eq!(px[0], 0, "red should be 0");
    assert_eq!(px[1], 255, "green should be 255");
    assert_eq!(px[2], 255, "blue should be 255");
    assert_eq!(px[3], 255, "alpha unchanged");
}

#[wasm_bindgen_test]
fn grayscale_filter_on_canvas_roundtrip() {
    let (_c, ctx) = make_canvas(4, 4);
    ctx.set_fill_style_str("#ff0000");
    ctx.fill_rect(0.0, 0.0, 4.0, 4.0);

    apply_filter_to_canvas(&ctx, Filter::Grayscale, 4, 4);

    let px = read_pixel(&ctx, 2.0, 2.0);
    // Grayscale красного ≈ 54.
    assert_eq!(px[0], px[1], "grayscale should have R == G");
    assert_eq!(px[1], px[2], "grayscale should have G == B");
    assert!(px[0] >= 53 && px[0] <= 55, "got {}", px[0]);
}

#[wasm_bindgen_test]
fn pure_function_operates_on_vec_without_canvas() {
    // Проверяем, что чистые функции работают на произвольных Vec<u8>
    // без участия canvas.
    let px = vec![10, 20, 30, 40];
    let out = invert(&px);
    assert_eq!(out, vec![245, 235, 225, 40]);

    let gray = to_grayscale(&[255, 0, 0, 255]);
    assert!(gray[0] > 50 && gray[0] < 60);
}

#[wasm_bindgen_test]
fn pattern_roundtrip_via_canvas() {
    let w = 32;
    let h = 24;
    let (_c, ctx) = make_canvas(w, h);
    let pattern = make_pattern(w, h);

    let clamped = wasm_bindgen::Clamped(pattern.as_slice());
    let data = ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h).unwrap();

    #[cfg(web_sys_unstable_apis)]
    ctx.put_image_data(&data, 0, 0).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    ctx.put_image_data(&data, 0.0, 0.0).unwrap();

    #[cfg(web_sys_unstable_apis)]
    let read = ctx.get_image_data(0, 0, w as i32, h as i32).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    let read = ctx.get_image_data(0.0, 0.0, w as f64, h as f64).unwrap();

    let back = read.data().0;
    assert_eq!(back.len(), pattern.len());
}

#[wasm_bindgen_test]
fn simd_flag_is_reported() {
    // Функция возвращает bool без паники. Наличие флага зависит от
    // сборки — тест проверяет только корректность вызова.
    let _flag = has_simd();
}