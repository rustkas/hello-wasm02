//! Ядро Worker: создание, двусторонний обмен сообщениями.
//! Автономный модуль — не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, Worker, WorkerOptions};

/// Сообщение от Worker'а к главному потоку.
#[derive(Debug, Clone)]
pub enum WorkerResponse {
    /// Worker загрузил WASM и готов принимать задачи.
    Ready,
    /// Промежуточный прогресс (0..100).
    Progress(u32),
    /// Финальный результат.
    Done(Vec<u8>),
    /// Ошибка.
    Error(String),
}

/// Создаёт классический Worker по URL.
pub fn spawn(url: &str) -> Result<Worker, JsValue> {
    Worker::new(url)
        .map_err(|e| JsValue::from_str(&format!("Worker::new: {e:?}")))
}

/// Создаёт Worker с опциями (например, module worker).
///
/// Для ES-модулей (`wasm-pack build --target web`) обязательно передать
/// `WorkerOptions::new().set_type(WorkerType::Module)`.
pub fn spawn_with_options(url: &str, options: &WorkerOptions) -> Result<Worker, JsValue> {
    Worker::new_with_options(url, options)
        .map_err(|e| JsValue::from_str(&format!("Worker::new_with_options: {e:?}")))
}

/// Отправляет сообщение Worker'у.
pub fn send(worker: &Worker, msg: &JsValue) -> Result<(), JsValue> {
    worker
        .post_message(msg)
        .map_err(|e| JsValue::from_str(&format!("post_message: {e:?}")))
}

/// Отправляет сообщение с Transferable-объектами (без копирования).
/// `transfer` — массив объектов (обычно ArrayBuffer), которые нужно
/// «переместить» в Worker. После передачи исходный буфер становится пустым.
pub fn send_transfer(
    worker: &Worker,
    msg: &JsValue,
    transfer: &js_sys::Array,
) -> Result<(), JsValue> {
    worker
        .post_message_with_transfer(msg, transfer)
        .map_err(|e| JsValue::from_str(&format!("post_message_with_transfer: {e:?}")))
}

/// Парсит MessageEvent в WorkerResponse.
/// Ожидается объект `{type: "ready"|"progress"|"done"|"error", value?: ..., message?: ...}`.
pub fn parse_response(e: &MessageEvent) -> Option<WorkerResponse> {
    let data = e.data();
    let obj = data.dyn_into::<js_sys::Object>().ok()?;
    let type_val = js_sys::Reflect::get(&obj, &JsValue::from_str("type"))
        .ok()?
        .as_string()?;
    match type_val.as_str() {
        "ready" => Some(WorkerResponse::Ready),
        "progress" => {
            let value = js_sys::Reflect::get(&obj, &JsValue::from_str("value"))
                .ok()?
                .as_f64()? as u32;
            Some(WorkerResponse::Progress(value))
        }
        "done" => {
            let value = js_sys::Reflect::get(&obj, &JsValue::from_str("value")).ok()?;
            let array = js_sys::Uint8Array::new(&value).to_vec();
            Some(WorkerResponse::Done(array))
        }
        "error" => {
            let msg = js_sys::Reflect::get(&obj, &JsValue::from_str("message"))
                .ok()?
                .as_string()?;
            Some(WorkerResponse::Error(msg))
        }
        _ => None,
    }
}