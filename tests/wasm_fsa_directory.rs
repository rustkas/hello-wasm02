//! File System Access API: работа с папками.
//!
//! **Требования:** Chrome/Edge + secure context + user gesture.
//!
//! **Про `values()`.** В web-sys 0.3.105 `FileSystemDirectoryHandle::values()`
//! возвращает `js_sys::AsyncIterator`, а не `Promise`. `JsFuture::from`
//! такой тип не принимает — итерируем вручную через `iter.next()`.
//!
//! **Итеративная очередь вместо рекурсии.** Async-рекурсия в Rust
//! требует `Box::pin` на каждом уровне и может переполнить стек.

use std::collections::VecDeque;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

/// Запись о файле, найденном при обходе директории.
#[derive(Debug, Clone, PartialEq)]
pub struct DirEntry {
    pub path: String,
    pub size: f64,
    pub mime_type: String,
}

/// Открывает диалог выбора директории.
pub async fn pick_directory() -> Result<Option<FileSystemDirectoryHandle>, JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?;
    let window_js: &JsValue = window.as_ref();

    let func_js = js_sys::Reflect::get(
        window_js,
        &JsValue::from_str("showDirectoryPicker"),
    )?;
    if func_js.is_undefined() {
        return Err(JsValue::from_str("showDirectoryPicker недоступен"));
    }
    let func: js_sys::Function = func_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не функция"))?;

    let promise_js = func.call0(window_js)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;

    match JsFuture::from(promise).await {
        Ok(v) => {
            let handle: FileSystemDirectoryHandle = v
                .dyn_into()
                .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))?;
            Ok(Some(handle))
        }
        Err(_) => Ok(None), // пользователь отменил
    }
}

/// Читает один элемент из async iterator директории.
async fn next_entry(iter: &JsValue) -> Result<Option<JsValue>, JsValue> {
    let next_fn_js = js_sys::Reflect::get(iter, &JsValue::from_str("next"))?;
    let next_fn: js_sys::Function = next_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("next не функция"))?;
    let promise_js = next_fn.call0(iter)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("next не Promise"))?;

    let result: JsValue = JsFuture::from(promise).await?;
    let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
        .as_bool()
        .unwrap_or(true);
    if done {
        return Ok(None);
    }
    let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
    return Ok(Some(value));
}

/// Рекурсивно обходит директорию и возвращает список файлов.
///
/// `max_files` — защита от обхода огромных деревьев.
pub async fn scan_directory(
    root: &FileSystemDirectoryHandle,
    max_files: usize,
) -> Result<Vec<DirEntry>, JsValue> {
    let mut out: Vec<DirEntry> = Vec::new();
    let mut queue: VecDeque<(FileSystemDirectoryHandle, String)> = VecDeque::new();
    queue.push_back((root.clone(), String::new()));

    while let Some((dir, prefix)) = queue.pop_front() {
        if out.len() >= max_files {
            break;
        }

        // dir.values() → AsyncIterator
        let dir_js: &JsValue = dir.as_ref();
        let values_fn_js = js_sys::Reflect::get(
            dir_js,
            &JsValue::from_str("values"),
        )?;
        let values_fn: js_sys::Function = values_fn_js
            .dyn_into()
            .map_err(|_| JsValue::from_str("values не функция"))?;
        let iter: JsValue = values_fn.call0(dir_js)?;

        loop {
            if out.len() >= max_files {
                break;
            }
            let entry = match next_entry(&iter).await? {
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
            // блока if/else в этом окружении выводится как `()`.
            let full_path: String;
            if prefix.is_empty() {
                full_path = name.clone();
            } else {
                full_path = format!("{prefix}/{name}");
            }

            if kind == "file" {
                let file_handle: FileSystemFileHandle = entry
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("не FileSystemFileHandle"))?;
                let file_promise: js_sys::Promise = file_handle.get_file();
                let file_value: JsValue = JsFuture::from(file_promise).await?;
                let file: web_sys::File = file_value
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("не File"))?;
                out.push(DirEntry {
                    path: full_path,
                    size: file.size(),
                    mime_type: file.type_(),
                });
            } else if kind == "directory" {
                let child_dir: FileSystemDirectoryHandle = entry
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))?;
                queue.push_back((child_dir, full_path));
            }
        }
    }

    return Ok(out);
}

/// Удаляет файл или поддиректорию из директории.
///
/// `remove_entry` возвращает `Promise` напрямую, не `Result<Promise>` —
/// поэтому `?` на самом вызове не работает, только на `.await`.
pub async fn remove_entry(
    dir: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<(), JsValue> {
    let promise: js_sys::Promise = dir.remove_entry(name);
    let _ = JsFuture::from(promise).await?;
    return Ok(());
}