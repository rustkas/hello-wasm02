#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use hello_wasm02::pixel_ops::{
    average_rgb, blit_rgba_to_canvas, copy_frame_to_rgba,
    has_webgl2_support, invert_rgba, GpuInverter,
};
use hello_wasm02::video_frame::{invoke, num};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
fn webgl2_support_detected() {
    // В headless Chrome WebGL2 обычно доступен (software rendering
    // через SwiftShader). Проверяем, что функция не паникует.
    let _ = has_webgl2_support();
}

// =============================================================================
// GpuInverter: смоук-тесты
// =============================================================================

#[wasm_bindgen_test]
fn gpu_inverter_constructs() {
    if !has_webgl2_support() {
        return;
    }
    let inv = GpuInverter::new(64, 48);
    assert!(
        inv.is_ok(),
        "GpuInverter should construct: {:?}",
        inv.err(),
    );
}

#[wasm_bindgen_test]
fn gpu_inverter_round_trip() {
    if !has_webgl2_support() {
        return;
    }
    let inv = match GpuInverter::new(4, 1) {
        Ok(v) => v,
        Err(_) => return,
    };

    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(4);
    canvas.set_height(1);

    let ctx = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
    let data: Vec<u8> = vec![
        10, 20, 30, 255,
        200, 100, 50, 255,
        0, 0, 0, 255,
        255, 255, 255, 255,
    ];
    let clamped = wasm_bindgen::Clamped(&data[..]);
    let img = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, 4, 1)
        .unwrap();
    invoke(ctx.as_ref(), "putImageData", &[img.into(), num(0.0), num(0.0)])
        .unwrap();

    let frame = create_video_frame(&canvas);
    assert!(frame.is_ok(), "VideoFrame creation");

    let bitmap = inv.apply(&frame.unwrap());
    assert!(
        bitmap.is_ok(),
        "GPU apply should succeed: {:?}",
        bitmap.err(),
    );
}

// =============================================================================
// CPU: copyFrameToRgba + invert, точные значения (alpha = 255)
// =============================================================================

#[wasm_bindgen_test]
async fn cpu_round_trip_from_canvas() {
    // VideoFrame из canvas → copyTo → invert.
    //
    // Alpha = 255: при alpha < 255 canvas хранит пиксели как
    // premultiplied (R_premult = R × A / 255). `VideoFrame.copyTo`
    // возвращает пиксели как есть, в premultiplied виде, что
    // искажает точное сравнение RGB. С alpha = 255 premultiplied
    // совпадает с unpremultiplied, тест становится детерминированным.
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(2);
    canvas.set_height(1);

    let ctx = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
    let data: Vec<u8> = vec![
        50, 100, 150, 255,
        200, 50, 100, 255,
    ];
    let clamped = wasm_bindgen::Clamped(&data[..]);
    let img = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, 2, 1)
        .unwrap();
    invoke(ctx.as_ref(), "putImageData", &[img.into(), num(0.0), num(0.0)])
        .unwrap();

    let frame = create_video_frame(&canvas).expect("VideoFrame");
    let mut rgba = copy_frame_to_rgba(&frame, 2, 1).await.expect("copyTo");
    assert_eq!(rgba.len(), 8);

    invert_rgba(&mut rgba);
    // 50 → 205, 100 → 155, 150 → 105
    assert_eq!(rgba[0], 205);
    assert_eq!(rgba[1], 155);
    assert_eq!(rgba[2], 105);
    assert_eq!(rgba[3], 255);
    // 200 → 55, 50 → 205, 100 → 155
    assert_eq!(rgba[4], 55);
    assert_eq!(rgba[5], 205);
    assert_eq!(rgba[6], 155);
    assert_eq!(rgba[7], 255);
}

// =============================================================================
// CPU: alpha сохраняется после copyTo + invert
// =============================================================================

#[wasm_bindgen_test]
async fn cpu_round_trip_preserves_alpha() {
    // Проверяем отдельно: alpha сохраняется при copyTo + invert.
    // Точные RGB не сравниваем — premultiplied-конвертация может
    // сдвинуть их значения при alpha < 255.
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(1);
    canvas.set_height(1);

    let ctx = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
    let data: Vec<u8> = vec![100, 100, 100, 128];
    let clamped = wasm_bindgen::Clamped(&data[..]);
    let img = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, 1, 1)
        .unwrap();
    invoke(ctx.as_ref(), "putImageData", &[img.into(), num(0.0), num(0.0)])
        .unwrap();

    let frame = create_video_frame(&canvas).expect("VideoFrame");
    let mut rgba = copy_frame_to_rgba(&frame, 1, 1).await.expect("copyTo");
    let alpha_before = rgba[3];
    invert_rgba(&mut rgba);
    assert_eq!(
        rgba[3], alpha_before,
        "alpha должна сохраниться после invert",
    );
}

// =============================================================================
// CPU: blit обратно на canvas — round-trip через getImageData
// =============================================================================

#[wasm_bindgen_test]
fn blit_rgba_round_trip() {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(2);
    canvas.set_height(1);

    // Alpha = 255 для обоих пикселей — см. объяснение в
    // cpu_round_trip_from_canvas.
    let rgba = vec![10, 20, 30, 255, 40, 50, 60, 255];
    blit_rgba_to_canvas(&canvas, rgba.clone(), 2, 1).expect("blit");

    let ctx = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
    // getImageData: все четыре аргумента — i32.
    let readback = ctx.get_image_data(0, 0, 2, 1).expect("readback");
    let bytes = readback.data().0;
    assert_eq!(&bytes[..], &rgba[..]);
}

// =============================================================================
// Среднее после инверсии
// =============================================================================

#[wasm_bindgen_test]
fn average_after_invert_is_255_minus_original() {
    let mut bytes = vec![100, 50, 200, 255, 200, 150, 50, 128];
    let (r0, g0, b0) = average_rgb(&bytes);
    invert_rgba(&mut bytes);
    let (r1, g1, b1) = average_rgb(&bytes);
    assert!((r1 - (255.0 - r0)).abs() < 0.01);
    assert!((g1 - (255.0 - g0)).abs() < 0.01);
    assert!((b1 - (255.0 - b0)).abs() < 0.01);
}

// =============================================================================
// Хелпер: создать VideoFrame из canvas
// =============================================================================

fn create_video_frame(canvas: &web_sys::HtmlCanvasElement) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("VideoFrame"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("VideoFrame not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let init = js_sys::Object::new();
    js_sys::Reflect::set(
        &init,
        &JsValue::from_str("timestamp"),
        &JsValue::from_f64(0.0),
    )?;
    let args = js_sys::Array::new();
    args.push(&JsValue::from(canvas.clone()));
    args.push(&init);
    js_sys::Reflect::construct(&ctor, &args)
}