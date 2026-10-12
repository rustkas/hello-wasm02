//! Три picker'а FSA: open, save, directory.
//! Все вызовы через Reflect — сигнатуры web-sys различаются
//! между минорными версиями.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

/// Проверка: доступен ли FSA в этом браузере.
pub fn is_supported() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let w: &JsValue = window.as_ref();
    js_sys::Reflect::get(w, &JsValue::from_str("showOpenFilePicker"))
        .map(|v| !v.is_undefined())
        .unwrap_or(false)
}

async fn call_window_picker(
    method: &str,
    options: Option<JsValue>,
) -> Result<Option<JsValue>, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let w: &JsValue = window.as_ref();
    let fn_js = js_sys::Reflect::get(w, &JsValue::from_str(method))?;
    if fn_js.is_undefined() {
        return Err(JsValue::from_str(&format!("{method} недоступен")));
    }
    let func: js_sys::Function = fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не функция"))?;
    let promise_js = match options {
        Some(opts) => func.call1(w, &opts)?,
        None => func.call0(w)?,
    };
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    match JsFuture::from(promise).await {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None), // отмена
    }
}

/// showOpenFilePicker — возвращает список handle'ов.
pub async fn open_files(
    options: Option<JsValue>,
) -> Result<Option<Vec<FileSystemFileHandle>>, JsValue> {
    let value = match call_window_picker("showOpenFilePicker", options).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    let array: js_sys::Array = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("open_files: не массив"))?;
    let mut out: Vec<FileSystemFileHandle> = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let fh: FileSystemFileHandle = array
            .get(i)
            .dyn_into()
            .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))?;
        out.push(fh);
    }
    Ok(Some(out))
}

/// showSaveFilePicker — возвращает один handle.
pub async fn save_file(
    suggested_name: &str,
) -> Result<Option<FileSystemFileHandle>, JsValue> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;
    let value = match call_window_picker("showSaveFilePicker", Some(options.into())).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    value
        .dyn_into::<FileSystemFileHandle>()
        .map(Some)
        .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))
}

/// showDirectoryPicker — возвращает handle директории.
///
/// `mode`: "read" (по умолчанию) или "readwrite".
pub async fn pick_directory(
    mode: &str,
) -> Result<Option<FileSystemDirectoryHandle>, JsValue> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("mode"),
        &JsValue::from_str(mode),
    )?;
    let value = match call_window_picker("showDirectoryPicker", Some(options.into())).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map(Some)
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
}

/// Готовый объект `types` для фильтра по расширениям.
pub fn build_types(entries: &[(&str, &[&str])]) -> Result<JsValue, JsValue> {
    let types = js_sys::Array::new();
    for (description, extensions) in entries {
        let ext_arr = js_sys::Array::new();
        for ext in extensions.iter() {
            ext_arr.push(&JsValue::from_str(ext));
        }
        let accept = js_sys::Object::new();
        js_sys::Reflect::set(
            &accept,
            &JsValue::from_str(description),
            &ext_arr,
        )?;
        let entry = js_sys::Object::new();
        js_sys::Reflect::set(
            &entry,
            &JsValue::from_str("description"),
            &JsValue::from_str(description),
        )?;
        js_sys::Reflect::set(&entry, &JsValue::from_str("accept"), &accept)?;
        types.push(&entry);
    }
    Ok(types.into())
}