//! FileSystemDirectoryHandle — handle на директорию.
//!
//! * getFileHandle(name, {create}) → FileSystemFileHandle
//! * getDirectoryHandle(name, {create}) → FileSystemDirectoryHandle
//! * removeEntry(name, {recursive}) → void
//! * resolve(child) → string[] | null
//! * values() → AsyncIterator (не Promise!)
//!
//! Обход — итеративный, через VecDeque; async-рекурсия в Rust
//! требует Box::pin на каждом уровне.

use std::collections::VecDeque;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

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

/// getFileHandle(name, {create}).
pub async fn get_file_handle(
    dir: &FileSystemDirectoryHandle,
    name: &str,
    create: bool,
) -> Result<FileSystemFileHandle, JsValue> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("create"),
        &JsValue::from_bool(create),
    )?;
    let dir_js: &JsValue = dir.as_ref();
    let promise = call_promise(dir_js, "getFileHandle", &[
        JsValue::from_str(name),
        options.into(),
    ])?;
    let value = JsFuture::from(promise).await?;
    value
        .dyn_into::<FileSystemFileHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))
}

/// getDirectoryHandle(name, {create}).
pub async fn get_directory_handle(
    dir: &FileSystemDirectoryHandle,
    name: &str,
    create: bool,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("create"),
        &JsValue::from_bool(create),
    )?;
    let dir_js: &JsValue = dir.as_ref();
    let promise = call_promise(dir_js, "getDirectoryHandle", &[
        JsValue::from_str(name),
        options.into(),
    ])?;
    let value = JsFuture::from(promise).await?;
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
}

/// removeEntry(name, {recursive}) — идемпотентное удаление.
///
/// Правило проекта: «удалить дважды» — успех, а не 404.
/// Некоторые версии Chrome возвращают `NotFoundError` для
/// отсутствующей записи, некоторые — молча `Ok`. Приводим
/// оба случая к `Ok(())`.
pub async fn remove_entry(
    dir: &FileSystemDirectoryHandle,
    name: &str,
    recursive: bool,
) -> Result<(), JsValue> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("recursive"),
        &JsValue::from_bool(recursive),
    )?;
    let dir_js: &JsValue = dir.as_ref();
    let promise = call_promise(dir_js, "removeEntry", &[
        JsValue::from_str(name),
        options.into(),
    ])?;

    match JsFuture::from(promise).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // NotFoundError → запись уже удалена, это успех (идемпотентность).
            let err_name = js_sys::Reflect::get(&e, &JsValue::from_str("name"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            if err_name == "NotFoundError" {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// resolve(child) → путь относительно root или None, если не найден.
pub async fn resolve_path(
    root: &FileSystemDirectoryHandle,
    child: &JsValue,
) -> Result<Option<String>, JsValue> {
    let root_js: &JsValue = root.as_ref();
    let promise = call_promise(root_js, "resolve", &[child.clone()])?;
    let value = JsFuture::from(promise).await?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let array: js_sys::Array = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("resolve: не массив"))?;
    let mut parts: Vec<String> = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        if let Some(s) = array.get(i).as_string() {
            parts.push(s);
        }
    }
    Ok(Some(parts.join("/")))
}

/// Запись о найденном файле при обходе.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: String,
    pub kind: String,
    pub size: f64,
    pub mime: String,
}

/// Обход директории с лимитом файлов.
///
/// Yield раз в 16 мс — правило проекта: длительная async-операция,
/// обновляющая UI, должна отдавать управление браузеру, иначе
/// прогресс и отмена видны только после завершения.
pub async fn scan(
    root: &FileSystemDirectoryHandle,
    max_files: usize,
    mut on_progress: impl FnMut(usize),
    is_cancelled: impl Fn() -> bool,
) -> Result<Vec<Entry>, JsValue> {
    let mut out: Vec<Entry> = Vec::new();
    let mut queue: VecDeque<(FileSystemDirectoryHandle, String)> = VecDeque::new();
    queue.push_back((root.clone(), String::new()));
    let mut last_yield = 0.0_f64;

    while let Some((dir, prefix)) = queue.pop_front() {
        if is_cancelled() || out.len() >= max_files {
            break;
        }

        let dir_js: &JsValue = dir.as_ref();
        let values_fn_js = js_sys::Reflect::get(dir_js, &JsValue::from_str("values"))?;
        if values_fn_js.is_undefined() {
            continue;
        }
        let values_fn: js_sys::Function = values_fn_js
            .dyn_into()
            .map_err(|_| JsValue::from_str("values не функция"))?;
        let iter: JsValue = values_fn.call0(dir_js)?;

        loop {
            if is_cancelled() || out.len() >= max_files {
                break;
            }
            let entry = match iter_next(&iter).await? {
                Some(v) => v,
                None => break,
            };

            let kind = js_sys::Reflect::get(&entry, &JsValue::from_str("kind"))?
                .as_string()
                .unwrap_or_default();
            let name = js_sys::Reflect::get(&entry, &JsValue::from_str("name"))?
                .as_string()
                .unwrap_or_default();

            // Предобъявленная переменная: `format!` в tail-позиции
            // if/else в этом окружении может выводиться как `()`.
            let full_path: String;
            if prefix.is_empty() {
                full_path = name.clone();
            } else {
                full_path = format!("{prefix}/{name}");
            }

            if kind == "file" {
                let fh: FileSystemFileHandle = entry
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))?;
                let file = crate::fsa_api_file::get_file(&fh).await?;
                out.push(Entry {
                    path: full_path,
                    kind: "file".into(),
                    size: file.size(),
                    mime: file.type_(),
                });
                on_progress(out.len());

                // Yield раз в 16 мс — прогресс и отмена видны в UI.
                let now = js_sys::Date::now();
                if now - last_yield >= 16.0 {
                    TimeoutFuture::new(0).await;
                    last_yield = now;
                }
            } else if kind == "directory" {
                let dh: FileSystemDirectoryHandle = entry
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))?;
                queue.push_back((dh, full_path));
            }
        }
    }
    Ok(out)
}

/// Один шаг async-итератора: iter.next() → Promise → JsFuture.
///
/// Публичный — используется также из `fsa_api_project` при аудите.
pub async fn iter_next(iter: &JsValue) -> Result<Option<JsValue>, JsValue> {
    let next_js = js_sys::Reflect::get(iter, &JsValue::from_str("next"))?;
    let next_fn: js_sys::Function = next_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("next не функция"))?;
    let promise: js_sys::Promise = next_fn
        .call0(iter)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("next не Promise"))?;
    let result: JsValue = JsFuture::from(promise).await?;
    let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
        .as_bool()
        .unwrap_or(true);
    if done {
        return Ok(None);
    }
    Ok(Some(js_sys::Reflect::get(
        &result,
        &JsValue::from_str("value"),
    )?))
}