#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use web_sys::{Blob, BlobPropertyBag};

use image::{ImageFormat, Rgba, RgbaImage};

use hello_wasm02::canvas_image_pipeline::{
    derived_png_name, is_save_picker_supported, load_image_as_pixels,
    pixels_to_png_blob, Filter,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_png_blob(w: u32, h: u32, r: u8, g: u8, b: u8) -> Blob {
    let mut img = RgbaImage::new(w, h);
    for px in img.pixels_mut() {
        *px = Rgba([r, g, b, 255]);
    }
    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
    img.write_to(&mut cursor, ImageFormat::Png).expect("encode");
    let bytes = cursor.into_inner();

    let u8arr = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::new();
    parts.push(u8arr.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type("image/png");
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag).unwrap()
}

#[wasm_bindgen_test]
async fn load_image_returns_pixels_with_exif_path() {
    let blob = make_png_blob(8, 6, 255, 0, 0);
    let (pixels, w, h, resized) =
        load_image_as_pixels(&blob, 800).await.expect("load");
    assert_eq!((w, h), (8, 6));
    assert_eq!(pixels.len(), 8 * 6 * 4);
    assert!(!resized);
    // Первый пиксель — красный.
    assert_eq!(pixels[0], 255);
    assert_eq!(pixels[1], 0);
}

#[wasm_bindgen_test]
async fn load_image_respects_max_dim() {
    let blob = make_png_blob(100, 50, 0, 255, 0);
    let (_pixels, w, h, resized) =
        load_image_as_pixels(&blob, 20).await.expect("load");
    assert!(resized, "should be resized from 100×50");
    assert_eq!(w, 20);
    assert_eq!(h, 10);
}

#[wasm_bindgen_test]
async fn pixels_to_png_blob_produces_valid_png() {
    let pixels = vec![255u8; 4 * 4 * 4];
    let blob = pixels_to_png_blob(&pixels, 4, 4).await.expect("png");
    assert_eq!(blob.type_(), "image/png");
    assert!(blob.size() > 0.0);
}

#[wasm_bindgen_test]
async fn pixels_to_png_rejects_wrong_size() {
    let r = pixels_to_png_blob(&[0u8; 5], 2, 2).await;
    assert!(r.is_err());
}

#[wasm_bindgen_test]
fn derived_png_name_replaces_extension() {
    assert_eq!(derived_png_name("photo.jpg", "processed"), "photo.processed.png");
    assert_eq!(derived_png_name("scan", "out"), "scan.out.png");
    assert_eq!(derived_png_name("a.b.jpeg", "sobel"), "a.b.sobel.png");
}

#[wasm_bindgen_test]
fn filter_original_is_identity() {
    let px = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let out = Filter::Original.apply(&px, 2, 1);
    assert_eq!(out, px);
}

#[wasm_bindgen_test]
fn filter_grayscale_uses_rec709() {
    // Красный → яркость около 54.
    let px = vec![255, 0, 0, 255];
    let out = Filter::Grayscale.apply(&px, 1, 1);
    assert!(out[0] >= 53 && out[0] <= 55, "got {}", out[0]);
    assert_eq!(out[0], out[1]);
    assert_eq!(out[1], out[2]);
}

#[wasm_bindgen_test]
fn save_picker_support_is_reported() {
    // Не паникует, возвращает bool. В headless Chrome — обычно true.
    let _ = is_save_picker_supported();
}