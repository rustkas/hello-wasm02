//! Обход `FileSystemDirectoryHandle`: итеративный BFS, resolve, идемпотентное
//! удаление.
//!
//! ## Почему BFS, а не рекурсия
//!
//! Рекурсивный обход в async-Rust — это стек промисов. На дереве глубины N
//! вы держите N активных future'ов. Быстро, шумно, растёт линейно. BFS через
//! `VecDeque<(handle, prefix, depth)>` держит очередь плоской: одна future,
//! одно состояние, память O(ширина), а не O(глубина).
//!
//! ## Почему values() — не Promise
//!
//! `dirHandle.values()` возвращает объект с методом `.next()`. Сам объект —
//! не Promise, у него нет `.then()`; передать его в `JsFuture::from` нельзя.
//! Что возвращает `.next()` — уже Promise, разворачивается через `JsFuture`.
//! Результат — `{value, done}`; читаем через `Reflect`.
//!
//! ## Почему removeEntry должен быть идемпотентен
//!
//! `removeEntry(name)` на отсутствующем имени бросает `NotFoundError`. Контракт
//! метода — «удали, если есть» — на этом и строится: `NotFoundError` → `Ok(false)`.
//! Повторное нажатие кнопки безопасно. Это общий принцип: любая
//! «отменить/удалить» операция должна быть идемпотентной.

use std::collections::VecDeque;

use gloo_timers::future::TimeoutFuture;
use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemHandle,
    FileSystemRemoveOptions, FileSystemWritableFileStream,
};

use crate::fsa_choose_options::format_str;

/// Сколько записей обрабатываем между yield'ами.
const YIELD_EVERY: u32 = 100;

// ---------------------------------------------------------------------------
// Модель дерева
// ---------------------------------------------------------------------------

/// Тип записи в дереве.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

/// Одна запись дерева.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeEntry {
    /// Путь относительно корня обхода, компоненты через `/`.
    pub path: String,
    /// Глубина от корня. Корень = 0, его дети = 1, и т.д.
    pub depth: u32,
    pub kind: EntryKind,
    /// Размер файла. `None` для директорий.
    pub size: Option<u64>,
}

/// Результат обхода.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
    pub entries: Vec<TreeEntry>,
    /// `true` — упёрлись в `max_entries` и не всё дерево осмотрено.
    pub truncated: bool,
    pub dirs_visited: u32,
    pub max_entries: usize,
}

// ---------------------------------------------------------------------------
// Внутренние помощники: async-iterator
// ---------------------------------------------------------------------------

/// Получить async-iterator у `dir.values()`.
///
/// `values` на `FileSystemDirectoryHandle` может отсутствовать в текущей
/// версии web-sys — вызываем через `Reflect`. Функция возвращает `JsValue`,
/// потому что тип «async-iterator» в `js_sys` не выделен: только
/// `Iterator` (sync) и `AsyncIterator` (отдельный трейт для async-функций).
/// Универсально: через `Reflect::get(&iter, "next")` + вызов.
fn dir_values_iter(dir: &FileSystemDirectoryHandle) -> Result<JsValue, String> {
    let values_fn: Function = Reflect::get(dir, &JsValue::from_str("values"))
        .map_err(|e| format_str(format_args!("get values: {e:?}")))?
        .dyn_into()
        .map_err(|_| "values — не функция".to_string())?;
    return values_fn
        .call0(dir)
        .map_err(|e| format_str(format_args!("values(): {e:?}")));
}

/// Один шаг async-iterator'а. `None` — итерация завершена.
///
/// `next()` в async-iterator'е возвращает `Promise<{value, done}>`.
/// Разворачиваем: вызов → `JsFuture::from` → читаем `done` и `value`.
async fn iter_next(iter: &JsValue) -> Result<Option<JsValue>, String> {
    let next_fn: Function = Reflect::get(iter, &JsValue::from_str("next"))
        .map_err(|e| format_str(format_args!("get next: {e:?}")))?
        .dyn_into()
        .map_err(|_| "next — не функция".to_string())?;

    let promise: Promise = next_fn
        .call0(iter)
        .map_err(|e| format_str(format_args!("next(): {e:?}")))?
        .dyn_into()
        .map_err(|_| "next() — не Promise".to_string())?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| format_str(format_args!("await next: {e:?}")))?;

    let done: bool = Reflect::get(&result, &JsValue::from_str("done"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if done {
        return Ok(None);
    }

    let value = Reflect::get(&result, &JsValue::from_str("value"))
        .map_err(|e| format_str(format_args!("get value: {e:?}")))?;
    return Ok(Some(value));
}

/// Имя handle'а (`FileSystemHandle.name`).
fn handle_name(handle: &JsValue) -> String {
    return Reflect::get(handle, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
}

/// Тип handle'а (`FileSystemHandle.kind`): `"file"` или `"directory"`.
fn handle_kind(handle: &JsValue) -> String {
    return Reflect::get(handle, &JsValue::from_str("kind"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
}

/// Соединить префикс и имя через `/`. Пустой префикс → только имя.
fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        return name.to_string();
    }
    return format_str(format_args!("{prefix}/{name}"));
}

/// Последний компонент пути. `"a/b/c.txt"` → `"c.txt"`, `"c.txt"` → `"c.txt"`.
fn leaf_name(path: &str) -> &str {
    return match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    };
}

// ---------------------------------------------------------------------------
// Публичный API
// ---------------------------------------------------------------------------

/// Итеративный BFS-обход дерева. Лимит `max_entries` — обязательный.
///
/// Порядок обхода — BFS (в ширину). После обхода записи сортируются
/// лексикографически по пути: это даёт стабильный, предсказуемый порядок,
/// при котором дети идут сразу за родителем, а соседи — рядом.
///
/// Каждые `YIELD_EVERY` записей отдаём управление через `TimeoutFuture`.
/// Без этого на большом дереве UI не успеет перерисоваться: цикл крутится
/// в одной макротаске, а браузер перерисовывает страницу только между ними.
pub async fn scan_tree(
    root: &FileSystemDirectoryHandle,
    max_entries: usize,
) -> Result<ScanResult, String> {
    let mut entries: Vec<TreeEntry> = Vec::new();
    let mut truncated: bool = false;
    let mut dirs_visited: u32 = 0;
    let mut since_yield: u32 = 0;

    // Очередь: (директория, префикс пути, глубина).
    let mut queue: VecDeque<(FileSystemDirectoryHandle, String, u32)> = VecDeque::new();
    queue.push_back((root.clone(), String::new(), 0));

    'outer: while let Some((dir, prefix, depth)) = queue.pop_front() {
        dirs_visited += 1;
        let iter = dir_values_iter(&dir)?;

        loop {
            if entries.len() >= max_entries {
                truncated = true;
                break 'outer;
            }

            let handle: JsValue = match iter_next(&iter).await? {
                Some(v) => v,
                None => break,
            };

            let name: String = handle_name(&handle);
            let path: String = join_path(&prefix, &name);
            let kind: String = handle_kind(&handle);
            let child_depth: u32 = depth + 1;

            if kind == "directory" {
                let dh: FileSystemDirectoryHandle = handle
                    .dyn_into()
                    .map_err(|_| format_str(format_args!("не DirectoryHandle: {path}")))?;
                entries.push(TreeEntry {
                    path: path.clone(),
                    depth: child_depth,
                    kind: EntryKind::Directory,
                    size: None,
                });
                queue.push_back((dh, path, child_depth));
            } else {
                let fh: FileSystemFileHandle = handle
                    .dyn_into()
                    .map_err(|_| format_str(format_args!("не FileHandle: {path}")))?;
                let file_js = JsFuture::from(fh.get_file())
                    .await
                    .map_err(|e| format_str(format_args!("get_file({path}): {e:?}")))?;
                let file: web_sys::File = file_js
                    .dyn_into()
                    .map_err(|_| format_str(format_args!("не File: {path}")))?;
                entries.push(TreeEntry {
                    path,
                    depth: child_depth,
                    kind: EntryKind::File,
                    size: Some(file.size() as u64),
                });
            }

            since_yield += 1;
            if since_yield >= YIELD_EVERY {
                since_yield = 0;
                TimeoutFuture::new(0).await;
            }
        }
    }

    // Стабильный порядок: дети идут сразу за родителем, соседи — рядом.
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    return Ok(ScanResult {
        entries,
        truncated,
        dirs_visited,
        max_entries,
    });
}

/// Путь `child` относительно `ancestor`.
///
/// Возвращает `Some("docs/readme.txt")`, если `ancestor` — предок `child`,
/// или `None`, если нет. `resolve` определён только на директориях, поэтому
/// вызываем через `Reflect` — в web-sys его может не быть.
pub async fn resolve_path(
    ancestor: &FileSystemDirectoryHandle,
    child: &FileSystemHandle,
) -> Result<Option<String>, String> {
    let resolve_fn: Function = Reflect::get(ancestor, &JsValue::from_str("resolve"))
        .map_err(|e| format_str(format_args!("get resolve: {e:?}")))?
        .dyn_into()
        .map_err(|_| "resolve — не функция".to_string())?;

    let promise: Promise = resolve_fn
        .call1(ancestor, child)
        .map_err(|e| format_str(format_args!("resolve(): {e:?}")))?
        .dyn_into()
        .map_err(|_| "resolve() — не Promise".to_string())?;

    let value = JsFuture::from(promise)
        .await
        .map_err(|e| format_str(format_args!("await resolve: {e:?}")))?;

    // `null` — не предок.
    if value.is_null() {
        return Ok(None);
    }

    let arr: Array = value
        .dyn_into()
        .map_err(|_| "resolve() — не массив".to_string())?;

    let mut parts: Vec<String> = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let s: String = arr.get(i).as_string().unwrap_or_default();
        parts.push(s);
    }

    if parts.is_empty() {
        return Ok(None);
    }

    return Ok(Some(parts.join("/")));
}

/// Идемпотентное удаление.
///
/// * `Ok(true)` — файл/директория были и удалены.
/// * `Ok(false)` — имени уже нет (`NotFoundError`).
/// * `Err(msg)` — любая другая ошибка.
///
/// Контракт важен для UI: пользователь нажимает «Удалить» дважды, второй
/// раз должен увидеть «уже отсутствует», а не «ошибка». Так же ведёт себя
/// любой DELETE-эндпоинт в REST: повторный запрос — идемпотентный, не 404.
pub async fn remove_idempotent(
    parent: &FileSystemDirectoryHandle,
    name: &str,
    recursive: bool,
) -> Result<bool, String> {
    let promise: Promise = if recursive {
        let opts = FileSystemRemoveOptions::new();
        opts.set_recursive(true);
        parent.remove_entry_with_options(name, &opts)
    } else {
        parent.remove_entry(name)
    };

    match JsFuture::from(promise).await {
        Ok(_) => return Ok(true),
        Err(e) => {
            let err_name: String = Reflect::get(&e, &JsValue::from_str("name"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            if err_name == "NotFoundError" {
                return Ok(false);
            }
            let msg: String = Reflect::get(&e, &JsValue::from_str("message"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| format_str(format_args!("{e:?}")));
            return Err(msg);
        }
    }
}

// ---------------------------------------------------------------------------
// Помощники для демо: создать папку/файл
// ---------------------------------------------------------------------------

/// Создать (или получить существующую) подпапку.
pub async fn get_or_create_dir(
    parent: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<FileSystemDirectoryHandle, String> {
    let handle = JsFuture::from(parent.get_directory_handle(name))
        .await
        .map_err(|e| format_str(format_args!("get_directory_handle({name}): {e:?}")))?;
    return handle
        .dyn_into()
        .map_err(|_| format_str(format_args!("не DirectoryHandle: {name}")));
}

/// Создать (или перезаписать) файл с содержимым `bytes`.
pub async fn write_file(
    parent: &FileSystemDirectoryHandle,
    name: &str,
    bytes: &[u8],
) -> Result<FileSystemFileHandle, String> {
    let file_js = JsFuture::from(parent.get_file_handle(name))
        .await
        .map_err(|e| format_str(format_args!("get_file_handle({name}): {e:?}")))?;
    let file: FileSystemFileHandle = file_js
        .dyn_into()
        .map_err(|_| format_str(format_args!("не FileHandle: {name}")))?;

    let writable_js = JsFuture::from(file.create_writable())
        .await
        .map_err(|e| format_str(format_args!("create_writable({name}): {e:?}")))?;
    let writable: FileSystemWritableFileStream = writable_js
        .dyn_into()
        .map_err(|_| format_str(format_args!("не WritableFileStream: {name}")))?;

    let write_promise = writable
        .write_with_u8_array(bytes)
        .map_err(|e| format_str(format_args!("write({name}): {e:?}")))?;
    JsFuture::from(write_promise)
        .await
        .map_err(|e| format_str(format_args!("await write({name}): {e:?}")))?;

    JsFuture::from(writable.close())
        .await
        .map_err(|e| format_str(format_args!("close({name}): {e:?}")))?;

    return Ok(file);
}

// ---------------------------------------------------------------------------
// Форматтеры для UI
// ---------------------------------------------------------------------------

pub fn format_size(n: u64) -> String {
    if n < 1024 {
        return format_str(format_args!("{n} Б"));
    }
    if n < 1024 * 1024 {
        let k: f64 = n as f64 / 1024.0;
        return format_str(format_args!("{k:.1} КБ"));
    }
    let m: f64 = n as f64 / (1024.0 * 1024.0);
    return format_str(format_args!("{m:.2} МБ"));
}

/// Человекочитаемый отчёт об обходе. Каждая запись — на своей строке,
/// с отступом по глубине и меткой типа.
pub fn format_scan(result: &ScanResult) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(result.entries.len() + 8);

    let title: String = if result.truncated {
        format_str(format_args!(
            "Дерево (обрезано на {} записях, лимит {})",
            result.entries.len(),
            result.max_entries,
        ))
    } else {
        format_str(format_args!("Дерево ({} записей)", result.entries.len()))
    };
    lines.push(title);
    lines.push(format_str(format_args!(
        "Директорий посещено: {}",
        result.dirs_visited,
    )));
    lines.push(String::new());

    for e in &result.entries {
        let indent: String = "  ".repeat(e.depth as usize);
        let leaf: &str = leaf_name(&e.path);
        match e.kind {
            EntryKind::Directory => {
                let line: String = format_str(format_args!("{indent}[D] {leaf}/"));
                lines.push(line);
            }
            EntryKind::File => {
                let size_s: String = e
                    .size
                    .map(format_size)
                    .unwrap_or_else(|| "?".to_string());
                let line: String = format_str(format_args!("{indent}[F] {leaf}  ({size_s})"));
                lines.push(line);
            }
        }
    }

    if result.truncated {
        lines.push(String::new());
        lines.push("⚠ Список обрезан — в дереве есть ещё записи.".to_string());
    }

    return lines.join("\n");
}