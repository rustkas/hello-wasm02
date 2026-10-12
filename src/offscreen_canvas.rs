//! 22.1. OffscreenCanvas: canvas вне DOM.
//!
//! Обёртки вокруг `OffscreenCanvas` и его API:
//!
//! - **feature detection** — поддерживает ли браузер `OffscreenCanvas`
//!   и `HtmlCanvasElement::transferControlToOffscreen`;
//! - **два способа создания** — `new OffscreenCanvas(w, h)` и
//!   `canvas.transferControlToOffscreen()`;
//! - **получение 2D-контекста** — через `getContext("2d")`;
//! - **размеры буфера** — `width`/`height` в пикселях, не CSS;
//! - **опциональные методы** — `convertToBlob`, `transferToImageBitmap`.
//!
//! ## Утилита `bool_of`
//!
//! rust-analyzer стабильно теряет тип `matches!` в tail-позиции
//! функции: `expected bool, found ()`. Обёртка `bool_of(matches!(…))`
//! с явной сигнатурой возвращает типу конкретность. См. подробнее
//! в комментарии к самой функции.

use js_sys::{Function, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, OffscreenCanvas, OffscreenCanvasRenderingContext2d,
};

// =============================================================================
// Утилита для обхода бага rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `matches!` для tail-позиций.
///
/// rust-analyzer теряет тип `matches!` в хвостовой позиции функции
/// и выдаёт `expected bool, found ()`. Обёртка с явной сигнатурой
/// возвращает типу конкретность и снимает ошибку. На кодогенерацию
/// не влияет — `#[inline]` + identity-функция компилируются в ничто.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Feature detection
// =============================================================================

/// Глобальный конструктор `OffscreenCanvas` доступен.
pub fn is_offscreen_canvas_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("OffscreenCanvas")),
        Ok(v) if v.is_function()
    ))
}

/// `HtmlCanvasElement::transferControlToOffscreen` доступен.
pub fn is_transfer_control_supported() -> bool {
    let global = js_sys::global();
    let Ok(document) = Reflect::get(&global, &JsValue::from_str("document")) else {
        return false;
    };
    let Ok(create) = Reflect::get(&document, &JsValue::from_str("createElement")) else {
        return false;
    };
    let Ok(create) = create.dyn_into::<Function>() else {
        return false;
    };
    let Ok(canvas) = create.call1(&document, &JsValue::from_str("canvas")) else {
        return false;
    };
    bool_of(matches!(
        Reflect::get(&canvas, &JsValue::from_str("transferControlToOffscreen")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// Два способа создания
// =============================================================================

/// Создаёт `OffscreenCanvas` напрямую, без DOM-элемента.
///
/// Используется, когда холст нужен только для пиксельной обработки
/// и не показывается пользователю.
pub fn new_offscreen(width: u32, height: u32) -> Result<OffscreenCanvas, JsValue> {
    OffscreenCanvas::new(width, height)
}

/// Отзывает управление у DOM-canvas'а и возвращает `OffscreenCanvas`.
///
/// Одноразовая операция: повторный вызов на том же элементе даёт
/// `InvalidStateError`.
pub fn transfer_control(canvas: &HtmlCanvasElement) -> Result<OffscreenCanvas, JsValue> {
    canvas.transfer_control_to_offscreen()
}

// =============================================================================
// Контекст
// =============================================================================

/// Получает 2D-контекст у `OffscreenCanvas`.
///
/// `OffscreenCanvas::get_context` возвращает `Object` — конкретный тип
/// контекста зависит от строки. Приводим через `dyn_into`.
pub fn get_2d(
    canvas: &OffscreenCanvas,
) -> Result<OffscreenCanvasRenderingContext2d, JsValue> {
    let ctx = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("2d context unavailable"))?;
    ctx.dyn_into::<OffscreenCanvasRenderingContext2d>()
        .map_err(|_| JsValue::from_str("not an OffscreenCanvasRenderingContext2d"))
}

// =============================================================================
// Размеры буфера
// =============================================================================

/// Ширина буфера в пикселях. **Не** CSS-размер.
pub fn width_of(canvas: &OffscreenCanvas) -> u32 {
    canvas.width()
}

/// Высота буфера в пикселях. **Не** CSS-размер.
pub fn height_of(canvas: &OffscreenCanvas) -> u32 {
    canvas.height()
}

// =============================================================================
// Опциональные методы
// =============================================================================

/// `convertToBlob` доступен на этом `OffscreenCanvas`.
///
/// Метод не во всех браузерах (Safari добавил позже), поэтому
/// наличие проверяется через Reflect.
pub fn has_convert_to_blob(canvas: &OffscreenCanvas) -> bool {
    bool_of(matches!(
        Reflect::get(canvas.as_ref(), &JsValue::from_str("convertToBlob")),
        Ok(v) if v.is_function()
    ))
}

/// `transferToImageBitmap` доступен на этом `OffscreenCanvas`.
pub fn has_transfer_to_image_bitmap(canvas: &OffscreenCanvas) -> bool {
    bool_of(matches!(
        Reflect::get(canvas.as_ref(), &JsValue::from_str("transferToImageBitmap")),
        Ok(v) if v.is_function()
    ))
}