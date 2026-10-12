//! 20.3. Скачивание файла: три пути.
//!
//! 1. `<a download>`            — программно, без диалога, работает везде.
//! 2. `showSaveFilePicker`      — «Сохранить как…», Chrome/Edge, требует user gesture.
//! 3. `navigator.share({files})`— системный шер, мобильные + Safari desktop.
//!
//! Все три используют **один и тот же Blob/File** — это ядро подглавы:
//! сформировали контент один раз, а отдать пользователю можно по-разному.
//!
//! `web-sys` не экспортирует `showSaveFilePicker` с нужной сигнатурой и часть
//! методов потока (createWritable/write/close) — вызываем через `Reflect` +
//! `Function::call*`. Это универсальный способ дотянуться до любого JS API
//! независимо от версии `web-sys`.

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Blob, BlobPropertyBag, File, FilePropertyBag, HtmlAnchorElement, Url,
};

// ---------- Feature detection ----------

/// `window.showSaveFilePicker` присутствует и является функцией.
pub fn is_save_picker_supported() -> bool {
    let global = js_sys::global();
    match Reflect::get(&global, &JsValue::from_str("showSaveFilePicker")) {
        Ok(v) => v.is_function(),
        Err(_) => false,
    }
}

/// `navigator.share` присутствует и является функцией.
pub fn is_navigator_share_supported() -> bool {
    let global = js_sys::global();
    let Ok(nav) = Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    match Reflect::get(&nav, &JsValue::from_str("share")) {
        Ok(v) => v.is_function(),
        Err(_) => false,
    }
}

/// `navigator.canShare({files:[file]})` — точнее, чем `is_navigator_share_supported`.
/// На десктопном Chrome share есть, а файлы шэрить нельзя — canShare вернёт false.
pub fn can_share_file(file: &File) -> bool {
    let global = js_sys::global();
    let Ok(nav) = Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    let Ok(func) = Reflect::get(&nav, &JsValue::from_str("canShare")) else {
        return false;
    };
    let Ok(func) = func.dyn_into::<Function>() else {
        return false;
    };
    let files = Array::new();
    files.push(file.as_ref());
    let data = Object::new();
    if Reflect::set(&data, &JsValue::from_str("files"), &files).is_err() {
        return false;
    }
    func.call1(&nav, &data)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

// ---------- Создание Blob / File ----------

/// Blob с текстовым содержимым.
pub fn make_text_blob(text: &str, mime: &str) -> Result<Blob, JsValue> {
    let bytes = Uint8Array::from(text.as_bytes());
    let parts = Array::new();
    parts.push(bytes.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type(mime);
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
}

/// File с именем и MIME. `File` в DOM — наследник `Blob`, поэтому его
/// можно передавать и туда, где ожидается `Blob`, и в `navigator.share`.
pub fn make_file(bytes: &[u8], name: &str, mime: &str) -> Result<File, JsValue> {
    let u8arr = Uint8Array::from(bytes);
    let parts = Array::new();
    parts.push(u8arr.as_ref());
    let bag = FilePropertyBag::new();
    bag.set_type(mime);
    File::new_with_u8_array_sequence_and_options(&parts, name, &bag)
}

// ---------- Путь 1: `<a download>` ----------

/// Программное скачивание через невидимый `<a download>`. Диалога нет:
/// файл уходит в папку загрузок (или браузер спрашивает — по своим настройкам).
/// Работает во всех браузерах; user gesture не обязателен, но обычно идёт из клика.
pub fn download_via_anchor(blob: &Blob, filename: &str) -> Result<(), JsValue> {
    let url = Url::create_object_url_with_blob(blob)?;

    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let anchor = document.create_element("a")?.dyn_into::<HtmlAnchorElement>()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display: none;")?;

    let body = document
        .body()
        .ok_or_else(|| JsValue::from_str("no body"))?;
    body.append_child(&anchor)?;
    anchor.click();
    body.remove_child(&anchor)?;

    // Blob уже отдан браузеру — реестр URL можно чистить сразу.
    Url::revoke_object_url(&url)?;
    Ok(())
}

// ---------- Путь 2: `showSaveFilePicker` ----------

/// «Сохранить как…» через File System Access API. Требует user gesture (клика).
///
/// Возвращает:
/// - `Ok(Some(name))` — пользователь выбрал файл, всё записано;
/// - `Ok(None)`       — пользователь отменил диалог (AbortError — не ошибка);
/// - `Err(e)`         — реальная ошибка (не поддержано, нет прав, IO).
pub async fn save_via_picker(
    blob: Blob,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    let global = js_sys::global();
    let func = Reflect::get(&global, &JsValue::from_str("showSaveFilePicker"))?;
    let func: Function = func.dyn_into()?;

    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;

    let promise: js_sys::Promise = func.call1(&JsValue::UNDEFINED, &opts)?.dyn_into()?;
    let handle = match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(h) => h,
        Err(e) if is_abort_error(&e) => return Ok(None),
        Err(e) => return Err(e),
    };

    // createWritable() → Promise<FileSystemWritableFileStream>
    let writable_promise = call_method(&handle, "createWritable", &[])?;
    let writable = wasm_bindgen_futures::JsFuture::from(writable_promise).await?;

    // write(blob) → Promise<void>
    let write_promise = call_method(&writable, "write", &[blob.as_ref()])?;
    wasm_bindgen_futures::JsFuture::from(write_promise).await?;

    // close() → Promise<void>
    let close_promise = call_method(&writable, "close", &[])?;
    wasm_bindgen_futures::JsFuture::from(close_promise).await?;

    // handle.name — имя сохранённого файла.
    let name = Reflect::get(&handle, &JsValue::from_str("name"))?;
    let name_str: String = name
        .as_string()
        .unwrap_or_else(|| suggested_name.to_string());
    Ok(Some(name_str))
}

// ---------- Путь 3: `navigator.share({files})` ----------

/// Системный шер через `navigator.share`. Требует user gesture.
///
/// Возвращает:
/// - `Ok(true)`  — шер прошёл;
/// - `Ok(false)` — пользователь закрыл лист (AbortError);
/// - `Err(e)`    — реальная ошибка (не поддержано, `canShare` дал false, IO).
pub async fn share_via_navigator(file: &File, title: &str) -> Result<bool, JsValue> {
    let global = js_sys::global();
    let nav = Reflect::get(&global, &JsValue::from_str("navigator"))?;
    let share_func = Reflect::get(&nav, &JsValue::from_str("share"))?;
    let share_func: Function = share_func.dyn_into()?;

    let files = Array::new();
    files.push(file.as_ref());
    let data = Object::new();
    Reflect::set(&data, &JsValue::from_str("title"), &JsValue::from_str(title))?;
    Reflect::set(&data, &JsValue::from_str("files"), &files)?;

    let promise: js_sys::Promise = share_func.call1(&nav, &data)?.dyn_into()?;
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(_) => Ok(true),
        Err(e) if is_abort_error(&e) => Ok(false),
        Err(e) => Err(e),
    }
}

// ---------- Утилиты ----------

/// Человекочитаемое сообщение об ошибке.
pub fn js_err_to_string(e: &JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    if let Ok(msg) = Reflect::get(e, &JsValue::from_str("message")) {
        if let Some(s) = msg.as_string() {
            return s;
        }
    }
    if let Ok(name) = Reflect::get(e, &JsValue::from_str("name")) {
        if let Some(s) = name.as_string() {
            let v: String = format!("{s}");
            return v;
        }
    }
    let v: String = format!("{e:?}");
    v
}

/// Вызов метода объекта: `obj[name](...args)` → Promise/JsValue.
/// Ошибка — если метод не функция (правило проекта: сначала проверить is_function).
fn call_method(obj: &JsValue, name: &str, args: &[&JsValue]) -> Result<js_sys::Promise, JsValue> {
    let func_js = Reflect::get(obj, &JsValue::from_str(name))?;
    let func: Function = func_js
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{name} is not a function")))?;
    let args_arr = Array::new();
    for a in args {
        args_arr.push(a);
    }
    let ret = func.apply(obj, &args_arr)?;
    ret.dyn_into::<js_sys::Promise>()
}

/// `err.name === "AbortError"` — пользователь отменил действие.
fn is_abort_error(e: &JsValue) -> bool {
    match Reflect::get(e, &JsValue::from_str("name")) {
        Ok(v) => v.as_string().as_deref() == Some("AbortError"),
        Err(_) => false,
    }
}