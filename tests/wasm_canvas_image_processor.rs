#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Blob, BlobPropertyBag, HtmlCanvasElement, ImageData};

use hello_wasm02::canvas_image_processor::{
    draw_image_data_to_canvas, image_data_to_vec, load_image_from_blob,
    vec_to_image_data,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_tiny_png_blob() -> Blob {
    // 1×1 красный PNG. Данные фиксированные — не требуют сети.
    // 67 байт стандартного PNG; если картинка загрузится — decode() пройдёт.
    const PNG_1X1_RED: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41,
        0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
        0x00, 0x00, 0x03, 0x00, 0x01, 0x9A, 0x4C, 0x8C,
        0x4A, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let u8arr = js_sys::Uint8Array::from(PNG_1X1_RED);
    let parts = js_sys::Array::new();
    parts.push(u8arr.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type("image/png");
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag).unwrap()
}

#[wasm_bindgen_test]
async fn load_blob_as_image_and_decode() {
    let blob = make_tiny_png_blob();
    let img = load_image_from_blob(&blob).await.expect("decode");
    assert_eq!(img.natural_width(), 1);
    assert_eq!(img.natural_height(), 1);
}

#[wasm_bindgen_test]
fn vec_roundtrip_via_image_data() {
    let w = 3u32;
    let h = 2u32;
    let original: Vec<u8> = (0..w * h * 4).map(|i| (i & 0xFF) as u8).collect();
    let data = vec_to_image_data(&original, w, h).expect("to imagedata");
    assert_eq!(data.width(), w);
    assert_eq!(data.height(), h);
    let back = image_data_to_vec(&data);
    assert_eq!(back, original);
}

#[wasm_bindgen_test]
fn vec_to_image_data_rejects_wrong_size() {
    // Передаём буфер неправильного размера — ждём Err, не панику.
    let r = vec_to_image_data(&[0u8; 5], 2, 2);
    assert!(r.is_err());
}

#[wasm_bindgen_test]
fn draw_image_data_resizes_canvas() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: HtmlCanvasElement = document
        .create_element("canvas").unwrap().dyn_into().unwrap();
    let pixels = vec![255u8; 4 * 4 * 4];
    let data = vec_to_image_data(&pixels, 4, 4).unwrap();
    draw_image_data_to_canvas(&canvas, &data).unwrap();
    assert_eq!(canvas.width(), 4);
    assert_eq!(canvas.height(), 4);
}

#[wasm_bindgen_test]
fn empty_imagedata_roundtrip() {
    // 1×1 пиксель с нулевой альфой.
    let pixels = vec![0u8, 0, 0, 0];
    let data: ImageData = vec_to_image_data(&pixels, 1, 1).unwrap();
    let back = image_data_to_vec(&data);
    assert_eq!(back, pixels);
}