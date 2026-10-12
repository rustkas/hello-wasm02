//! FileSystemHandle — базовый тип для FileSystemFileHandle и
//! FileSystemDirectoryHandle. Даёт kind, name, isSameEntry,
//! queryPermission, requestPermission.
//!
//! Все методы вызываются через Reflect::get + Function::call* —
//! это не зависит от того, какие сигнатуры экспортирует web-sys.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

/// Вид handle: "file" | "directory" | "" (если поле отсутствует).
pub fn kind(handle: &JsValue) -> String {
    js_sys::Reflect::get(handle, &JsValue::from_str("kind"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// Имя файла или директории.
pub fn name(handle: &JsValue) -> String {
    js_sys::Reflect::get(handle, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// Вызывает метод через Reflect и возвращает Promise.
fn call_method(handle: &JsValue, method: &str, args: &[JsValue]) -> Result<js_sys::Promise, JsValue> {
    let fn_js = js_sys::Reflect::get(handle, &JsValue::from_str(method))?;
    if fn_js.is_undefined() {
        return Err(JsValue::from_str(&format!("метод {method} недоступен")));
    }
    let func: js_sys::Function = fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{method} не функция")))?;
    let promise_js = match args.len() {
        0 => func.call0(handle)?,
        1 => func.call1(handle, &args[0])?,
        2 => func.call2(handle, &args[0], &args[1])?,
        _ => return Err(JsValue::from_str("слишком много аргументов")),
    };
    promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{method} вернул не Promise")))
}

/// isSameEntry(other) — true, если два handle указывают на один объект.
pub async fn is_same_entry(a: &JsValue, b: &JsValue) -> Result<bool, JsValue> {
    let promise = call_method(a, "isSameEntry", &[b.clone()])?;
    let value = JsFuture::from(promise).await?;
    Ok(value.as_bool().unwrap_or(false))
}

/// Приводит handle к FileSystemFileHandle. Ошибка, если это директория.
pub fn as_file_handle(v: JsValue) -> Result<FileSystemFileHandle, JsValue> {
    let kind_str = kind(&v);
    if kind_str == "directory" {
        return Err(JsValue::from_str("handle указывает на директорию"));
    }
    v.dyn_into::<FileSystemFileHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))
}

/// Приводит handle к FileSystemDirectoryHandle.
pub fn as_directory_handle(v: JsValue) -> Result<FileSystemDirectoryHandle, JsValue> {
    let kind_str = kind(&v);
    if kind_str == "file" {
        return Err(JsValue::from_str("handle указывает на файл"));
    }
    v.dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
}