//! 21.7. Image Processor Pipeline.
//!
//! Расширение базового модуля `canvas_image_basic`: полный workflow
//! от файла пользователя до скачанного результата.
//!
//! ## Чем отличается от `canvas_image_basic`
//!
//! | Возможность | `basic` | `pipeline` |
//! |---|---|---|
//! | Загрузка через `<img>.decode()` | ✅ | — |
//! | Загрузка через `createImageBitmap` с EXIF | — | ✅ |
//! | Возврат сразу `Vec<u8>` | — | ✅ |
//! | Экспорт в PNG Blob | — | ✅ |
//! | Скачивание (`<a download>` + `showSaveFilePicker`) | — | ✅ |
//! | Round-trip с `HtmlCanvasElement` | ✅ | — |
//!
//! Оба модуля дополняют друг друга. `pipeline` показывает **полный
//! workflow**: загрузили → обработали → отдали. Плюс применяет
//! EXIF-ориентацию (21.5) — фото с телефона отображается правильно.
//!
//! Фильтры берутся из `canvas_filters` (21.6) — намеренная композиция.
//!
//! ## cfg-хак для `getImageData` / `putImageData`
//!
//! Сигнатуры зависят от флага `web_sys_unstable_apis`: `f64` в stable,
//! `i32` в unstable. Различие инкапсулировано в хелперах
//! `read_image_data` / `write_image_data_offscreen`.

use std::fmt::Arguments;

use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{
    Blob, HtmlAnchorElement, ImageBitmap, ImageData, OffscreenCanvas,
    OffscreenCanvasRenderingContext2d, Url,
};

use crate::canvas_filters::{box_blur_3x3, invert, sobel_edge, to_grayscale};

// =============================================================================
// Утилиты для обхода багов rust-analyzer в tail-позиции
// =============================================================================
//
// rust-analyzer стабильно теряет тип `format!` и `matches!`, когда они
// стоят последним выражением функции, `if`-ветки или `match`-арма.
// Диагностика: `expected String, found ()` / `expected bool, found ()`.
//
// Обход — обернуть макрос в функцию с явной сигнатурой. Инференс не
// зависит от контекста вызова, и rust-analyzer больше не ошибается.

/// Обёртка над `format!` для tail-позиций.
#[inline]
fn format_str(args: Arguments<'_>) -> String {
    args.to_string()
}

/// Обёртка над `matches!` для tail-позиций.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Палитра фильтров
// =============================================================================

/// Палитра фильтров пайплайна. `Original` — identity, остальные делегируют
/// в `canvas_filters` (21.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Original,
    Grayscale,
    Invert,
    Blur,
    Sobel,
}

impl Filter {
    pub fn label(self) -> &'static str {
        match self {
            Filter::Original => "Оригинал",
            Filter::Grayscale => "Оттенки серого",
            Filter::Invert => "Инверсия",
            Filter::Blur => "Размытие",
            Filter::Sobel => "Границы (Sobel)",
        }
    }

    pub fn apply(self, rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
        match self {
            Filter::Original => rgba.to_vec(),
            Filter::Grayscale => to_grayscale(rgba),
            Filter::Invert => invert(rgba),
            Filter::Blur => box_blur_3x3(rgba, w, h),
            Filter::Sobel => sobel_edge(rgba, w, h),
        }
    }
}

// =============================================================================
// cfg-хелперы для несовместимых сигнатур
// =============================================================================

#[cfg(web_sys_unstable_apis)]
fn read_image_data(
    ctx: &OffscreenCanvasRenderingContext2d,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(0, 0, w as i32, h as i32)
}

#[cfg(not(web_sys_unstable_apis))]
fn read_image_data(
    ctx: &OffscreenCanvasRenderingContext2d,
    w: u32,
    h: u32,
) -> Result<ImageData, JsValue> {
    ctx.get_image_data(0.0, 0.0, w as f64, h as f64)
}

#[cfg(web_sys_unstable_apis)]
fn write_image_data_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    data: &ImageData,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, 0, 0)
}

#[cfg(not(web_sys_unstable_apis))]
fn write_image_data_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    data: &ImageData,
) -> Result<(), JsValue> {
    ctx.put_image_data(data, 0.0, 0.0)
}

// =============================================================================
// Загрузка с EXIF
// =============================================================================

/// Загружает Blob через `createImageBitmap` с применением EXIF-ориентации,
/// рисует на OffscreenCanvas, читает `ImageData`.
///
/// Возвращает `(pixels, w, h, resized)`. Размеры клампятся до `max_dim`
/// по длинной стороне. `resized` сообщает, было ли применено
/// масштабирование — чтобы UI мог честно сказать «resize: да».
pub async fn load_image_as_pixels(
    blob: &Blob,
    max_dim: u32,
) -> Result<(Vec<u8>, u32, u32, bool), JsValue> {
    let bitmap = create_image_bitmap_with_exif(blob).await?;
    let iw = bitmap.width();
    let ih = bitmap.height();
    if iw == 0 || ih == 0 {
        return Err(JsValue::from_str("image has zero size"));
    }

    let resized = iw.max(ih) > max_dim;
    let (w, h) = if resized {
        let scale = max_dim as f64 / iw.max(ih) as f64;
        (
            (iw as f64 * scale).round().max(1.0) as u32,
            (ih as f64 * scale).round().max(1.0) as u32,
        )
    } else {
        (iw, ih)
    };

    let canvas = OffscreenCanvas::new(w, h)?;
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    let ctx: OffscreenCanvasRenderingContext2d = ctx_js.dyn_into()?;
    ctx.draw_image_with_image_bitmap_and_dw_and_dh(
        &bitmap, 0.0, 0.0, w as f64, h as f64,
    )?;
    bitmap.close();

    let data = read_image_data(&ctx, w, h)?;
    let pixels = data.data().0;
    Ok((pixels, w, h, resized))
}

/// `createImageBitmap(blob, { imageOrientation: "from-image" })`.
async fn create_image_bitmap_with_exif(blob: &Blob) -> Result<ImageBitmap, JsValue> {
    let global = js_sys::global();
    let func_value = Reflect::get(&global, &JsValue::from_str("createImageBitmap"))?;
    if !func_value.is_function() {
        return Err(JsValue::from_str(
            "createImageBitmap not available in this browser",
        ));
    }
    let func: Function = func_value.dyn_into()?;

    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("imageOrientation"),
        &JsValue::from_str("from-image"),
    )?;

    let promise: js_sys::Promise = func.call2(&global, blob.as_ref(), &opts)?.dyn_into()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    value.dyn_into::<ImageBitmap>()
}

// =============================================================================
// Экспорт: пиксели → PNG Blob
// =============================================================================

/// Рисует `Vec<u8>` на OffscreenCanvas и конвертирует в PNG Blob.
pub async fn pixels_to_png_blob(pixels: &[u8], w: u32, h: u32) -> Result<Blob, JsValue> {
    let expected = (w as usize) * (h as usize) * 4;
    if pixels.len() != expected {
        return Err(JsValue::from_str(&format!(
            "pixel buffer size {} != {} for {}x{}",
            pixels.len(), expected, w, h,
        )));
    }

    let canvas = OffscreenCanvas::new(w, h)?;
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    let ctx: OffscreenCanvasRenderingContext2d = ctx_js.dyn_into()?;

    let data = ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixels), w, h)?;
    write_image_data_offscreen(&ctx, &data)?;

    let promise = canvas.convert_to_blob()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    value.dyn_into::<Blob>()
}

// =============================================================================
// Скачивание
// =============================================================================

/// Программный клик по невидимому `<a download>`.
pub fn trigger_anchor_download(blob: &Blob, filename: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let url = Url::create_object_url_with_blob(blob)?;
    let anchor = document
        .create_element("a")?
        .dyn_into::<HtmlAnchorElement>()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display: none;")?;

    let body = document.body().ok_or_else(|| JsValue::from_str("no body"))?;
    body.append_child(&anchor)?;
    anchor.click();
    body.remove_child(&anchor)?;
    let _ = Url::revoke_object_url(&url);
    Ok(())
}

/// `showSaveFilePicker` присутствует в этом браузере?
///
/// Использует `bool_of(matches!(...))` — обход бага rust-analyzer:
/// `matches!` в tail-позиции теряет тип.
pub fn is_save_picker_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("showSaveFilePicker")),
        Ok(v) if v.is_function()
    ))
}

/// «Сохранить как…» через File System Access API.
///
/// - `Ok(Some(name))` — сохранено;
/// - `Ok(None)` — пользователь отменил (AbortError — не ошибка);
/// - `Err(e)` — реальная ошибка.
pub async fn save_via_picker(
    blob: Blob,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    if !is_save_picker_supported() {
        return Err(JsValue::from_str(
            "showSaveFilePicker not supported in this browser",
        ));
    }

    let global = js_sys::global();
    let picker = Reflect::get(&global, &JsValue::from_str("showSaveFilePicker"))?;
    let picker: Function = picker.dyn_into()?;

    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;

    let promise: js_sys::Promise = picker.call1(&global, &opts)?.dyn_into()?;
    let handle = match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(h) => h,
        Err(e) if is_abort_error(&e) => return Ok(None),
        Err(e) => return Err(e),
    };

    let writable_promise = call_method(&handle, "createWritable", &[])?;
    let writable = wasm_bindgen_futures::JsFuture::from(writable_promise).await?;

    let write_promise = call_method(&writable, "write", &[blob.as_ref()])?;
    wasm_bindgen_futures::JsFuture::from(write_promise).await?;

    let close_promise = call_method(&writable, "close", &[])?;
    wasm_bindgen_futures::JsFuture::from(close_promise).await?;

    let name = Reflect::get(&handle, &JsValue::from_str("name"))?;
    Ok(Some(name.as_string().unwrap_or_else(|| suggested_name.to_string())))
}

fn call_method(
    obj: &JsValue,
    name: &str,
    args: &[&JsValue],
) -> Result<js_sys::Promise, JsValue> {
    let func_js = Reflect::get(obj, &JsValue::from_str(name))?;
    if !func_js.is_function() {
        return Err(JsValue::from_str(&format!("{name} is not a function")));
    }
    let func: Function = func_js.dyn_into()?;
    let args_arr = Array::new();
    for a in args {
        args_arr.push(a);
    }
    let ret = func.apply(obj, &args_arr)?;
    ret.dyn_into::<js_sys::Promise>()
}

fn is_abort_error(e: &JsValue) -> bool {
    match Reflect::get(e, &JsValue::from_str("name")) {
        Ok(v) => v.as_string().as_deref() == Some("AbortError"),
        Err(_) => false,
    }
}

// =============================================================================
// Утилиты
// =============================================================================

/// Имя для выходного файла: `photo.jpg` + `processed` → `photo.processed.png`.
///
/// Использует `format_str(format_args!(...))` — обход бага rust-analyzer:
/// `format!` в tail-позиции теряет тип.
pub fn derived_png_name(input_name: &str, suffix: &str) -> String {
    let stem = input_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(input_name);
    format_str(format_args!("{stem}.{suffix}.png"))
}