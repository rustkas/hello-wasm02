#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::HtmlCanvasElement;

use hello_wasm02::offscreen_worker::{
    blob_size, create_worker, image_data_size, is_transfer_control_supported,
    is_worker_supported, post_command, transfer_control,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_canvas() -> HtmlCanvasElement {
    let doc = web_sys::window().unwrap().document().unwrap();
    doc.create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap()
}

#[wasm_bindgen_test]
fn worker_is_supported_in_browser() {
    // В headless Chrome Worker доступен.
    assert!(is_worker_supported(), "Worker should be available");
}

#[wasm_bindgen_test]
fn transfer_control_is_supported() {
    assert!(
        is_transfer_control_supported(),
        "transferControlToOffscreen should be available",
    );
}

#[wasm_bindgen_test]
fn transfer_control_yields_offscreen_canvas() {
    let canvas = make_canvas();
    canvas.set_width(100);
    canvas.set_height(100);
    let offscreen = transfer_control(&canvas).expect("transfer");
    assert_eq!(offscreen.width(), 100);
    assert_eq!(offscreen.height(), 100);
}

#[wasm_bindgen_test]
fn second_transfer_control_fails() {
    let canvas = make_canvas();
    canvas.set_width(50);
    canvas.set_height(50);
    let _ = transfer_control(&canvas).expect("first");
    // Повторная передача — InvalidStateError.
    let second = transfer_control(&canvas);
    assert!(second.is_err(), "second transfer should fail");
}

#[wasm_bindgen_test]
fn worker_created_from_dummy_url() {
    // Создаём Worker с несуществующим URL — сам объект создаётся,
    // ошибка будет при попытке загрузки скрипта. Тест проверяет, что
    // API вызова доступно без паники.
    let w = create_worker("/nonexistent_worker_xyz.js");
    assert!(w.is_ok(), "Worker object should be constructible");
}

#[wasm_bindgen_test]
fn post_command_does_not_panic() {
    let Ok(worker) = create_worker("/nonexistent_worker_xyz.js") else {
        return;
    };
    // postMessage на недоступный скрипт не паникует на этом шаге —
    // сообщение уйдёт в очередь, обработчика нет.
    let _ = post_command(&worker, "pause");
}

#[wasm_bindgen_test]
fn image_data_size_is_width_times_height_times_four() {
    // Небольшая проверка обвязки: ImageData размер считается как w*h*4.
    let w = 4u32;
    let h = 3u32;
    let pixels = vec![0u8; (w * h * 4) as usize];
    let clamped = wasm_bindgen::Clamped(pixels.as_slice());
    let data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h).unwrap();
    assert_eq!(image_data_size(&data), (w * h * 4) as usize);
}

#[wasm_bindgen_test]
fn blob_size_returns_size_in_bytes() {
    let parts = js_sys::Array::new();
    let u8arr = js_sys::Uint8Array::from(&[1u8, 2, 3, 4][..]);
    parts.push(u8arr.as_ref());
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type("application/octet-stream");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag).unwrap();
    assert_eq!(blob_size(&blob), 4.0);
}