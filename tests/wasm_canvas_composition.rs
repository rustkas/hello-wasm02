#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement};

use hello_wasm02::canvas_composition::{
    draw_clip_demo, draw_layers, draw_transform_demo, read_transform,
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

/// Читает RGBA в точке `(x, y)`. Скрывает разницу между `f64`- и
/// `i32`-версиями `getImageData` (зависит от `web_sys_unstable_apis`).
fn read_pixel(ctx: &CanvasRenderingContext2d, x: f64, y: f64) -> Vec<u8> {
    #[cfg(web_sys_unstable_apis)]
    let data = ctx.get_image_data(x as i32, y as i32, 1, 1).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    let data = ctx.get_image_data(x, y, 1.0, 1.0).unwrap();

    data.data().0
}

// ---------- save / restore ----------

#[wasm_bindgen_test]
fn save_restore_restores_identity_transform() {
    let (_c, ctx) = make_canvas(100, 100);
    let before = read_transform(&ctx);
    ctx.save();
    ctx.translate(10.0, 20.0).expect("translate");
    ctx.scale(2.0, 2.0).expect("scale");
    ctx.rotate(std::f64::consts::FRAC_PI_4).expect("rotate");
    ctx.restore();
    let after = read_transform(&ctx);
    assert!((before.0 - after.0).abs() < 1e-9, "a: {before:?} vs {after:?}");
    assert!((before.4 - after.4).abs() < 1e-9, "e: {before:?} vs {after:?}");
    assert!((before.5 - after.5).abs() < 1e-9, "f: {before:?} vs {after:?}");
}

#[wasm_bindgen_test]
fn without_restore_transform_stays() {
    let (_c, ctx) = make_canvas(100, 100);
    ctx.translate(30.0, 5.0).expect("translate");
    let t = read_transform(&ctx);
    assert!((t.4 - 30.0).abs() < 1e-9, "e should be 30, got {}", t.4);
    assert!((t.5 - 5.0).abs() < 1e-9, "f should be 5, got {}", t.5);
}

#[wasm_bindgen_test]
fn transform_demo_with_save_puts_third_square_at_origin() {
    let (_c, ctx) = make_canvas(300, 200);
    draw_transform_demo(&ctx, 300.0, 200.0, true).unwrap();

    // С restore третий квадрат — в (20, 20). Пиксель в (50, 50) оранжевый.
    let px = read_pixel(&ctx, 50.0, 50.0);
    assert!(px[0] > 200, "expected orange, got rgb({},{},{})", px[0], px[1], px[2]);
    assert!(px[1] > 100, "orange has significant green");
}

#[wasm_bindgen_test]
fn transform_demo_without_save_puts_third_square_at_shift() {
    let (_c, ctx) = make_canvas(300, 200);
    draw_transform_demo(&ctx, 300.0, 200.0, false).unwrap();

    // Без restore третий квадрат — в (120, 60). Пиксель в (150, 90) оранжевый.
    let px = read_pixel(&ctx, 150.0, 90.0);
    assert!(px[0] > 200, "expected orange at shifted position, got rgb({},{},{})",
        px[0], px[1], px[2]);
}

// ---------- Композиция ----------

#[wasm_bindgen_test]
fn draw_layers_completes_for_all_modes() {
    let (_c, ctx) = make_canvas(400, 220);
    for mode in [
        "source-over", "multiply", "screen", "overlay", "difference", "lighter",
    ] {
        draw_layers(&ctx, 400.0, 220.0, 0.8, mode, false).expect(mode);
    }
}

#[wasm_bindgen_test]
fn multiply_darkens_intersection() {
    let (_c, ctx) = make_canvas(200, 200);

    // Красный прямоугольник.
    ctx.set_global_composite_operation("source-over").unwrap();
    ctx.set_global_alpha(1.0);
    ctx.set_fill_style_str("#ff0000");
    ctx.fill_rect(0.0, 0.0, 100.0, 100.0);

    // Зелёный прямоугольник со смещением, режим multiply.
    ctx.set_global_composite_operation("multiply").unwrap();
    ctx.set_fill_style_str("#00ff00");
    ctx.fill_rect(50.0, 50.0, 100.0, 100.0);

    // Пересечение (75, 75): multiply даёт (0, 0, 0) — black.
    let cross = read_pixel(&ctx, 75.0, 75.0);
    assert!(cross[0] < 40, "red channel should be near 0, got {}", cross[0]);
    assert!(cross[1] < 40, "green channel should be near 0, got {}", cross[1]);
    assert!(cross[2] < 40, "blue channel should be near 0, got {}", cross[2]);

    // Красная зона вне пересечения (25, 25): (255, 0, 0).
    let red_only = read_pixel(&ctx, 25.0, 25.0);
    assert!(red_only[0] > 200, "red zone should stay red");
    assert!(red_only[1] < 40, "red zone should have no green");
}

#[wasm_bindgen_test]
fn global_alpha_affects_result_color() {
    let (_c, ctx) = make_canvas(50, 50);

    // Красный фон.
    ctx.set_global_composite_operation("source-over").unwrap();
    ctx.set_global_alpha(1.0);
    ctx.set_fill_style_str("#ff0000");
    ctx.fill_rect(0.0, 0.0, 50.0, 50.0);

    // Зелёный с alpha = 0.5.
    ctx.set_global_alpha(0.5);
    ctx.set_fill_style_str("#00ff00");
    ctx.fill_rect(0.0, 0.0, 50.0, 50.0);

    // Результат: смесь 50/50 — примерно (127, 127, 0).
    let px = read_pixel(&ctx, 25.0, 25.0);
    assert!(px[0] > 100 && px[0] < 160, "red should be around 127, got {}", px[0]);
    assert!(px[1] > 100 && px[1] < 160, "green should be around 127, got {}", px[1]);
}

// ---------- Clip ----------

#[wasm_bindgen_test]
fn clip_limits_drawing_area() {
    let (_c, ctx) = make_canvas(200, 200);
    draw_clip_demo(&ctx, 200.0, 200.0).unwrap();

    // Центр — внутри круга, белый.
    let inside = read_pixel(&ctx, 100.0, 100.0);
    assert!(inside[0] > 200, "inside clip should be white, got {}", inside[0]);

    // Угол — вне круга, остался чёрным.
    let outside = read_pixel(&ctx, 5.0, 5.0);
    assert!(outside[0] < 40, "outside clip should be black, got {}", outside[0]);
}

#[wasm_bindgen_test]
fn clip_does_not_persist_after_restore() {
    let (_c, ctx) = make_canvas(200, 200);
    draw_clip_demo(&ctx, 200.0, 200.0).unwrap();

    // После restore клип снят — можно рисовать в углу.
    ctx.set_fill_style_str("#ffff00");
    ctx.fill_rect(0.0, 0.0, 20.0, 20.0);

    let px = read_pixel(&ctx, 5.0, 5.0);
    assert!(px[0] > 200 && px[1] > 200, "yellow after restore, got rgb({},{},{})",
        px[0], px[1], px[2]);
}