//! Тесты Web Worker-движка: `src/converter_worker.rs`.

mod raster {
    pub fn make_rgba(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                out.push((x * 255 / w.max(1)) as u8);
                out.push((y * 255 / h.max(1)) as u8);
                out.push(((x + y) * 255 / (w + h).max(1)) as u8);
                out.push(255);
            }
        }
        out
    }

    pub fn assert_all_gray(buf: &[u8]) {
        assert_eq!(buf.len() % 4, 0, "RGBA length must be multiple of 4");
        for px in buf.chunks_exact(4) {
            assert_eq!(px[0], px[1], "R != G at some pixel");
            assert_eq!(px[1], px[2], "G != B at some pixel");
        }
    }

    pub fn alpha_preserved(orig: &[u8], out: &[u8]) {
        for (a, b) in orig.chunks_exact(4).zip(out.chunks_exact(4)) {
            assert_eq!(a[3], b[3]);
        }
    }
}

use hello_wasm02::converter_worker;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn worker_grayscale_basic() {
    let rgba = raster::make_rgba(16, 16);
    let out = converter_worker::grayscale_in_worker(rgba.clone())
        .await
        .expect("worker ok");
    assert_eq!(out.len(), rgba.len());
    raster::assert_all_gray(&out);
    raster::alpha_preserved(&rgba, &out);
}

#[wasm_bindgen_test]
async fn parallel_matches_single_worker() {
    let rgba = raster::make_rgba(32, 32);
    let single = converter_worker::grayscale_in_worker(rgba.clone())
        .await
        .unwrap();
    let par = converter_worker::grayscale_parallel(rgba.clone())
        .await
        .unwrap();
    assert_eq!(single.len(), par.len());
    assert_eq!(single, par, "parallel result must match single-worker result");
}

#[wasm_bindgen_test]
async fn parallel_handles_zero_and_tiny() {
    let empty = converter_worker::grayscale_parallel(Vec::new())
        .await
        .unwrap();
    assert_eq!(empty.len(), 0);

    let one_px = vec![120u8, 40, 200, 255];
    let out = converter_worker::grayscale_parallel(one_px)
        .await
        .unwrap();
    assert_eq!(out.len(), 4);
    raster::assert_all_gray(&out);
    assert_eq!(out[3], 255);
}

#[wasm_bindgen_test]
async fn parallel_odd_length() {
    let rgba: Vec<u8> = (0..12).map(|i| (i * 20) as u8).collect();
    let out = converter_worker::grayscale_parallel(rgba)
        .await
        .unwrap();
    assert_eq!(out.len(), 12);
    raster::assert_all_gray(&out);
}