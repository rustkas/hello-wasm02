use js_sys::{Function, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, OffscreenCanvas};

/// 5.3: grayscale через OffscreenCanvas.
/// Все методы, сигнатура которых отличается между минорными версиями web-sys
/// (createImageBitmap, getContext, convertToBlob, arrayBuffer) вызываются
/// через Reflect — это гарантирует независимость от версии.
pub async fn grayscale_native(png_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    let window = web_sys::window().unwrap();

    // Blob из исходных байт PNG.
    let parts = js_sys::Array::new();
    parts.push(&Uint8Array::from(png_bytes));
    let blob: Blob = Blob::new_with_u8_array_sequence(&parts)?;

    // --- createImageBitmap(blob) ---
    let create_fn = Reflect::get(&window, &JsValue::from_str("createImageBitmap"))?
        .dyn_into::<Function>()?;
    let bitmap_promise_js: JsValue = create_fn.call1(&window, &blob)?;
    let bitmap_promise: js_sys::Promise = bitmap_promise_js.dyn_into()?;
    let bitmap_js: JsValue = JsFuture::from(bitmap_promise).await?;

    let w = Reflect::get(&bitmap_js, &JsValue::from_str("width"))?
        .as_f64()
        .unwrap_or(0.0);
    let h = Reflect::get(&bitmap_js, &JsValue::from_str("height"))?
        .as_f64()
        .unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return Err(JsValue::from_str("createImageBitmap: bad size"));
    }

    // --- OffscreenCanvas(w, h) ---
    let canvas = OffscreenCanvas::new(w as u32, h as u32)?;

    // --- canvas.getContext("2d") ---
    let get_ctx = Reflect::get(&canvas, &JsValue::from_str("getContext"))?
        .dyn_into::<Function>()?;
    let ctx: JsValue = get_ctx.call1(&canvas, &JsValue::from_str("2d"))?;
    if ctx.is_null() || ctx.is_undefined() {
        return Err(JsValue::from_str("OffscreenCanvas: no 2d context"));
    }

    // ctx.filter = "grayscale(1)" — свойство может отсутствовать,
    // ошибку установки молча игнорируем.
    let _ = Reflect::set(
        &ctx,
        &JsValue::from_str("filter"),
        &JsValue::from_str("grayscale(1)"),
    );

    // --- ctx.drawImage(bitmap, 0, 0) ---
    let draw_fn = Reflect::get(&ctx, &JsValue::from_str("drawImage"))?
        .dyn_into::<Function>()?;
    draw_fn.call3(
        &ctx,
        &bitmap_js,
        &JsValue::from_f64(0.0),
        &JsValue::from_f64(0.0),
    )?;

    // --- canvas.convertToBlob() ---
    let convert_fn = Reflect::get(&canvas, &JsValue::from_str("convertToBlob"))?
        .dyn_into::<Function>()?;
    let blob_promise_js: JsValue = convert_fn.call0(&canvas)?;
    let blob_promise: js_sys::Promise = blob_promise_js.dyn_into()?;
    let result_blob_js: JsValue = JsFuture::from(blob_promise).await?;
    let result_blob: Blob = result_blob_js.dyn_into()?;

    // --- result_blob.arrayBuffer() ---
    // В разных версиях web-sys метод возвращает либо Promise, либо
    // Result<Promise, JsValue>. Reflect-вызов работает в обоих случаях.
    let array_buffer_fn = Reflect::get(&result_blob, &JsValue::from_str("arrayBuffer"))?
        .dyn_into::<Function>()?;
    let buf_promise_js: JsValue = array_buffer_fn.call0(&result_blob)?;
    let buf_promise: js_sys::Promise = buf_promise_js.dyn_into()?;
    let buf_js: JsValue = JsFuture::from(buf_promise).await?;
    let u8arr = Uint8Array::new(&buf_js);
    Ok(u8arr.to_vec())
}