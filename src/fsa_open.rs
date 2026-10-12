//! File System Access API: открытие файла.
//!
//! **Требования:** Chrome/Edge + secure context + user gesture.
//! В Firefox и Safari модуль не используется — там fallback
//! на `<input type="file">`.
//!
//! **Про вызов пикера.** web-sys 0.3.105 экспортирует
//! `Window::show_open_file_picker()` **без аргументов** — опции
//! передать нельзя. Чтобы использовать фильтры, вызываем
//! `showOpenFilePicker` через `Reflect::get` + `Function::call1`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::FileSystemFileHandle;

/// Вызывает `showOpenFilePicker` с опциями (или без).
///
/// Возвращает `Ok(None)`, если пользователь отменил диалог.
async fn call_open_picker(
    options: Option<&JsValue>,
) -> Result<Option<JsValue>, JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?;
    let window_js: &JsValue = window.as_ref();

    // Получаем функцию showOpenFilePicker через Reflect — работает
    // независимо от того, какие сигнатуры экспортирует web-sys.
    let func_js = js_sys::Reflect::get(
        window_js,
        &JsValue::from_str("showOpenFilePicker"),
    )?;
    if func_js.is_undefined() {
        return Err(JsValue::from_str("showOpenFilePicker недоступен"));
    }
    let func: js_sys::Function = func_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не функция"))?;

    let promise_js = match options {
        Some(opts) => func.call1(window_js, opts)?,
        None => func.call0(window_js)?,
    };
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;

    match JsFuture::from(promise).await {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None), // пользователь отменил
    }
}

/// Извлекает первый `FileSystemFileHandle` из массива и читает файл.
async fn first_handle_to_file(handles: JsValue) -> Result<Option<web_sys::File>, JsValue> {
    let array: js_sys::Array = handles
        .dyn_into()
        .map_err(|_| JsValue::from_str("ответ не Array"))?;
    if array.length() == 0 {
        return Ok(None);
    }

    let handle: FileSystemFileHandle = array
        .get(0)
        .dyn_into()
        .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))?;

    let file_promise: js_sys::Promise = handle.get_file();
    let file_value: JsValue = JsFuture::from(file_promise).await?;
    let file: web_sys::File = file_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не File"))?;
    return Ok(Some(file));
}

/// Открывает диалог выбора одного файла.
pub async fn pick_one_file() -> Result<Option<web_sys::File>, JsValue> {
    let handles = match call_open_picker(None).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    return first_handle_to_file(handles).await;
}

/// Открывает диалог выбора файла с фильтром по расширениям.
///
/// `accept` — `{ "text/plain": [".txt"] }` как `JsValue`.
pub async fn pick_one_file_with_filter(
    accept: &JsValue,
) -> Result<Option<web_sys::File>, JsValue> {
    let type_entry = js_sys::Object::new();
    js_sys::Reflect::set(
        &type_entry,
        &JsValue::from_str("description"),
        &JsValue::from_str("Files"),
    )?;
    js_sys::Reflect::set(&type_entry, &JsValue::from_str("accept"), accept)?;

    let types = js_sys::Array::new();
    types.push(&type_entry);

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &JsValue::from_str("types"), &types)?;
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("multiple"),
        &JsValue::from_bool(false),
    )?;

    let handles = match call_open_picker(Some(&options)).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    return first_handle_to_file(handles).await;
}

/// Проверяет, поддерживается ли File System Access API в браузере.
pub fn is_supported() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let window_js: &JsValue = window.as_ref();
    return js_sys::Reflect::get(
        window_js,
        &JsValue::from_str("showOpenFilePicker"),
    )
    .map(|v| !v.is_undefined())
    .unwrap_or(false);
}