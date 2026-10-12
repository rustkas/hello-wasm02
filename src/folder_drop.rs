//! Drag & Drop папок через webkitGetAsEntry.
//!
//! Модуль автономен: не зависит от других модулей проекта.
//!
//! Работает через устаревший File and Directory Entries API:
//! `FileSystemEntry` → `FileSystemDirectoryEntry` → `createReader()`
//! → `readEntries()` (повторять, пока не вернётся пустой массив).
//!
//! Рекурсивный обход реализован через `Box::pin` — рекурсивная
//! `async fn` в Rust требует явной косвенности, иначе future
//! получается бесконечно большого размера.

use std::future::Future;
use std::pin::Pin;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    DataTransferItem, FileSystemDirectoryEntry, FileSystemDirectoryReader,
    FileSystemEntry, FileSystemFileEntry,
};

/// Информация о файле, найденном при рекурсивном обходе папки.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderEntry {
    /// Полный путь от корня перетащенной папки, например
    /// `photos/2024/img001.png`.
    pub path: String,
    /// Размер файла в байтах.
    pub size: f64,
    /// MIME-тип (может быть пустой строкой).
    pub mime_type: String,
}

/// Рекурсивно обходит `FileSystemEntry` и собирает все файлы.
///
/// Возвращает `Pin<Box<dyn Future<...>>>` вместо `async fn` —
/// это обязательное условие для рекурсивных асинхронных функций:
/// без бокса компилятор не может вычислить размер future.
///
/// Параметры берутся по владению, чтобы future был `'static` и
/// не зависел от времени жизни вызывающего кода.
///
/// * Для файла (`FileSystemFileEntry`) — получает `File` через
///   `file()` и добавляет запись в результат.
/// * Для директории (`FileSystemDirectoryEntry`) — вызывает
///   `readEntries()` в цикле (пока не вернётся пустой массив),
///   затем рекурсивно обходит каждый элемент.
///
/// `base_path` — накопленный путь от корня; для корневого
/// вызова передаётся пустая строка.
pub fn scan_entry(
    entry: FileSystemEntry,
    base_path: String,
) -> Pin<Box<dyn Future<Output = Vec<FolderEntry>>>> {
    return Box::pin(async move {
        let mut results = Vec::new();
        let name = entry.name();

        let full_path: String;
        if base_path.is_empty() {
            full_path = name;
        } else {
            full_path = format!("{base_path}/{name}");
        }

        if entry.is_file() {
            if let Ok(file_entry) = entry.dyn_into::<FileSystemFileEntry>() {
                if let Some(record) = read_file_entry(&file_entry, &full_path).await {
                    results.push(record);
                }
            }
        } else if entry.is_directory() {
            if let Ok(dir_entry) = entry.dyn_into::<FileSystemDirectoryEntry>() {
                let entries = read_directory_entries(&dir_entry).await;
                for child in entries {
                    let sub = scan_entry(child, full_path.clone()).await;
                    results.extend(sub);
                }
            }
        }

        return results;
    });
}

/// Читает `FileSystemFileEntry` и превращает его в `FolderEntry`.
async fn read_file_entry(entry: &FileSystemFileEntry, path: &str) -> Option<FolderEntry> {
    let file = file_from_entry(entry).await?;
    return Some(FolderEntry {
        path: path.to_string(),
        size: file.size(),
        mime_type: file.type_(),
    });
}

/// Обёртка над `FileSystemFileEntry::file()` — возвращает `File` через Promise.
async fn file_from_entry(entry: &FileSystemFileEntry) -> Option<web_sys::File> {
    let entry_clone = entry.clone();
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let on_success = Closure::once_into_js(move |f: web_sys::File| {
            let _ = resolve_c.call1(&JsValue::NULL, &f);
        });
        let on_error = Closure::once_into_js(move |err: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &err);
        });
        let _ = entry_clone.file_with_callback_and_callback(
            on_success.unchecked_ref(),
            on_error.unchecked_ref(),
        );
    });
    let value = JsFuture::from(promise).await.ok()?;
    return value.dyn_into::<web_sys::File>().ok();
}

/// Читает все элементы директории, повторяя `readEntries()`,
/// пока не вернётся пустой массив.
async fn read_directory_entries(dir: &FileSystemDirectoryEntry) -> Vec<FileSystemEntry> {
    let reader = dir.create_reader();
    let mut all: Vec<FileSystemEntry> = Vec::new();

    loop {
        let batch = read_entries_once(&reader).await;
        if batch.is_empty() {
            break;
        }
        all.extend(batch);
    }
    return all;
}

/// Один вызов `readEntries()` — возвращает пакет записей.
///
/// Согласно спецификации, `readEntries` возвращает не все записи
/// сразу, а порциями; нужно вызывать повторно до пустого массива.
async fn read_entries_once(reader: &FileSystemDirectoryReader) -> Vec<FileSystemEntry> {
    let reader_clone = reader.clone();
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let on_success = Closure::once_into_js(move |entries: js_sys::Array| {
            let _ = resolve_c.call1(&JsValue::NULL, &entries);
        });
        let on_error = Closure::once_into_js(move |err: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &err);
        });
        let _ = reader_clone.read_entries_with_callback_and_callback(
            on_success.unchecked_ref(),
            on_error.unchecked_ref(),
        );
    });

    let value = match JsFuture::from(promise).await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let array = match value.dyn_into::<js_sys::Array>() {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        if let Ok(entry) = array.get(i).dyn_into::<FileSystemEntry>() {
            entries.push(entry);
        }
    }
    return entries;
}

/// Извлекает `FileSystemEntry` из `DataTransferItem`.
///
/// Возвращает `None`, если элемент не является файлом или
/// директорией (например, перетащили текст или ссылку).
pub fn entry_from_item(item: &DataTransferItem) -> Option<FileSystemEntry> {
    return item.webkit_get_as_entry().ok().flatten();
}