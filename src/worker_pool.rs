//! 22.5. Worker pools и SharedArrayBuffer.
//!
//! Пул из N Worker'ов: задача распределяется по worker'ам,
//! результаты агрегируются. Плюс feature detection для
//! `SharedArrayBuffer` и `Atomics`.
//!
//! ## Что здесь есть
//!
//! - **Feature detection** — `SharedArrayBuffer`, `Atomics`, `Worker`.
//! - **Пул** — `create_pool(url, size)` возвращает `Vec<Worker>`.
//! - **Отправка задачи** — `post_stripe` отправляет полосу Worker'у.
//! - **Агрегация** — `extract_stripe_result` извлекает результат.
//! - **Завершение** — `terminate_pool` останавливает все Worker'ы.
//!
//! ## Утилита `bool_of`
//!
//! rust-analyzer теряет тип `matches!` в tail-позиции. Обёртка
//! возвращает типу конкретность.

use std::fmt::Arguments;

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

// =============================================================================
// Утилита для обхода бага rust-analyzer в tail-позиции
// =============================================================================

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
// Feature detection
// =============================================================================

/// `Worker` доступен в этом браузере.
pub fn is_worker_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("Worker")),
        Ok(v) if v.is_function()
    ))
}

/// `SharedArrayBuffer` доступен.
///
/// Требует COOP/COEP-заголовков. Без них — undefined, даже в
/// современных браузерах.
pub fn is_shared_array_buffer_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("SharedArrayBuffer")),
        Ok(v) if v.is_function()
    ))
}


/// `Atomics` доступен.
pub fn is_atomics_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("Atomics")),
        Ok(v) if !v.is_undefined() && !v.is_null()
    ))
}

/// Страница является cross-origin isolated.
///
/// Это `self.crossOriginIsolated === true` — прямой индикатор того,
/// что COOP/COEP-заголовки применились и браузер открыл SAB.
pub fn is_cross_origin_isolated() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("crossOriginIsolated")),
        Ok(v) if v.as_bool() == Some(true)
    ))
}

/// Человекочитаемый статус cross-origin isolation.
pub fn coi_status() -> String {
    if is_cross_origin_isolated() {
        String::from("да")
    } else {
        String::from("нет")
    }
}

/// Человекочитаемый статус SharedArrayBuffer с точной диагностикой.
///
/// Различает три случая:
/// - SAB доступен — заголовки есть, браузер поддерживает.
/// - COI включён, но SAB нет — редкий случай, браузер старый.
/// - COI выключен — заголовки не применены или кэш.
pub fn sab_status() -> String {
    let sab = is_shared_array_buffer_supported();
    let atomics = is_atomics_supported();
    let coi = is_cross_origin_isolated();
    if sab && atomics {
        String::from("доступен")
    } else if coi && !sab {
        String::from("COI есть, но SAB не exposed (старый браузер?)")
    } else {
        String::from("недоступен (нет COOP/COEP)")
    }
}

// =============================================================================
// Пул Worker'ов
// =============================================================================

/// Создаёт пул из `size` module worker'ов.
///
/// Все Worker'ы используют один URL, но каждый — отдельный
/// JavaScript-контекст со своим WASM-инстансом.
pub fn create_pool(url: &str, size: usize) -> Result<Vec<Worker>, JsValue> {
    let opts = WorkerOptions::new();
    opts.set_type(WorkerType::Module);

    let mut workers = Vec::with_capacity(size);
    for _ in 0..size {
        let w = Worker::new_with_options(url, &opts)?;
        workers.push(w);
    }
    Ok(workers)
}

/// Принудительно завершает все Worker'ы пула.
pub fn terminate_pool(workers: &[Worker]) {
    for w in workers {
        w.terminate();
    }
}

// =============================================================================
// Отправка задачи Worker'у
// =============================================================================

/// Отправляет полосу Worker'у: `{ type: "stripe", buffer, y, w, h, passes, slot }`.
///
/// `slot` — индекс Worker'а в пуле. Возвращается в ответе, чтобы main
/// thread знал, чей это результат.
///
/// `buffer` создаётся в JS-heap и трансферится — zero-copy.
pub fn post_stripe(
    worker: &Worker,
    data: &[u8],
    y_offset: u32,
    stripe_w: u32,
    stripe_h: u32,
    passes: u32,
    slot: u32,
) -> Result<(), JsValue> {
    let arr = Uint8Array::new_with_length(data.len() as u32);
    arr.copy_from(data);
    let buffer: JsValue = arr.buffer().into();

    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("stripe"))?;
    Reflect::set(&msg, &JsValue::from_str("buffer"), &buffer)?;
    Reflect::set(&msg, &JsValue::from_str("y"), &JsValue::from_f64(y_offset as f64))?;
    Reflect::set(&msg, &JsValue::from_str("width"), &JsValue::from_f64(stripe_w as f64))?;
    Reflect::set(&msg, &JsValue::from_str("height"), &JsValue::from_f64(stripe_h as f64))?;
    Reflect::set(&msg, &JsValue::from_str("passes"), &JsValue::from_f64(passes as f64))?;
    Reflect::set(&msg, &JsValue::from_str("slot"), &JsValue::from_f64(slot as f64))?;

    let transfer = Array::new();
    transfer.push(&buffer);
    worker.post_message_with_transfer(&msg, &transfer)
}

// =============================================================================
// Результат от Worker'а
// =============================================================================

/// Результат обработки полосы.
pub struct StripeResult {
    pub bytes: Vec<u8>,
    pub y_offset: u32,
    pub width: u32,
    pub height: u32,
    pub elapsed_ms: f64,
    pub slot: u32,
}

/// Извлекает результат из `MessageEvent`.
///
/// Ожидает `{ type: "stripe_done", buffer, y, width, height, elapsed_ms, slot }`.
pub fn extract_stripe_result(event: &MessageEvent) -> Option<StripeResult> {
    let data = event.data();

    let kind = Reflect::get(&data, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())?;
    if kind != "stripe_done" {
        return None;
    }

    let buffer_val = Reflect::get(&data, &JsValue::from_str("buffer")).ok()?;
    let arr: Uint8Array = buffer_val.dyn_into().ok()?;
    let bytes: Vec<u8> = arr.to_vec();

    let y_offset = Reflect::get(&data, &JsValue::from_str("y")).ok()?.as_f64()? as u32;
    let width = Reflect::get(&data, &JsValue::from_str("width"))
        .ok()?
        .as_f64()? as u32;
    let height = Reflect::get(&data, &JsValue::from_str("height"))
        .ok()?
        .as_f64()? as u32;
    let elapsed_ms = Reflect::get(&data, &JsValue::from_str("elapsed_ms"))
        .ok()?
        .as_f64()?;
    let slot = Reflect::get(&data, &JsValue::from_str("slot"))
        .ok()?
        .as_f64()? as u32;

    Some(StripeResult {
        bytes,
        y_offset,
        width,
        height,
        elapsed_ms,
        slot,
    })
}

/// Извлекает ID инстанса из сообщения `{ type: "ready", instance, slot }`.
pub fn extract_ready_instance(event: &MessageEvent) -> Option<(String, u32)> {
    let data = event.data();
    let kind = Reflect::get(&data, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())?;
    if kind != "ready" {
        return None;
    }
    let instance = Reflect::get(&data, &JsValue::from_str("instance"))
        .ok()
        .and_then(|v| v.as_string())?;
    let slot = Reflect::get(&data, &JsValue::from_str("slot"))
        .ok()
        .and_then(|v| v.as_f64())? as u32;
    Some((instance, slot))
}

// =============================================================================
// Вспомогательное: строка для статуса SAB
// =============================================================================
/// Отладочная строка о поддержке (используется в `update_stats`).
pub fn support_summary() -> String {
    format_str(format_args!(
        "Worker: {w} · SAB: {s} · Atomics: {a}",
        w = if is_worker_supported() { "да" } else { "нет" },
        s = if is_shared_array_buffer_supported() { "да" } else { "нет" },
        a = if is_atomics_supported() { "да" } else { "нет" },
    ))
}