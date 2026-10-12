//! Async ClipboardItem: `Promise<Blob>` и множественные items.
//!
//! `ClipboardItem` (глава 32) принимает готовый `Blob` — данные должны
//! быть собраны до `clipboard.write()`. Для генерации изображения это
//! проблема: 500-2000 мс блокировки между «хочу скопировать» и
//! «write()». UI зависает.
//!
//! Async ClipboardItem решает две задачи:
//!
//!   1. `Promise<Blob>` в конструкторе. `clipboard.write()` стартует
//!      сразу, но завершается, когда промис резолвится. UI остаётся
//!      отзывчивым. Мы можем показывать прогресс.
//!
//!   2. Массив `ClipboardItem[]` в `write()`. Несколько альтернативных
//!      представлений. Получатель (приложение при вставке) сам
//!      выбирает подходящее.
//!
//! ПРАВИЛО: `ClipboardItem` — класс, вызывается через
//! `js_sys::Reflect::construct`.
//!
//! ПРАВИЛО: значения в объекте-источнике могут быть как `Blob`, так
//! и `Promise<Blob>`. Оба варианта валидны.
//!
//! ПРАВИЛО: `canvas.toBlob` — callback API. Оборачиваем в `Promise`
//! через `Closure::once_into_js`.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Function, Promise};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

// ============================================================
//                    ДОСТУП
// ============================================================

fn clipboard() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let nav = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))
        .map_err(|e| JsValue::from_str(&format!("Reflect navigator: {e:?}")))?;
    let clip = js_sys::Reflect::get(&nav, &JsValue::from_str("clipboard"))
        .map_err(|e| JsValue::from_str(&format!("Reflect clipboard: {e:?}")))?;
    if clip.is_undefined() || clip.is_null() {
        return Err(JsValue::from_str(
            "navigator.clipboard недоступен (нужен HTTPS или localhost)",
        ));
    }
    Ok(clip)
}

/// true, если `navigator.clipboard` доступен.
pub fn is_available() -> bool {
    return clipboard().is_ok();
}

/// true, если браузер разрешает записывать этот MIME в `ClipboardItem`.
///
/// Использует статический `ClipboardItem.supports(mime)`. Если метод
/// недоступен — `true` (оптимистично).
pub fn supports_mime(mime: &str) -> bool {
    let global = js_sys::global();
    let ctor = match js_sys::Reflect::get(&global, &JsValue::from_str("ClipboardItem")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return true,
    };
    let supports = match js_sys::Reflect::get(&ctor, &JsValue::from_str("supports")) {
        Ok(v) if !v.is_undefined() => v,
        _ => return true,
    };
    let f: Function = match supports.dyn_into() {
        Ok(f) => f,
        Err(_) => return true,
    };
    f.call1(&ctor, &JsValue::from_str(mime))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

// ============================================================
//                    PROMISE PNG С ЗАДЕРЖКОЙ
// ============================================================

/// Создаёт `Promise<Blob>` с PNG размером `size × size`, который
/// резолвится через `delay_ms` миллисекунд.
///
/// Пока промис не резолвлен, его можно передать в `ClipboardItem` —
/// запись стартует сразу, а данные придут когда будут готовы.
///
/// `on_ready` вызывается с размером готового `Blob` в байтах — для
/// логирования и UI. Вызывается ровно один раз.
///
/// ПРАВИЛО: `Promise::new` создаёт `FnMut`-замыкание. `on_ready` —
/// `FnOnce`, поэтому его нельзя переместить в callback напрямую.
/// Оборачиваем в `Rc<RefCell<Option<F>>>` и вызываем через `take()`.
pub fn make_png_promise<F>(size: u32, delay_ms: u32, on_ready: F) -> Result<Promise, JsValue>
where
    F: FnOnce(u64) + 'static,
{
    let on_ready_cell: Rc<RefCell<Option<F>>> = Rc::new(RefCell::new(Some(on_ready)));

    let promise = Promise::new(&mut move |resolve, reject| {
        let resolve_outer = resolve.clone();
        let reject_outer = reject.clone();
        let on_ready_for_cb = on_ready_cell.clone();

        let cb = Closure::once_into_js(move || {
            let canvas = match create_painted_canvas(size) {
                Ok(c) => c,
                Err(e) => {
                    let _ = reject_outer.call1(&JsValue::UNDEFINED, &e);
                    return;
                }
            };
            let resolve_final = resolve_outer.clone();
            let reject_final = reject_outer.clone();
            let on_ready_inner = on_ready_for_cb.clone();

            let on_blob = Closure::once_into_js(move |blob: JsValue| {
                if blob.is_null() || blob.is_undefined() {
                    let _ = reject_final.call1(
                        &JsValue::UNDEFINED,
                        &JsValue::from_str("canvas.toBlob вернул null"),
                    );
                    return;
                }
                if let Some(b) = blob.dyn_ref::<web_sys::Blob>() {
                    // take() — вызываем FnOnce ровно один раз.
                    // Повторный вызов не сработает: Option уже None.
                    if let Some(f) = on_ready_inner.borrow_mut().take() {
                        f(b.size() as u64);
                    }
                }
                let _ = resolve_final.call1(&JsValue::UNDEFINED, &blob);
            });

            let to_blob: Function =
                match js_sys::Reflect::get(&canvas, &JsValue::from_str("toBlob"))
                    .and_then(|v| v.dyn_into::<Function>())
                {
                    Ok(f) => f,
                    Err(_) => {
                        let _ = reject_outer.call1(
                            &JsValue::UNDEFINED,
                            &JsValue::from_str("toBlob не функция"),
                        );
                        return;
                    }
                };
            let _ = to_blob.call2(
                &canvas,
                on_blob.unchecked_ref(),
                &JsValue::from_str("image/png"),
            );
        });

        let win = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.unchecked_ref(),
            delay_ms as i32,
        );
    });
    Ok(promise)
}

/// Рисует canvas — градиент, круг, цифра.
fn create_painted_canvas(size: u32) -> Result<web_sys::HtmlCanvasElement, JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no doc")?;
    let canvas = document
        .create_element("canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("createElement('canvas') не canvas"))?;
    canvas.set_width(size);
    canvas.set_height(size);
    let ctx = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| JsValue::from_str("ctx не CanvasRenderingContext2d"))?;

    let gradient = ctx.create_linear_gradient(0.0, 0.0, size as f64, size as f64);
    let _ = gradient.add_color_stop(0.0, "#3498db");
    let _ = gradient.add_color_stop(1.0, "#9b59b6");
    ctx.set_fill_style_canvas_gradient(&gradient);
    ctx.fill_rect(0.0, 0.0, size as f64, size as f64);

    let r = size as f64 / 2.0;
    ctx.set_fill_style_str("rgba(255,255,255,0.8)");
    ctx.begin_path();
    let _ = ctx.arc(r, r, r * 0.5, 0.0, std::f64::consts::PI * 2.0);
    ctx.fill();

    let label = size.to_string();
    ctx.set_fill_style_str("#fff");
    ctx.set_font("bold 14px system-ui");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(&label, r, r);

    Ok(canvas)
}

// ============================================================
//                    BLOB ДЛЯ ТЕКСТА
// ============================================================

/// Создаёт `Blob` из строки с указанным MIME.
pub fn make_text_blob(text: &str, mime: &str) -> Result<web_sys::Blob, JsValue> {
    let parts = Array::new();
    parts.push(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
        .map_err(|e| JsValue::from_str(&format!("Blob::new: {e:?}")))
}

// ============================================================
//                    CLIPBOARD ITEM
// ============================================================

/// Создаёт `ClipboardItem` из набора источников.
///
/// Каждое значение может быть как `Blob`, так и `Promise<Blob>`.
/// `mime` — ключ объекта-источника; например, `"image/png"` или
/// `"text/plain"`.
pub fn make_item(sources: &[(String, JsValue)]) -> Result<JsValue, JsValue> {
    let data = js_sys::Object::new();
    for (mime, value) in sources {
        js_sys::Reflect::set(&data, &JsValue::from_str(mime), value)
            .map_err(|e| JsValue::from_str(&format!("set {mime}: {e:?}")))?;
    }
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("ClipboardItem"))
        .map_err(|e| JsValue::from_str(&format!("Reflect ClipboardItem: {e:?}")))?;
    if ctor.is_undefined() || ctor.is_null() {
        return Err(JsValue::from_str("ClipboardItem недоступен"));
    }
    let ctor: Function = ctor
        .dyn_into()
        .map_err(|_| JsValue::from_str("ClipboardItem не конструктор"))?;
    let args = Array::new();
    args.push(&data);
    js_sys::Reflect::construct(&ctor, &args)
        .map_err(|e| JsValue::from_str(&format!("new ClipboardItem: {e:?}")))
}

// ============================================================
//                    WRITE
// ============================================================

/// Вызывает `clipboard.write(items)` и возвращает **сырой** `Promise`.
///
/// Вызывающий сам решает, как ждать: через `JsFuture::from` или
/// параллельно с другими задачами — например, обновляя прогресс-бар.
pub fn write_items_promise(items: &[JsValue]) -> Result<Promise, JsValue> {
    let clip = clipboard()?;
    let arr = Array::new();
    for item in items {
        arr.push(item);
    }
    let write_fn: Function = js_sys::Reflect::get(&clip, &JsValue::from_str("write"))
        .map_err(|e| JsValue::from_str(&format!("Reflect write: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str("write не функция"))?;
    let promise_val = write_fn
        .call1(&clip, &arr)
        .map_err(|e| JsValue::from_str(&format!("write: {e:?}")))?;
    promise_val
        .dyn_into::<Promise>()
        .map_err(|_| JsValue::from_str("write не вернул Promise"))
}

/// Удобная обёртка: вызывает `write` и ждёт резолва.
///
/// Для сценариев, где не нужен прогресс.
pub async fn write_items(items: &[JsValue]) -> Result<(), JsValue> {
    let promise = write_items_promise(items)?;
    JsFuture::from(promise).await?;
    Ok(())
}