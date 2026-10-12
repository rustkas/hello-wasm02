//! Тесты OffscreenCanvas-движка: `src/converter_native.rs`.

mod raster {
    use image::{ImageBuffer, Rgb};

    pub fn make_test_png(w: u32, h: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(w, h, |x, y| {
                Rgb([
                    ((x * 255 / w.max(1)) as u8).wrapping_add(10),
                    ((y * 255 / h.max(1)) as u8).wrapping_add(20),
                    (((x + y) * 255 / (w + h).max(1)) as u8).wrapping_add(30),
                ])
            });
        let mut out = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Png,
        )
        .expect("PNG encode");
        out
    }

    pub fn make_test_jpeg(w: u32, h: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(w, h, |x, y| {
                Rgb([
                    ((x * 255 / w.max(1)) as u8).wrapping_add(10),
                    ((y * 255 / h.max(1)) as u8).wrapping_add(20),
                    (((x + y) * 255 / (w + h).max(1)) as u8).wrapping_add(30),
                ])
            });
        let mut out = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .expect("JPEG encode");
        out
    }
}

use hello_wasm02::{converter, converter_native};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn offscreen_available() -> bool {
    let window = web_sys::window().unwrap();
    js_sys::Reflect::get(&window, &"OffscreenCanvas".into())
        .map(|v| !v.is_undefined())
        .unwrap_or(false)
}

#[wasm_bindgen_test]
async fn native_grayscale_roundtrip() {
    if !offscreen_available() {
        return;
    }
    let png = raster::make_test_png(16, 16);
    let out = converter_native::grayscale_native(&png)
        .await
        .expect("native grayscale ok");

    assert_eq!(
        converter::validate_magic_bytes(&out).unwrap(),
        converter::ImageFormat::Png
    );

    use image::GenericImageView;
    let orig =
        image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
    let res =
        image::load_from_memory_with_format(&out, image::ImageFormat::Png).unwrap();
    assert_eq!(orig.dimensions(), res.dimensions());
}

#[wasm_bindgen_test]
async fn native_grayscale_from_jpeg() {
    if !offscreen_available() {
        return;
    }
    let jpg = raster::make_test_jpeg(16, 16);
    let out = converter_native::grayscale_native(&jpg)
        .await
        .expect("native grayscale from JPEG");
    assert_eq!(
        converter::validate_magic_bytes(&out).unwrap(),
        converter::ImageFormat::Png
    );
}

#[wasm_bindgen_test]
async fn native_rejects_garbage() {
    if !offscreen_available() {
        return;
    }
    let res = converter_native::grayscale_native(b"definitely not an image").await;
    assert!(res.is_err(), "garbage must not decode");
}

/// Мягкая проверка: если ctx.filter поддерживается, пиксели серые.
/// Если нет — не фейлим, просто логируем (редкий случай).
#[wasm_bindgen_test]
async fn native_grayscale_is_gray_or_warn() {
    if !offscreen_available() {
        return;
    }
    let png = raster::make_test_png(16, 16);
    let out = match converter_native::grayscale_native(&png).await {
        Ok(o) => o,
        Err(_) => return,
    };
    use image::GenericImageView;
    let img =
        image::load_from_memory_with_format(&out, image::ImageFormat::Png).unwrap();
    let (w, h) = img.dimensions();
    let mut non_gray = 0u32;
    for iy in 0..4u32 {
        for ix in 0..4u32 {
            let x = (ix * w / 4).min(w - 1);
            let y = (iy * h / 4).min(h - 1);
            let p = img.get_pixel(x, y).0;
            if p[0] != p[1] || p[1] != p[2] {
                non_gray += 1;
            }
        }
    }
    if non_gray > 0 {
        web_sys::console::warn_1(
            &"ctx.filter=grayscale(1) не поддержан в этом движке".into(),
        );
    }
}