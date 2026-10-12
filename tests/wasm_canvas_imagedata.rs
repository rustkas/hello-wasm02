#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement};

use hello_wasm02::canvas_imagedata::{
    image_data_to_vec, read_image_data_cfg, read_pixel, vec_to_image_data,
    write_image_data_cfg,
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

#[wasm_bindgen_test]
fn get_image_data_returns_painted_color() {
    let (_c, ctx) = make_canvas(10, 10);
    ctx.set_fill_style_str("#ff0000");
    ctx.fill_rect(0.0, 0.0, 10.0, 10.0);

    let data = read_image_data_cfg(&ctx, 0, 0, 10, 10).unwrap();
    assert_eq!(data.width(), 10);
    assert_eq!(data.height(), 10);
    let pixels = image_data_to_vec(&data);
    let (r, g, b, a) = read_pixel(&pixels, 5, 5, 10);
    assert_eq!((r, g, b, a), (255, 0, 0, 255));
}

#[wasm_bindgen_test]
fn vec_to_image_data_roundtrip() {
    let w = 4u32;
    let h = 3u32;
    let original: Vec<u8> = (0..w * h * 4).map(|i| (i & 0xFF) as u8).collect();
    let data = vec_to_image_data(&original, w, h).unwrap();
    assert_eq!(data.width(), w);
    assert_eq!(data.height(), h);
    let back = image_data_to_vec(&data);
    assert_eq!(back, original);
}

#[wasm_bindgen_test]
fn vec_to_image_data_rejects_wrong_size() {
    let r = vec_to_image_data(&[0u8; 5], 2, 2);
    assert!(r.is_err());
}

#[wasm_bindgen_test]
fn put_image_data_ignores_scale() {
    // Ключевой тест главы: putImageData не подчиняется трансформациям.
    //
    // Холст 40×40 физических пикселей. При scale(4, 4) в CSS-координатах
    // доступна область 10×10. Это позволяет проверить обе стороны:
    //   - putImageData(data, 0, 0) — остаётся 1×1, не растягивается;
    //   - fillRect в CSS-координатах — растягивается ×4.
    let (_c, ctx) = make_canvas(40, 40);
    ctx.scale(4.0, 4.0).expect("scale");

    // --- Шаг 1: putImageData игнорирует scale ---
    // Пишем один красный пиксель через ImageData.
    let red = vec_to_image_data(&[255, 0, 0, 255], 1, 1).unwrap();
    write_image_data_cfg(&ctx, &red, 0, 0).unwrap();

    let data = read_image_data_cfg(&ctx, 0, 0, 40, 40).unwrap();
    let pixels = image_data_to_vec(&data);

    // Пиксель (0,0) красный. Пиксели (1,0), (2,0), (3,0) — НЕ красные.
    // Если бы putImageData подчинялся scale, они были бы красными.
    let (r0, _, _, _) = read_pixel(&pixels, 0, 0, 40);
    let (r1, _, _, _) = read_pixel(&pixels, 1, 0, 40);
    let (r2, _, _, _) = read_pixel(&pixels, 2, 0, 40);
    let (r3, _, _, _) = read_pixel(&pixels, 3, 0, 40);
    assert_eq!(r0, 255, "pixel (0,0) should be red");
    assert!(r1 < 40, "pixel (1,0) should NOT be red — putImageData ignores scale");
    assert!(r2 < 40, "pixel (2,0) should NOT be red");
    assert!(r3 < 40, "pixel (3,0) should NOT be red");

    // --- Шаг 2: fillRect подчиняется scale ---
    // CSS-координаты (0, 5), размер 1×1. При scale(4, 4) это:
    //   физические координаты (0, 20), размер 4×4.
    // То есть прямоугольник занимает пиксели от (0, 20) до (3, 23) включительно.
    ctx.set_fill_style_str("#00ff00");
    ctx.fill_rect(0.0, 5.0, 1.0, 1.0);

    let data2 = read_image_data_cfg(&ctx, 0, 0, 40, 40).unwrap();
    let pixels2 = image_data_to_vec(&data2);

    // Верхний левый угол прямоугольника — зелёный.
    let (_, g_tl, _, _) = read_pixel(&pixels2, 0, 20, 40);
    // Правый нижний угол прямоугольника — тоже зелёный.
    let (_, g_br, _, _) = read_pixel(&pixels2, 3, 23, 40);
    // Пиксель сразу за прямоугольником (x = 4) — НЕ зелёный.
    let (_, g_beyond_x, _, _) = read_pixel(&pixels2, 4, 23, 40);
    // Пиксель под прямоугольником (y = 24) — НЕ зелёный.
    let (_, g_beyond_y, _, _) = read_pixel(&pixels2, 0, 24, 40);

    assert!(g_tl > 200, "fillRect IS scaled (top-left of scaled rect)");
    assert!(g_br > 200, "fillRect IS scaled (bottom-right of scaled rect)");
    assert!(g_beyond_x < 40, "pixel right of scaled rect should not be green");
    assert!(g_beyond_y < 40, "pixel below scaled rect should not be green");
}

#[wasm_bindgen_test]
fn put_image_data_ignores_global_alpha() {
    // globalAlpha — не должен влиять на putImageData. Пишем полупрозрачный
    // красный поверх белого фона; при globalAlpha = 0.5 обычная заливка
    // дала бы розовый, а putImageData — чистый красный.
    let (_c, ctx) = make_canvas(4, 4);
    ctx.set_fill_style_str("#ffffff");
    ctx.fill_rect(0.0, 0.0, 4.0, 4.0);

    ctx.set_global_alpha(0.5);

    let red = vec_to_image_data(&[255, 0, 0, 255], 1, 1).unwrap();
    write_image_data_cfg(&ctx, &red, 1, 1).unwrap();

    let data = read_image_data_cfg(&ctx, 0, 0, 4, 4).unwrap();
    let pixels = image_data_to_vec(&data);
    let (r, g, _, _) = read_pixel(&pixels, 1, 1, 4);
    assert_eq!(r, 255, "putImageData ignores globalAlpha");
    assert_eq!(g, 0, "putImageData writes raw pixel");
}