#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement};

use hello_wasm02::canvas_drawing::{draw_scene, measure, Brush};

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

/// Читает RGBA в точке `(x, y)`.
///
/// `getImageData` в web-sys 0.3.105 имеет две сигнатуры, переключаемые
/// cfg-флагом `web_sys_unstable_apis`:
/// - stable: `f64, f64, f64, f64`;
/// - unstable: `i32, i32, i32, i32`.
///
/// Хелпер скрывает это различие. Внешний интерфейс — `f64`-координаты,
/// как их обычно и передают. Остальные тесты не думают про cfg.
fn read_pixel(ctx: &CanvasRenderingContext2d, x: f64, y: f64) -> Vec<u8> {
    #[cfg(web_sys_unstable_apis)]
    let data = ctx.get_image_data(x as i32, y as i32, 1, 1).unwrap();

    #[cfg(not(web_sys_unstable_apis))]
    let data = ctx.get_image_data(x, y, 1.0, 1.0).unwrap();

    // `ImageData::data()` в 0.3.105 возвращает `Clamped<Vec<u8>>` — берём `.0`.
    data.data().0
}

#[wasm_bindgen_test]
fn draw_scene_completes_without_panic() {
    let (_c, ctx) = make_canvas(400, 240);
    draw_scene(&ctx, 400.0, 240.0, Brush::default()).expect("draw");
}

#[wasm_bindgen_test]
fn filled_rect_has_expected_color() {
    let (_c, ctx) = make_canvas(50, 50);
    ctx.set_fill_style_str("#ff0000");
    ctx.fill_rect(10.0, 10.0, 30.0, 30.0);

    let px = read_pixel(&ctx, 20.0, 20.0);
    assert_eq!(px[0], 255, "red");
    assert_eq!(px[1], 0, "green");
    assert_eq!(px[2], 0, "blue");
}

#[wasm_bindgen_test]
fn stroke_thickness_affects_pixels() {
    // Тонкая линия vs толстая — толстая должна закрасить больше.
    let (_c1, ctx1) = make_canvas(100, 100);
    ctx1.set_stroke_style_str("#ffffff");
    ctx1.set_line_width(1.0);
    ctx1.begin_path();
    ctx1.move_to(10.0, 50.0);
    ctx1.line_to(90.0, 50.0);
    ctx1.stroke();

    let (_c2, ctx2) = make_canvas(100, 100);
    ctx2.set_stroke_style_str("#ffffff");
    ctx2.set_line_width(10.0);
    ctx2.begin_path();
    ctx2.move_to(10.0, 50.0);
    ctx2.line_to(90.0, 50.0);
    ctx2.stroke();

    // Читаем пиксель на расстоянии 4 px от центра линии.
    // Тонкая линия его не задела, толстая — задела.
    let thin = read_pixel(&ctx1, 50.0, 54.0);
    let thick = read_pixel(&ctx2, 50.0, 54.0);
    assert!(thin[0] < 40, "thin stroke should not reach far");
    assert!(thick[0] > 200, "thick stroke should cover y+4");
}

#[wasm_bindgen_test]
fn measure_text_grows_with_font_size() {
    let (_c, ctx) = make_canvas(200, 50);
    ctx.set_font("10px monospace");
    let w_small = measure(&ctx, "Sample").unwrap().0;
    ctx.set_font("30px monospace");
    let w_large = measure(&ctx, "Sample").unwrap().0;
    assert!(w_large > w_small, "large {w_large} should beat small {w_small}");
    assert!(w_small > 0.0, "width should be positive");
}

#[wasm_bindgen_test]
fn gradient_has_distinct_colors_at_ends() {
    let (_c, ctx) = make_canvas(100, 50);
    let grad = ctx.create_linear_gradient(0.0, 0.0, 100.0, 0.0);
    grad.add_color_stop(0.0, "#000000").unwrap();
    grad.add_color_stop(1.0, "#ffffff").unwrap();
    ctx.set_fill_style_canvas_gradient(&grad);
    ctx.fill_rect(0.0, 0.0, 100.0, 50.0);

    let left = read_pixel(&ctx, 2.0, 25.0);
    let right = read_pixel(&ctx, 97.0, 25.0);
    assert!(left[0] < 40, "left should be dark, got {}", left[0]);
    assert!(right[0] > 215, "right should be bright, got {}", right[0]);
}

#[wasm_bindgen_test]
fn begin_path_resets_previous_segments() {
    // Ключевая проверка: без beginPath второй stroke() обвёл бы
    // и первую линию тоже. С beginPath — только вторую.
    let (_c, ctx) = make_canvas(100, 100);

    // Первая линия — по диагонали (20,20)→(80,80).
    ctx.set_stroke_style_str("#ff0000");
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.move_to(20.0, 20.0);
    ctx.line_to(80.0, 80.0);
    ctx.stroke();

    // Вторая — горизонтальная на y=10. Начинаем с beginPath.
    ctx.set_stroke_style_str("#00ff00");
    ctx.begin_path();
    ctx.move_to(20.0, 10.0);
    ctx.line_to(80.0, 10.0);
    ctx.stroke();

    // Пиксель на середине диагонали должен остаться красным —
    // вторая линия прошла по y=10 и не пересеклась с ним.
    let diag = read_pixel(&ctx, 50.0, 50.0);
    assert!(diag[0] > 200, "diagonal should still be red");
    assert!(diag[1] < 40, "diagonal should not be green");
}