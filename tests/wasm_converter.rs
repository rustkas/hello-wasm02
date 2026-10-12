//! Тесты чистой логики: `src/converter.rs`.
//! Самодостаточны, не используют `tests/common`.

mod raster {
    //! Локальные хелперы (не путать с `tests/common`, который про DOM).

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

    pub fn verify_grayscale(png_bytes: &[u8]) -> bool {
        use image::GenericImageView;
        let img = match image::load_from_memory_with_format(
            png_bytes,
            image::ImageFormat::Png,
        ) {
            Ok(i) => i,
            Err(_) => return false,
        };
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return true;
        }
        for iy in 0..4u32 {
            for ix in 0..4u32 {
                let x = (ix * w / 4).min(w - 1);
                let y = (iy * h / 4).min(h - 1);
                let p = img.get_pixel(x, y).0;
                if p[0] != p[1] || p[1] != p[2] {
                    return false;
                }
            }
        }
        true
    }

    pub fn file_from_bytes(bytes: &[u8], name: &str) -> web_sys::File {
        let u8arr = js_sys::Uint8Array::from(bytes);
        let blob = web_sys::Blob::new_with_u8_array_sequence(
            &js_sys::Array::of1(&u8arr),
        )
        .expect("Blob::new");
        web_sys::File::new_with_blob_sequence(
            &js_sys::Array::of1(&blob),
            name,
        )
        .expect("File::new")
    }
}

use hello_wasm02::converter::{self, ReadOutcome};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// validate_magic_bytes
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
fn validate_png_jpeg_and_garbage() {
    let png = raster::make_test_png(2, 2);
    assert_eq!(
        converter::validate_magic_bytes(&png).unwrap(),
        converter::ImageFormat::Png
    );

    let jpg = raster::make_test_jpeg(2, 2);
    assert_eq!(
        converter::validate_magic_bytes(&jpg).unwrap(),
        converter::ImageFormat::Jpeg
    );

    assert!(converter::validate_magic_bytes(b"plain text").is_err());
    assert!(converter::validate_magic_bytes(&[]).is_err());
    assert!(converter::validate_magic_bytes(&[0x89, 0x50]).is_err());
}

// ---------------------------------------------------------------------------
// adaptive_chunk
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
fn adaptive_chunk_bounds() {
    // Нижняя граница clamp.
    assert_eq!(converter::adaptive_chunk(0, 200), 64 * 1024);
    assert_eq!(converter::adaptive_chunk(1, 200), 64 * 1024);

    // Верхняя граница clamp.
    let huge: usize = (u32::MAX - 1) as usize;
    assert_eq!(
        converter::adaptive_chunk(huge, 200),
        4 * 1024 * 1024
    );

    // Средний размер: 200 МБ / 200 = 1 МБ — в границах.
    let mid = converter::adaptive_chunk(200 * 1024 * 1024, 200);
    assert!(mid >= 64 * 1024 && mid <= 4 * 1024 * 1024);
}

// ---------------------------------------------------------------------------
// SHA-256: sync == async, прогресс монотонен
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
async fn sha256_sync_and_async_agree() {
    let data: Vec<u8> = (0..1024 * 1024).map(|i| (i & 0xFF) as u8).collect();
    let sync = converter::sha256_hex(&data);
    let stats = converter::sha256_hex_with_progress(&data, |_| {}).await;
    assert_eq!(sync.len(), 64);
    assert_eq!(stats.hex.len(), 64);
    assert_eq!(sync, stats.hex);
}

#[wasm_bindgen_test]
async fn sha256_progress_monotonic() {
    let data = vec![0xABu8; 4 * 1024 * 1024];
    let seen: std::rc::Rc<std::cell::RefCell<Vec<f64>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen_c = seen.clone();
    let stats = converter::sha256_hex_with_progress(&data, move |f| {
        seen_c.borrow_mut().push(f);
    })
    .await;

    let v = seen.borrow();
    for w in v.windows(2) {
        assert!(w[1] + 1e-9 >= w[0], "progress must be monotonic");
    }
    assert!((v.last().copied().unwrap_or(0.0) - 1.0).abs() < 1e-6);

    assert!(stats.chunk_size >= 64 * 1024 && stats.chunk_size <= 4 * 1024 * 1024);
    let expected_chunks =
        ((data.len() + stats.chunk_size - 1) / stats.chunk_size) as u32;
    assert_eq!(stats.chunks, expected_chunks);
}

// ---------------------------------------------------------------------------
// decode_to_rgba / encode_rgba_png
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
fn decode_encode_png_roundtrip() {
    let png = raster::make_test_png(8, 8);
    let (rgba, w, h) =
        converter::decode_to_rgba(&png, converter::ImageFormat::Png).unwrap();
    assert_eq!((w, h), (8, 8));
    assert_eq!(rgba.len(), (8 * 8 * 4) as usize);

    let back = converter::encode_rgba_png(&rgba, w, h).unwrap();
    assert_eq!(
        converter::validate_magic_bytes(&back).unwrap(),
        converter::ImageFormat::Png
    );
    use image::GenericImageView;
    let img = image::load_from_memory_with_format(&back, image::ImageFormat::Png).unwrap();
    assert_eq!(img.dimensions(), (8, 8));
}

// ---------------------------------------------------------------------------
// to_grayscale_png
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
fn grayscale_roundtrip_verify() {
    let png = raster::make_test_png(8, 8);
    let fmt = converter::validate_magic_bytes(&png).unwrap();
    let gray = converter::to_grayscale_png(&png, fmt).unwrap();
    assert_eq!(
        converter::validate_magic_bytes(&gray).unwrap(),
        converter::ImageFormat::Png
    );
    assert!(raster::verify_grayscale(&gray));
}

#[wasm_bindgen_test]
fn grayscale_from_jpeg() {
    let jpg = raster::make_test_jpeg(8, 8);
    let fmt = converter::validate_magic_bytes(&jpg).unwrap();
    let gray = converter::to_grayscale_png(&jpg, fmt).unwrap();
    assert!(raster::verify_grayscale(&gray));
}

// ---------------------------------------------------------------------------
// generate_upload_id
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
fn upload_id_is_url_safe_and_not_empty() {
    let id = converter::generate_upload_id();
    assert!(!id.is_empty());
    assert!(id.len() >= 8);
    for ch in id.chars() {
        assert!(
            ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
            "unexpected char in upload_id: {:?}",
            ch
        );
    }
}

// ---------------------------------------------------------------------------
// read_blob_stream_validated
// ---------------------------------------------------------------------------
#[wasm_bindgen_test]
async fn read_ok_png() {
    let bytes = raster::make_test_png(4, 4);
    let file = raster::file_from_bytes(&bytes, "a.png");

    let hits = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let hits_c = hits.clone();
    let outcome = converter::read_blob_stream_validated(
        &file,
        move |_| hits_c.set(hits_c.get() + 1),
        || false,
    )
    .await
    .unwrap();

    match outcome {
        ReadOutcome::Ok(d) => assert_eq!(d, bytes),
        _ => panic!("expected Ok"),
    }
    assert!(hits.get() >= 1);
}

#[wasm_bindgen_test]
async fn read_cancel_immediately() {
    let bytes = raster::make_test_png(8, 8);
    let file = raster::file_from_bytes(&bytes, "a.png");
    let outcome =
        converter::read_blob_stream_validated(&file, |_| {}, || true)
            .await
            .unwrap();
    assert!(matches!(outcome, ReadOutcome::Cancelled));
}

#[wasm_bindgen_test]
async fn read_early_exit_on_garbage() {
    let file = raster::file_from_bytes(b"not a picture at all", "a.txt");
    let outcome =
        converter::read_blob_stream_validated(&file, |_| {}, || false)
            .await
            .unwrap();
    match outcome {
        ReadOutcome::InvalidFormat(msg) => assert!(!msg.is_empty()),
        _ => panic!("expected InvalidFormat"),
    }
}