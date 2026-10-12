//! 22.2. Dedicated Worker: базовые примитивы.
//!
//! Обёртки вокруг `Worker` и `DedicatedWorkerGlobalScope`:
//!
//! - **feature detection** — поддерживает ли браузер `Worker`;
//! - **создание** — `new Worker(url)` с опциями `type` и `name`;
//! - **отправка сообщений** — `postMessage` со строкой или объектом;
//! - **завершение** — `terminate()`;
//! - **обработка ошибок** — `onerror` и `ErrorEvent`.
//!
//! ## Утилита `bool_of`
//!
//! rust-analyzer теряет тип `matches!` в tail-позиции функции и выдаёт
//! `expected bool, found ()`. Обёртка `bool_of` с явной сигнатурой
//! возвращает типу конкретность.

use js_sys::Reflect;
use wasm_bindgen::prelude::*;

use web_sys::{ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

// =============================================================================
// Утилита для обхода бага rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `matches!` для tail-позиций.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Feature detection
// =============================================================================

/// Глобальный конструктор `Worker` доступен.
pub fn is_worker_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("Worker")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// Создание Worker
// =============================================================================

/// Создаёт `Worker` из URL (классический скрипт).
pub fn create_worker(url: &str) -> Result<Worker, JsValue> {
    Worker::new(url)
}

/// Создаёт `Worker` с опциями: `type` (module) и `name`.
pub fn create_worker_with_options(
    url: &str,
    module: bool,
    name: Option<&str>,
) -> Result<Worker, JsValue> {
    let opts = WorkerOptions::new();
    if module {
        opts.set_type(WorkerType::Module);
    }
    if let Some(n) = name {
        opts.set_name(n);
    }
    Worker::new_with_options(url, &opts)
}

// =============================================================================
// Отправка сообщений
// =============================================================================

/// Отправляет строку Worker'у.
pub fn post_string(worker: &Worker, text: &str) -> Result<(), JsValue> {
    worker.post_message(&JsValue::from_str(text))
}

/// Отправляет объект `{ type, text }`.
pub fn post_echo(worker: &Worker, text: &str) -> Result<(), JsValue> {
    let obj = js_sys::Object::new();
    Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("echo"))?;
    Reflect::set(&obj, &JsValue::from_str("text"), &JsValue::from_str(text))?;
    worker.post_message(&obj)
}

/// Отправляет команду `{ type: "..." }`.
pub fn post_command(worker: &Worker, cmd: &str) -> Result<(), JsValue> {
    let obj = js_sys::Object::new();
    Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(cmd))?;
    worker.post_message(&obj)
}

// =============================================================================
// Завершение
// =============================================================================

/// Принудительно завершает Worker.
pub fn terminate(worker: &Worker) {
    worker.terminate();
}

// =============================================================================
// Извлечение полей из MessageEvent
// =============================================================================

/// Извлекает поле `type` из `MessageEvent.data` как `String`.
pub fn message_type(event: &MessageEvent) -> Option<String> {
    Reflect::get(&event.data(), &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
}

/// Извлекает поле `text` из `MessageEvent.data` как `String`.
pub fn message_text(event: &MessageEvent) -> Option<String> {
    Reflect::get(&event.data(), &JsValue::from_str("text"))
        .ok()
        .and_then(|v| v.as_string())
}

// =============================================================================
// Извлечение полей из ErrorEvent
// =============================================================================

/// Текст ошибки из `ErrorEvent`.
pub fn error_message(event: &ErrorEvent) -> String {
    event.message()
}

/// Имя файла, где произошла ошибка.
pub fn error_filename(event: &ErrorEvent) -> String {
    event.filename()
}

/// Строка ошибки.
pub fn error_lineno(event: &ErrorEvent) -> u32 {
    event.lineno()
}

/// Столбец ошибки.
pub fn error_colno(event: &ErrorEvent) -> u32 {
    event.colno()
}

/// Внутренний `Error` объект, если доступен.
pub fn error_object(event: &ErrorEvent) -> Option<JsValue> {
    let err = event.error();
    if err.is_undefined() || err.is_null() {
        None
    } else {
        Some(err)
    }
}