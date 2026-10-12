//! 22. OffscreenCanvas и Workers.
//!
//! Обёртки для работы с `Worker`, `OffscreenCanvas` и transferable
//! объектами. Модуль разделён на две части:
//!
//! - **feature detection** — поддерживает ли браузер `Worker` и
//!   `transferControlToOffscreen`;
//! - **обвязка** — создание Worker'а, передача canvas'а, отправка
//!   сообщений с transferable и без.
//!
//! ## Утилиты для обхода багов rust-analyzer
//!
//! `format!` и `matches!` в tail-позиции функции теряют тип и дают
//! `expected String, found ()` / `expected bool, found ()`. Обход —
//! обернуть в функцию с явной сигнатурой: `format_str` и `bool_of`.

use std::fmt::Arguments;

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Blob, HtmlCanvasElement, ImageData, OffscreenCanvas, Worker};

// =============================================================================
// Утилиты для обхода багов rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `format!` для tail-позиций.
#[inline]
pub fn format_str(args: Arguments<'_>) -> String {
    args.to_string()
}

/// Обёртка над `matches!` для tail-позиций.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Feature detection
// =============================================================================

/// `Worker` доступен в этом браузере.
///
/// Использует `bool_of(matches!(...))` — обход бага rust-analyzer
/// с `matches!` в tail-позиции.
pub fn is_worker_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("Worker")),
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
    let Ok(create) = create.dyn_into::<js_sys::Function>() else {
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
// Создание Worker
// =============================================================================

/// Создаёт `Worker` из URL. Ошибка — если URL недоступен или Worker
/// не может быть создан (CSP, неверный путь).
pub fn create_worker(url: &str) -> Result<Worker, JsValue> {
    Worker::new(url)
}

// =============================================================================
// Передача canvas в Worker
// =============================================================================

/// Отзывает управление canvas'ом у main thread и возвращает
/// `OffscreenCanvas`.
///
/// После вызова оригинальный `<canvas>` больше не может быть объектом
/// `getContext`; его содержимым управляет тот, кому передан
/// `OffscreenCanvas`.
///
/// Возвращает ошибку, если `transferControlToOffscreen` недоступен
/// или уже был вызван на этом элементе.
pub fn transfer_control(canvas: &HtmlCanvasElement) -> Result<OffscreenCanvas, JsValue> {
    canvas.transfer_control_to_offscreen()
}

/// Отправляет `OffscreenCanvas` Worker'у как transferable.
///
/// Сообщение — `{ type: "init", canvas: <offscreen>, width, height, palette, count }`.
/// Массив transferable содержит только `offscreen`.
pub fn post_init(
    worker: &Worker,
    canvas: OffscreenCanvas,
    width: u32,
    height: u32,
    palette: &[&str],
    count: u32,
) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("init"))?;
    Reflect::set(&msg, &JsValue::from_str("canvas"), canvas.as_ref())?;
    Reflect::set(
        &msg,
        &JsValue::from_str("width"),
        &JsValue::from_f64(width as f64),
    )?;
    Reflect::set(
        &msg,
        &JsValue::from_str("height"),
        &JsValue::from_f64(height as f64),
    )?;
    Reflect::set(&msg, &JsValue::from_str("count"), &JsValue::from_f64(count as f64))?;

    let pal = Array::new();
    for c in palette {
        pal.push(&JsValue::from_str(c));
    }
    Reflect::set(&msg, &JsValue::from_str("palette"), pal.as_ref())?;

    let transfer = Array::new();
    transfer.push(canvas.as_ref());

    worker.post_message_with_transfer(&msg, &transfer)
}

// =============================================================================
// Команды Worker'у
// =============================================================================

/// Отправляет простую команду `{ type: "..." }`.
pub fn post_command(worker: &Worker, cmd_type: &str) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(
        &msg,
        &JsValue::from_str("type"),
        &JsValue::from_str(cmd_type),
    )?;
    worker.post_message(&msg)
}

/// Отправляет команду с числовым параметром `{ type: "count", count }`.
pub fn post_count(worker: &Worker, count: u32) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("count"))?;
    Reflect::set(
        &msg,
        &JsValue::from_str("count"),
        &JsValue::from_f64(count as f64),
    )?;
    worker.post_message(&msg)
}

/// Отправляет палитру `{ type: "palette", palette: [...] }`.
pub fn post_palette(worker: &Worker, palette: &[&str]) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(
        &msg,
        &JsValue::from_str("type"),
        &JsValue::from_str("palette"),
    )?;
    let pal = Array::new();
    for c in palette {
        pal.push(&JsValue::from_str(c));
    }
    Reflect::set(&msg, &JsValue::from_str("palette"), pal.as_ref())?;
    worker.post_message(&msg)
}

// =============================================================================
// Transferable: послать буфер и вернуть его же назад
// =============================================================================

/// Создаёт `ArrayBuffer` заданного размера, отправляет его Worker'у
/// как **transferable** и возвращает `t0` (время отправки).
///
/// Используется в демо: после ответа Worker'а (с тем же буфером,
/// тоже transferable) main thread измеряет round-trip.
///
/// Владение буфером теряется main thread'ом сразу после `postMessage`.
pub fn send_transfer_test(worker: &Worker, size: usize) -> Result<f64, JsValue> {
    let u8 = Uint8Array::new_with_length(size as u32);
    for i in 0..size.min(1024) {
        u8.set_index(i as u32, (i & 0xFF) as u8);
    }
    let buffer: JsValue = u8.buffer().into();

    let msg = Object::new();
    Reflect::set(
        &msg,
        &JsValue::from_str("type"),
        &JsValue::from_str("transfer_test"),
    )?;
    Reflect::set(&msg, &JsValue::from_str("buffer"), &buffer)?;
    Reflect::set(
        &msg,
        &JsValue::from_str("size"),
        &JsValue::from_f64(size as f64),
    )?;
    let t0 = js_sys::Date::now();
    Reflect::set(&msg, &JsValue::from_str("t0"), &JsValue::from_f64(t0))?;

    let transfer = Array::new();
    transfer.push(&buffer);
    worker.post_message_with_transfer(&msg, &transfer)?;
    Ok(t0)
}

/// То же самое, но без transferable — буфер **копируется**.
/// main thread сохраняет владение оригиналом.
pub fn send_copy_test(worker: &Worker, size: usize) -> Result<f64, JsValue> {
    let u8 = Uint8Array::new_with_length(size as u32);
    for i in 0..size.min(1024) {
        u8.set_index(i as u32, (i & 0xFF) as u8);
    }

    let msg = Object::new();
    Reflect::set(
        &msg,
        &JsValue::from_str("type"),
        &JsValue::from_str("transfer_test"),
    )?;
    // Передаём сам Uint8Array (копируется, потому что не в transfer list).
    Reflect::set(&msg, &JsValue::from_str("buffer"), u8.as_ref())?;
    Reflect::set(
        &msg,
        &JsValue::from_str("size"),
        &JsValue::from_f64(size as f64),
    )?;
    let t0 = js_sys::Date::now();
    Reflect::set(&msg, &JsValue::from_str("t0"), &JsValue::from_f64(t0))?;

    // Пустой transfer list — копирование.
    worker.post_message(&msg)?;
    Ok(t0)
}

// =============================================================================
// Размеры данных
// =============================================================================

/// Сколько байт в `ImageData`.
///
/// `ImageData::data()` возвращает `Clamped<Vec<u8>>` — tuple struct
/// с публичным полем `.0`. Длину берём у внутреннего `Vec<u8>`:
/// у `Clamped` нет собственного метода `length` или `len`.
pub fn image_data_size(data: &ImageData) -> usize {
    data.data().0.len()
}

/// Сколько байт в `Blob`. `Blob::size()` возвращает `f64`.
pub fn blob_size(blob: &Blob) -> f64 {
    blob.size()
}