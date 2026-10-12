//! Web Workers для файловой обработки.
//!
//! Модуль автономен: зависит только от `web-sys`, `js-sys`,
//! `wasm-bindgen`, `wasm-bindgen-futures`.
//!
//! `File` передаётся в воркер через `postMessage` по structured
//! clone. Это дешёвая операция: в браузере `File` — это ссылка
//! на blob-объект, а не копия содержимого.
//!
//! `FileReaderSync` доступен только внутри воркера — в главном
//! потоке его вызов бросает `InvalidStateError`. Поэтому чтение
//! вынесено в отдельный поток: главный остаётся отзывчивым.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, Worker};

/// Результат чтения файла в воркере.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerReadResult {
    /// Имя файла.
    pub name: String,
    /// Размер файла в байтах.
    pub size: f64,
    /// Первые 64 байта в hex-представлении (через пробел).
    pub head_hex: String,
    /// Сколько байт реально прочитано (может быть меньше 64,
    /// если файл короче).
    pub head_len: u32,
}

/// Читает `File` в отдельном Worker и возвращает первые 64 байта.
///
/// Внутри воркера используется `FileReaderSync` — синхронное
/// чтение, доступное только в Worker-контексте. Воркер создаётся
/// на каждый вызов и терминируется после получения ответа.
pub async fn read_file_in_worker(
    file: &web_sys::File,
    worker_url: &str,
) -> Result<WorkerReadResult, JsValue> {
    let worker = Worker::new(worker_url)?;

    // Формируем сообщение: { type: "read", id: 1, file }.
    let msg_obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("type"),
        &JsValue::from_str("read"),
    )?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("id"),
        &JsValue::from_f64(1.0),
    )?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("file"),
        file.as_ref(),
    )?;
    let msg: JsValue = msg_obj.into();

    // Оборачиваем ответ воркера в Promise.
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let reject_c = reject.clone();
        let on_message = Closure::once_into_js(move |event: MessageEvent| {
            let data = event.data();
            let obj: &js_sys::Object = match data.dyn_ref() {
                Some(o) => o,
                None => {
                    let _ = reject_c
                        .call1(&JsValue::NULL, &JsValue::from_str("ответ не Object"));
                    return;
                }
            };
            let type_js = js_sys::Reflect::get(obj, &JsValue::from_str("type"))
                .unwrap_or(JsValue::UNDEFINED);
            let type_str = type_js.as_string().unwrap_or_default();

            if type_str == "result" {
                let _ = resolve_c.call1(&JsValue::NULL, &data);
            } else {
                let err_msg = js_sys::Reflect::get(
                    obj,
                    &JsValue::from_str("message"),
                )
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "worker error".to_string());
                let _ = reject_c.call1(&JsValue::NULL, &JsValue::from_str(&err_msg));
            }
        });
        worker.set_onmessage(Some(on_message.unchecked_ref()));
    });

    worker.post_message(&msg)?;
    let value = JsFuture::from(promise).await;

    // Терминируем воркер — освобождаем ресурсы.
    worker.terminate();

    let value = value?;
    return extract_result(&value);
}

/// Извлекает поля из ответа воркера.
fn extract_result(value: &JsValue) -> Result<WorkerReadResult, JsValue> {
    let obj: &js_sys::Object = value
        .dyn_ref()
        .ok_or_else(|| JsValue::from_str("ответ не Object"))?;

    let name = js_sys::Reflect::get(obj, &JsValue::from_str("name"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("нет поля name"))?;
    let size = js_sys::Reflect::get(obj, &JsValue::from_str("size"))?
        .as_f64()
        .ok_or_else(|| JsValue::from_str("нет поля size"))?;
    let head_hex = js_sys::Reflect::get(obj, &JsValue::from_str("headHex"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("нет поля headHex"))?;
    let head_len_f = js_sys::Reflect::get(obj, &JsValue::from_str("headLen"))?
        .as_f64()
        .ok_or_else(|| JsValue::from_str("нет поля headLen"))?;

    return Ok(WorkerReadResult {
        name,
        size,
        head_hex,
        head_len: head_len_f as u32,
    });
}

/// Считает SHA-256 файла в Web Worker через `crypto.subtle.digest`.
///
/// Воркер читает файл целиком в `ArrayBuffer` и передаёт его
/// в нативный `crypto.subtle.digest("SHA-256", ...)`. Возвращает
/// hex-строку из 64 символов.
///
/// **Ограничение:** работает для файлов, которые влезают
/// в `ArrayBuffer` (до ~2 ГБ в браузере). Для больших используйте
/// потоковое хеширование в главном потоке.
pub async fn sha256_in_worker(
    file: &web_sys::File,
    worker_url: &str,
) -> Result<String, JsValue> {
    let worker = Worker::new(worker_url)?;

    // Сообщение: { type: "hash", id: 1, file }.
    let msg_obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("type"),
        &JsValue::from_str("hash"),
    )?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("id"),
        &JsValue::from_f64(1.0),
    )?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("file"),
        file.as_ref(),
    )?;
    let msg: JsValue = msg_obj.into();

    // Оборачиваем ответ воркера в Promise.
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let reject_c = reject.clone();
        let on_message = Closure::once_into_js(move |event: MessageEvent| {
            let data = event.data();
            let obj: &js_sys::Object = match data.dyn_ref() {
                Some(o) => o,
                None => {
                    let _ = reject_c
                        .call1(&JsValue::NULL, &JsValue::from_str("ответ не Object"));
                    return;
                }
            };
            let type_js = js_sys::Reflect::get(obj, &JsValue::from_str("type"))
                .unwrap_or(JsValue::UNDEFINED);
            let type_str = type_js.as_string().unwrap_or_default();

            if type_str == "hash_result" {
                let _ = resolve_c.call1(&JsValue::NULL, &data);
            } else {
                let err_msg = js_sys::Reflect::get(
                    obj,
                    &JsValue::from_str("message"),
                )
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "worker error".to_string());
                let _ = reject_c.call1(&JsValue::NULL, &JsValue::from_str(&err_msg));
            }
        });
        worker.set_onmessage(Some(on_message.unchecked_ref()));
    });

    worker.post_message(&msg)?;
    let value = JsFuture::from(promise).await;

    // Терминируем воркер.
    worker.terminate();

    let value = value?;
    return extract_hash_result(&value);
}

/// Извлекает hex-строку из ответа воркера типа `hash_result`.
fn extract_hash_result(value: &JsValue) -> Result<String, JsValue> {
    let obj: &js_sys::Object = value
        .dyn_ref()
        .ok_or_else(|| JsValue::from_str("ответ не Object"))?;

    let hex = js_sys::Reflect::get(obj, &JsValue::from_str("hex"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("нет поля hex"))?;

    return Ok(hex);
}

/// Считает SHA-256 куска файла `[start, end)` в Web Worker
/// через `crypto.subtle.digest`.
///
/// Возвращает hex-строку (64 символа). Используется для
/// chunked-хеширования больших файлов, которые не влезают
/// в один `ArrayBuffer`.
pub async fn sha256_chunk_in_worker(
    file: &web_sys::File,
    start: f64,
    end: f64,
    worker_url: &str,
) -> Result<String, JsValue> {
    let worker = Worker::new(worker_url)?;

    let msg_obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("type"),
        &JsValue::from_str("hash_chunk"),
    )?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("id"),
        &JsValue::from_f64(1.0),
    )?;
    js_sys::Reflect::set(&msg_obj, &JsValue::from_str("file"), file.as_ref())?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("start"),
        &JsValue::from_f64(start),
    )?;
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("end"),
        &JsValue::from_f64(end),
    )?;
    let msg: JsValue = msg_obj.into();

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let reject_c = reject.clone();
        let on_message = Closure::once_into_js(move |event: MessageEvent| {
            let data = event.data();
            let obj: &js_sys::Object = match data.dyn_ref() {
                Some(o) => o,
                None => {
                    let _ = reject_c
                        .call1(&JsValue::NULL, &JsValue::from_str("ответ не Object"));
                    return;
                }
            };
            let type_js = js_sys::Reflect::get(obj, &JsValue::from_str("type"))
                .unwrap_or(JsValue::UNDEFINED);
            let type_str = type_js.as_string().unwrap_or_default();

            if type_str == "hash_chunk_result" {
                let _ = resolve_c.call1(&JsValue::NULL, &data);
            } else {
                let err_msg = js_sys::Reflect::get(obj, &JsValue::from_str("message"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "worker error".to_string());
                let _ = reject_c.call1(&JsValue::NULL, &JsValue::from_str(&err_msg));
            }
        });
        worker.set_onmessage(Some(on_message.unchecked_ref()));
    });

    worker.post_message(&msg)?;
    let value = JsFuture::from(promise).await;
    worker.terminate();
    let value = value?;

    let obj: &js_sys::Object = value
        .dyn_ref()
        .ok_or_else(|| JsValue::from_str("ответ не Object"))?;
    let hex = js_sys::Reflect::get(obj, &JsValue::from_str("hex"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("нет поля hex"))?;
    return Ok(hex);
}