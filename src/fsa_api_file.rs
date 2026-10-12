//! FileSystemFileHandle — handle на файл.
//!
//! * `get_file()` — Promise<File>.
//! * `create_writable(options?)` — Promise<FileSystemWritableFileStream>.
//! * `move(name, dir?)` — экспериментальный, вызываем через Reflect.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileSystemDirectoryHandle, FileSystemFileHandle};

fn call_promise(
    handle: &JsValue,
    method: &str,
    args: &[JsValue],
) -> Result<js_sys::Promise, JsValue> {
    let fn_js = js_sys::Reflect::get(handle, &JsValue::from_str(method))?;
    if fn_js.is_undefined() {
        return Err(JsValue::from_str(&format!("{method} недоступен")));
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

/// getFile() → File.
pub async fn get_file(handle: &FileSystemFileHandle) -> Result<File, JsValue> {
    let h: &JsValue = handle.as_ref();
    let promise = call_promise(h, "getFile", &[])?;
    let value = JsFuture::from(promise).await?;
    value
        .dyn_into::<File>()
        .map_err(|_| JsValue::from_str("не File"))
}

/// createWritable() → FileSystemWritableFileStream.
///
/// `keep_existing` = true — дозапись в конец; false (по умолчанию) —
/// усечение файла до нуля перед записью.
pub async fn create_writable(
    handle: &FileSystemFileHandle,
    keep_existing: bool,
) -> Result<JsValue, JsValue> {
    let h: &JsValue = handle.as_ref();
    let promise = if keep_existing {
        let options = js_sys::Object::new();
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("keepExistingData"),
            &JsValue::from_bool(true),
        )?;
        call_promise(h, "createWritable", &[options.into()])?
    } else {
        call_promise(h, "createWritable", &[])?
    };
    JsFuture::from(promise).await
}

/// move(name, directory?) — экспериментальный метод.
///
/// В браузерах без поддержки вернёт Err.
pub async fn move_to(
    handle: &FileSystemFileHandle,
    new_name: &str,
    dir: Option<&FileSystemDirectoryHandle>,
) -> Result<(), JsValue> {
    let h: &JsValue = handle.as_ref();
    let name_js = JsValue::from_str(new_name);
    let promise = match dir {
        Some(d) => {
            let d_js: &JsValue = d.as_ref();
            call_promise(h, "move", &[name_js, d_js.clone()])?
        }
        None => call_promise(h, "move", &[name_js])?,
    };
    let _ = JsFuture::from(promise).await?;
    Ok(())
}