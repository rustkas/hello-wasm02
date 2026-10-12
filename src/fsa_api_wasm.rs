//! Интеграция с WASM-библиотеками.
//!
//! **Ключевая идея.** WASM позволяет использовать Rust-крейты
//! (`image`, `csv`, `zip`, `sha2`) прямо в браузере. Байты файла
//! попадают в Rust без промежуточных копий, обрабатываются,
//! результат возвращается как `Vec<u8>` и записывается обратно.
//!
//! **Три движка одной задачи.** Grayscale-преобразование PNG:
//! * `grayscale_wasm` — Rust-крейт `image` в WASM, main thread.
//! * `grayscale_native` — Canvas 2D `filter = 'grayscale(1)'`,
//!   main thread. Браузер оптимизирует это нативно.
//! * `grayscale_worker` — Canvas 2D в Worker'е. Main thread
//!   не блокируется.
//!
//! **Один интерфейс, три движка.** Все три принимают `&[u8]` PNG,
//! возвращают `Vec<u8>` PNG. Вызывающий код не знает деталей.
//!
//! **Правило WASM-инференса.** `format!` в tail-позиции `if/else`/
//! `match`/блока иногда выводится как `()` → E0308. Все такие
//! места обёрнуты в `format_str(format_args!(…))`.
//!
//! **Правило snake_case.** `web-sys` использует snake_case:
//! `create_element`, `set_attribute`, `append_child`.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FileSystemFileHandle, ImageBitmap, MessageEvent, OffscreenCanvas, Worker};

use crate::fsa_api_file::get_file;

// ===========================================================================
// Строковые хелперы (правило WASM-инференса)
// ===========================================================================

#[inline]
fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

// ===========================================================================
// File → Vec<u8>
// ===========================================================================

/// Прочитать файл как байты через `File.arrayBuffer()`.
///
/// `arrayBuffer()` — самый быстрый способ получить байты из Blob:
/// один вызов, одна копия. `FileReader.readAsArrayBuffer` — это
/// уже устаревший путь, `arrayBuffer()` предпочтительнее.
pub async fn read_bytes(handle: &FileSystemFileHandle) -> Result<Vec<u8>, JsValue> {
    let file = get_file(handle).await?;
    let promise = file.array_buffer();
    let value = JsFuture::from(promise).await?;
    let ab: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("arrayBuffer вернул не ArrayBuffer"))?;
    let u8arr = js_sys::Uint8Array::new(&ab);
    let len = u8arr.length() as usize;
    let mut out: Vec<u8> = vec![0u8; len];
    u8arr.copy_to(&mut out);
    Ok(out)
}

// ===========================================================================
// Движок 1: WASM (Rust-крейт image)
// ===========================================================================

/// Grayscale через Rust-крейт `image` в WASM.
///
/// Принимает байты PNG, декодирует, конвертирует в luma (один
/// канал), кодирует обратно в PNG. Всё в одном вызове — никаких
/// промежуточных копий через JS.
pub fn grayscale_wasm(png_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    use std::io::Cursor;

    let img = image::load_from_memory(png_bytes).map_err(|e| {
        let msg = format_str(format_args!("image decode: {e}"));
        JsValue::from_str(&msg)
    })?;

    let gray = img.to_luma8();

    let mut out: Vec<u8> = Vec::new();
    {
        let mut cursor = Cursor::new(&mut out);
        gray.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| {
                let msg = format_str(format_args!("image encode: {e}"));
                JsValue::from_str(&msg)
            })?;
    }
    Ok(out)
}

// ===========================================================================
// Движок 2: Native Canvas (main thread)
// ===========================================================================

/// Grayscale через Canvas 2D `filter = 'grayscale(1)'`.
///
/// Браузер выполняет фильтр нативно — как правило, быстрее, чем
/// WASM, потому что это оптимизированный C++ движка.
pub async fn grayscale_native(png_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    let bitmap = bytes_to_bitmap(png_bytes).await?;

    let w: u32 = bitmap.width();
    let h: u32 = bitmap.height();
    let canvas = OffscreenCanvas::new(w, h)?;
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("нет 2d контекста"))?;
    let ctx: web_sys::OffscreenCanvasRenderingContext2d = ctx_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не OffscreenCanvasRenderingContext2d"))?;

    ctx.set_filter("grayscale(1)");
    ctx.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)?;

    canvas_to_bytes(&canvas).await
}

// ===========================================================================
// Движок 3: Worker (Canvas в отдельном потоке)
// ===========================================================================

/// JS-код inline-Worker'а для grayscale через OffscreenCanvas.
const WORKER_JS: &str = r#"
    self.onmessage = async (e) => {
        const data = e.data;
        try {
            const blob = new Blob([data], { type: 'image/png' });
            const bitmap = await createImageBitmap(blob);
            const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
            const ctx = canvas.getContext('2d');
            ctx.filter = 'grayscale(1)';
            ctx.drawImage(bitmap, 0, 0);
            const outBlob = await canvas.convertToBlob({ type: 'image/png' });
            const buf = await outBlob.arrayBuffer();
            self.postMessage({ ok: true, buf });
        } catch (err) {
            self.postMessage({ ok: false, error: String(err) });
        }
    };
"#;

/// Grayscale в Worker'е через OffscreenCanvas.
///
/// Тот же алгоритм, что и `grayscale_native`, но в отдельном
/// потоке — main thread не блокируется.
pub async fn grayscale_worker(png_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(WORKER_JS));
    let blob = Blob::new_with_str_sequence(&parts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let worker = Worker::new(&url)?;
    web_sys::Url::revoke_object_url(&url)?;

    let u8arr = js_sys::Uint8Array::from(png_bytes);
    worker.post_message(&u8arr.buffer())?;

    let worker_ref: &Worker = &worker;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let onmessage = Closure::once(move |e: MessageEvent| {
            let _ = resolve.call1(&JsValue::UNDEFINED, &e.data());
        });
        worker_ref.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    });
    let value: JsValue = JsFuture::from(promise).await?;
    worker.terminate();

    let ok = js_sys::Reflect::get(&value, &JsValue::from_str("ok"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !ok {
        let err = js_sys::Reflect::get(&value, &JsValue::from_str("error"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "unknown worker error".to_string());
        return Err(JsValue::from_str(&err));
    }
    let buf = js_sys::Reflect::get(&value, &JsValue::from_str("buf"))?;
    let ab: js_sys::ArrayBuffer = buf
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let u8_out = js_sys::Uint8Array::new(&ab);
    let len = u8_out.length() as usize;
    let mut out: Vec<u8> = vec![0u8; len];
    u8_out.copy_to(&mut out);
    Ok(out)
}

// ===========================================================================
// Вспомогательные преобразования
// ===========================================================================

async fn bytes_to_bitmap(png_bytes: &[u8]) -> Result<ImageBitmap, JsValue> {
    let u8arr = js_sys::Uint8Array::from(png_bytes);
    let parts = js_sys::Array::new();
    parts.push(&u8arr.buffer());
    let blob = Blob::new_with_buffer_source_sequence(&parts)?;

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let promise = window.create_image_bitmap_with_blob(&blob)?;
    let value = JsFuture::from(promise).await?;
    value
        .dyn_into::<ImageBitmap>()
        .map_err(|_| JsValue::from_str("не ImageBitmap"))
}

async fn canvas_to_bytes(canvas: &OffscreenCanvas) -> Result<Vec<u8>, JsValue> {
    let promise = canvas.convert_to_blob()?;
    let value = JsFuture::from(promise).await?;
    let blob: Blob = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Blob"))?;
    let ab_value = JsFuture::from(blob.array_buffer()).await?;
    let ab: js_sys::ArrayBuffer = ab_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let u8arr = js_sys::Uint8Array::new(&ab);
    let len = u8arr.length() as usize;
    let mut out: Vec<u8> = vec![0u8; len];
    u8arr.copy_to(&mut out);
    Ok(out)
}

// ===========================================================================
// Диагностика для тестов
// ===========================================================================

/// Проверить, что изображение grayscale — все пиксели имеют R==G==B.
///
/// Сравнивать результаты разных движков попиксельно бессмысленно —
/// они используют разные цветовые пространства и разные алгоритмы
/// сведения. А вот проверить, что **результат действительно
/// grayscale** — можно и нужно.
pub fn check_grayscale(png_bytes: &[u8]) -> Result<(), JsValue> {
    let img = image::load_from_memory(png_bytes).map_err(|e| {
        let msg = format_str(format_args!("decode: {e}"));
        JsValue::from_str(&msg)
    })?;
    let rgba = img.to_rgba8();
    for (x, y, p) in rgba.enumerate_pixels() {
        let [r, g, b, _] = p.0;
        if r != g || g != b {
            let msg = format_str(format_args!(
                "pixel ({x},{y}) не grayscale: R={r} G={g} B={b}"
            ));
            return Err(JsValue::from_str(&msg));
        }
    }
    Ok(())
}

/// Сгенерировать тестовый PNG с градиентом под углом `angle` (радианы).
///
/// Рисует цветной паттерн: R — градиент вдоль оси X, повёрнутой на
/// `angle`; G — вдоль оси Y, повёрнутой на `angle`; B — константа.
///
/// Каждый вызов с новым углом даёт **визуально другую картинку**.
/// Это нужно демо: пользователь видит, что перезапуск действительно
/// пересоздал исходник, а не переиспользовал старый.
pub fn generate_test_png(
    width: u32,
    height: u32,
    angle: f64,
) -> Result<Vec<u8>, JsValue> {
    use image::{ImageBuffer, Rgb, RgbImage};

    let w = width.max(1);
    let h = height.max(1);

    let wf = w as f32;
    let hf = h as f32;
    let cx = wf / 2.0;
    let cy = hf / 2.0;
    let cos_a = (angle as f32).cos();
    let sin_a = (angle as f32).sin();

    let img: RgbImage = ImageBuffer::from_fn(w, h, |x, y| {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;

        // Проекция на повёрнутую ось X.
        let u = dx * cos_a + dy * sin_a;
        // Проекция на повёрнутую ось Y.
        let v = -dx * sin_a + dy * cos_a;

        let tr = ((u / wf) + 0.5).clamp(0.0, 1.0);
        let tg = ((v / hf) + 0.5).clamp(0.0, 1.0);

        let r = (tr * 255.0) as u8;
        let g = (tg * 255.0) as u8;
        let b = 128u8;

        Rgb([r, g, b])
    });

    let mut out: Vec<u8> = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut out);
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| {
                let msg = format_str(format_args!("encode: {e}"));
                JsValue::from_str(&msg)
            })?;
    }
    Ok(out)
}