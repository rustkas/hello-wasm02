#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use web_sys::{Blob, BlobPropertyBag};

use image::{ImageFormat, Rgba, RgbaImage};

use hello_wasm02::canvas_image_loading::{
    load_as_bitmap, load_as_image, BitmapOptions,
};

wasm_bindgen_test_configure!(run_in_browser);

/// Генерирует валидный PNG 4×4 в рантайме.
///
/// Ручные 1×1 PNG (с минимальным IDAT) могут отвергаться
/// `createImageBitmap` на некоторых версиях Chrome из-за edge case
/// декодера. `image` crate гарантирует корректный файл, а 4×4 — вне
/// зоны граничных случаев.
fn tiny_png_blob() -> Blob {
    let mut img = RgbaImage::new(4, 4);
    for px in img.pixels_mut() {
        *px = Rgba([255, 0, 0, 255]); // непрозрачный красный
    }

    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
    img.write_to(&mut cursor, ImageFormat::Png)
        .expect("PNG encode");
    let png_bytes = cursor.into_inner();

    let u8arr = js_sys::Uint8Array::from(png_bytes.as_slice());
    let parts = js_sys::Array::new();
    parts.push(u8arr.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type("image/png");
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag).unwrap()
}

#[wasm_bindgen_test]
async fn img_decode_resolves_and_has_size() {
    let blob = tiny_png_blob();
    let img = load_as_image(&blob).await.expect("decode");
    assert_eq!(img.natural_width(), 4);
    assert_eq!(img.natural_height(), 4);
}

#[wasm_bindgen_test]
async fn bitmap_loads_with_correct_size() {
    let blob = tiny_png_blob();
    let options = BitmapOptions {
        apply_exif: false,
        max_dim: None,
        quality: "high",
    };
    let bitmap = load_as_bitmap(&blob, options).await.expect("bitmap");
    assert_eq!(bitmap.width(), 4);
    assert_eq!(bitmap.height(), 4);
    bitmap.close();
}

#[wasm_bindgen_test]
async fn bitmap_with_resize() {
    // 4×4 → resizeWidth: 16 даёт 16×16 (upscale сохраняет aspect ratio).
    let blob = tiny_png_blob();
    let options = BitmapOptions {
        apply_exif: false,
        max_dim: Some(16),
        quality: "medium",
    };
    let bitmap = load_as_bitmap(&blob, options).await.expect("resize");
    assert_eq!(bitmap.width(), 16);
    assert_eq!(bitmap.height(), 16);
    bitmap.close();
}

#[wasm_bindgen_test]
async fn bitmap_with_exif_option_does_not_panic() {
    // `imageOrientation: "from-image"` на PNG без EXIF-метаданных
    // должен быть просто проигнорирован — не падать.
    let blob = tiny_png_blob();
    let options = BitmapOptions {
        apply_exif: true,
        max_dim: None,
        quality: "high",
    };
    let bitmap = load_as_bitmap(&blob, options).await.expect("exif");
    assert_eq!(bitmap.width(), 4);
    assert_eq!(bitmap.height(), 4);
    bitmap.close();
}

#[wasm_bindgen_test]
async fn bitmap_close_is_safe() {
    let blob = tiny_png_blob();
    let bitmap = load_as_bitmap(&blob, BitmapOptions::default()).await.unwrap();
    // Идемпотентность: повторный close() безопасен.
    bitmap.close();
    bitmap.close();
}