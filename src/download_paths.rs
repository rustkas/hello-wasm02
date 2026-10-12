//! 20.3. Скачивание файла: три пути.
//!
//! 1. `<a download>`            — программно, без диалога, работает везде.
//! 2. `showSaveFilePicker`      — «Сохранить как…», Chrome/Edge, требует user gesture.
//! 3. `navigator.share({files})`— системный шер, мобильные + Safari desktop.
//!
//! Все три используют один и тот же Blob/File. В `web-sys` часть методов
//! отсутствует с нужной сигнатурой — вызываем через `Reflect` + `Function::call*`.

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, File, FilePropertyBag, HtmlAnchorElement, Url};

// ---------- Вспомогательные функции ----------

/// Возвращает `true`, если глобальный объект имеет свойство-функцию `name`.
///
/// Решает сразу две задачи:
/// 1. Обходит баг rust-analyzer: `matches!` в tail-позиции функции теряет
///    тип и даёт `expected bool, found ()`. Явная сигнатура `-> bool`
///    фиксирует тип на границе функции.
/// 2. Убирает дублирование `Reflect::get` + `is_function` в каждой
///    feature-detection.
fn global_has_function(name: &str) -> bool {
    let global = js_sys::global();
    let Ok(v) = Reflect::get(&global, &JsValue::from_str(name)) else {
        return false;
    };
    v.is_function()
}

/// То же, но для вложенного свойства: `global[obj][prop]`.
/// Например, `global_has_nested_function("navigator", "share")`.
fn global_has_nested_function(obj: &str, prop: &str) -> bool {
    let global = js_sys::global();
    let Ok(o) = Reflect::get(&global, &JsValue::from_str(obj)) else {
        return false;
    };
    let Ok(f) = Reflect::get(&o, &JsValue::from_str(prop)) else {
        return false;
    };
    f.is_function()
}

/// Достаёт вложенную функцию `obj[name]` из глобального объекта.
/// Возвращает `None`, если свойства нет или оно не функция.
/// Универсальный способ дотянуться до API, которое `web-sys` не экспортирует.
fn global_get_function(obj: &str, name: &str) -> Option<Function> {
    let global = js_sys::global();
    let o = Reflect::get(&global, &JsValue::from_str(obj)).ok()?;
    let f = Reflect::get(&o, &JsValue::from_str(name)).ok()?;
    f.dyn_into::<Function>().ok()
}

// ---------- Feature detection ----------

pub fn is_save_picker_supported() -> bool {
    global_has_function("showSaveFilePicker")
}

pub fn is_navigator_share_supported() -> bool {
    global_has_nested_function("navigator", "share")
}

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
    let result: bool = func
        .call1(&nav, &data)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    result
}

// ---------- Создание Blob / File ----------

pub fn make_text_blob(text: &str, mime: &str) -> Result<Blob, JsValue> {
    let bytes = Uint8Array::from(text.as_bytes());
    let parts = Array::new();
    parts.push(bytes.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type(mime);
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
}

pub fn make_file(bytes: &[u8], name: &str, mime: &str) -> Result<File, JsValue> {
    let u8arr = Uint8Array::from(bytes);
    let parts = Array::new();
    parts.push(u8arr.as_ref());
    let bag = FilePropertyBag::new();
    bag.set_type(mime);
    File::new_with_u8_array_sequence_and_options(&parts, name, &bag)
}

// ---------- Путь 1: `<a download>` ----------

/// Создать blob URL. Вызывающая сторона сама решает, когда revoke.
/// Разделение с `trigger_anchor_download` позволяет держать URL живым
/// для кликабельной ссылки в UI.
pub fn create_blob_url(blob: &Blob) -> Result<String, JsValue> {
    Url::create_object_url_with_blob(blob)
}

/// Программный клик по невидимому `<a download href=url>`. URL должен быть жив.
pub fn trigger_anchor_download(url: &str, filename: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let anchor = document
        .create_element("a")?
        .dyn_into::<HtmlAnchorElement>()?;
    anchor.set_href(url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display: none;")?;

    let body = document.body().ok_or_else(|| JsValue::from_str("no body"))?;
    body.append_child(&anchor)?;
    anchor.click();
    body.remove_child(&anchor)?;
    Ok(())
}

/// Комбинированный путь: создать URL → кликнуть → revoke.
pub fn download_via_anchor(blob: &Blob, filename: &str) -> Result<(), JsValue> {
    let url = create_blob_url(blob)?;
    let result = trigger_anchor_download(&url, filename);
    let _ = Url::revoke_object_url(&url);
    result
}

// ---------- Путь 2: `showSaveFilePicker` ----------

/// «Сохранить как…». Возвращает:
/// - `Ok(Some(name))` — файл записан;
/// - `Ok(None)` — пользователь отменил (AbortError);
/// - `Err(msg)` — реальная ошибка или API недоступно.
pub async fn save_via_picker(
    blob: Blob,
    suggested_name: &str,
) -> Result<Option<String>, JsValue> {
    let Some(picker) = global_get_function("window", "showSaveFilePicker")
        .or_else(|| global_get_function("", "showSaveFilePicker"))
    else {
        return Err(JsValue::from_str(
            "showSaveFilePicker не поддерживается этим браузером (Firefox/Safari)",
        ));
    };

    let window = js_sys::global();
    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;

    // this = window (важно для DOM-метода).
    let promise: js_sys::Promise = picker.call1(&window, &opts)?.dyn_into()?;

    let handle = match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(h) => h,
        Err(e) if is_abort_error(&e) => return Ok(None),
        Err(e) => return Err(clarify_picker_error(e)),
    };

    let writable_promise = call_method(&handle, "createWritable", &[])?;
    let writable = wasm_bindgen_futures::JsFuture::from(writable_promise).await?;

    let write_promise = call_method(&writable, "write", &[blob.as_ref()])?;
    wasm_bindgen_futures::JsFuture::from(write_promise).await?;

    let close_promise = call_method(&writable, "close", &[])?;
    wasm_bindgen_futures::JsFuture::from(close_promise).await?;

    let name = Reflect::get(&handle, &JsValue::from_str("name"))?;
    let name_str: String = name
        .as_string()
        .unwrap_or_else(|| suggested_name.to_string());
    Ok(Some(name_str))
}

/// Если браузер вернул `undefined` (потеря user gesture, фокус и т. п.) —
/// даём понятное сообщение вместо «JsValue(undefined)».
fn clarify_picker_error(e: JsValue) -> JsValue {
    if e.is_undefined() || e.is_null() {
        return JsValue::from_str(
            "диалог не открылся: браузер требует user gesture. Кликните кнопку ещё раз \
             и не переключайте окно во время показа диалога",
        );
    }
    e
}

// ---------- Путь 3: `navigator.share({files})` ----------

pub async fn share_via_navigator(file: &File, title: &str) -> Result<bool, JsValue> {
    if !is_navigator_share_supported() {
        return Err(JsValue::from_str(
            "navigator.share отсутствует в этом браузере",
        ));
    }
    if !can_share_file(file) {
        return Err(JsValue::from_str(
            "navigator.share есть, но этот браузер не поддерживает шаринг файлов \
             (типично для desktop Chrome). Откройте на мобильном или в Safari",
        ));
    }

    let Some(share_func) = global_get_function("navigator", "share") else {
        return Err(JsValue::from_str("navigator.share недоступен"));
    };

    let global = js_sys::global();
    let nav = Reflect::get(&global, &JsValue::from_str("navigator"))?;

    let files = Array::new();
    files.push(file.as_ref());
    let data = Object::new();
    Reflect::set(&data, &JsValue::from_str("title"), &JsValue::from_str(title))?;
    Reflect::set(&data, &JsValue::from_str("files"), &files)?;

    let promise: js_sys::Promise = share_func.call1(&nav, &data)?.dyn_into()?;
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(_) => Ok(true),
        Err(e) if is_abort_error(&e) => Ok(false),
        Err(e) if e.is_undefined() || e.is_null() => Err(JsValue::from_str(
            "системный шер не открылся: браузер требует user gesture \
             или не поддерживает шаринг файлов",
        )),
        Err(e) => Err(e),
    }
}

// ---------- Утилиты ----------

pub fn js_err_to_string(e: &JsValue) -> String {
    if e.is_undefined() {
        return String::from("(браузер вернул undefined)");
    }
    if e.is_null() {
        return String::from("(null)");
    }
    if let Some(s) = e.as_string() {
        return s;
    }
    let name = Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string());
    let message = Reflect::get(e, &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string());
    match (name, message) {
        (Some(n), Some(m)) => {
            let s: String = format!("{n}: {m}");
            s
        }
        (Some(n), None) => n,
        (None, Some(m)) => m,
        (None, None) => {
            let s: String = format!("{e:?}");
            s
        }
    }
}

fn call_method(obj: &JsValue, name: &str, args: &[&JsValue]) -> Result<js_sys::Promise, JsValue> {
    let func_js = Reflect::get(obj, &JsValue::from_str(name))?;
    if !func_js.is_function() {
        return Err(JsValue::from_str(&format!("{name} is not a function")));
    }
    let func: Function = func_js.dyn_into()?;
    let args_arr = Array::new();
    for a in args {
        args_arr.push(a);
    }
    let ret = func.apply(obj, &args_arr)?;
    ret.dyn_into::<js_sys::Promise>()
}

fn is_abort_error(e: &JsValue) -> bool {
    match Reflect::get(e, &JsValue::from_str("name")) {
        Ok(v) => v.as_string().as_deref() == Some("AbortError"),
        Err(_) => false,
    }
}