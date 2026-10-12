//! Иерархическое дерево директории и навигация по путям.
//!
//! Отличие от `fsa_api_directory::scan`:
//! * `scan` даёт **плоский** список `Entry` — удобно для прогресса
//!   и подсчётов.
//! * `build_tree` даёт **вложенную** структуру `DirInfo` — удобно
//!   для рендера, рекурсивных подсчётов и diff.
//!
//! Дополнительно модуль закрывает:
//! * `find_by_path` — handle по относительному пути.
//! * `ensure_path` — идемпотентно создать цепочку директорий.
//! * `open_dir` — открыть директорию по пути без создания.
//! * `remove_path` — идемпотентное удаление по пути.
//! * `path_of` — относительный путь handle относительно корня.
//!
//! Вся работа со строками вынесена в модуль `path_str`. Правило:
//! `format!` не должен стоять в tail-позиции `if/else` — на WASM
//! компилятор может вывести такое выражение как `()` (E0308).
//! Поэтому в каждой ветке — явная `let s: String = ...; s`.

use std::collections::HashMap;
use std::collections::VecDeque;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

use crate::fsa_api_directory::{
    get_directory_handle, get_file_handle, iter_next, remove_entry, resolve_path,
};
use crate::fsa_api_file::get_file;

// ===========================================================================
// path_str — вся работа со строками путей в одном месте
// ---------------------------------------------------------------------------
// Здесь и только здесь вызывается `format!` для склейки путей.
// Возврат — всегда через явную переменную с типом, никаких
// `if/else` в tail-позиции, отдающих `String`.
// ===========================================================================

mod path_str {
    /// Склеить префикс и имя. Пустой префикс → только имя.
    pub fn join(prefix: &str, name: &str) -> String {
        if prefix.is_empty() {
            let s: String = name.to_string();
            s
        } else {
            let s: String = format!("{prefix}/{name}");
            s
        }
    }

    /// Разбить путь на непустые сегменты.
    pub fn segments(path: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for part in path.split('/') {
            if !part.is_empty() {
                let s: String = part.to_string();
                out.push(s);
            }
        }
        out
    }

    /// Разбить путь на (родитель, имя).
    /// Для пути без `/` родитель — пустая строка.
    pub fn split_parent(path: &str) -> (String, String) {
        match path.rfind('/') {
            Some(i) => {
                let parent: String = path[..i].to_string();
                let name: String = path[i + 1..].to_string();
                (parent, name)
            }
            None => {
                let parent: String = String::new();
                let name: String = path.to_string();
                (parent, name)
            }
        }
    }
}

// ===========================================================================
// Модель дерева
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    File(FileInfo),
    Dir(DirInfo),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub name: String,
    pub size: f64,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirInfo {
    pub name: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::File(f) => f.name.as_str(),
            Node::Dir(d) => d.name.as_str(),
        }
    }

    pub fn is_dir(&self) -> bool {
        let is: bool = matches!(self, Node::Dir(_));
        is
    }
}

impl DirInfo {
    /// Суммарное число файлов во всём поддереве.
    pub fn total_files(&self) -> usize {
        let mut n: usize = 0;
        for c in &self.children {
            match c {
                Node::File(_) => {
                    n += 1;
                }
                Node::Dir(d) => {
                    n += d.total_files();
                }
            }
        }
        n
    }

    /// Суммарный размер (байты) всех файлов во всём поддереве.
    pub fn total_bytes(&self) -> f64 {
        let mut n: f64 = 0.0;
        for c in &self.children {
            match c {
                Node::File(f) => {
                    n += f.size;
                }
                Node::Dir(d) => {
                    n += d.total_bytes();
                }
            }
        }
        n
    }

    /// Рекурсивная визуализация дерева — удобно для логов и тестов.
    pub fn render(&self) -> String {
        let mut out: String = String::new();
        out.push_str(&self.name);
        out.push_str("/\n");
        render_children(&self.children, 1, &mut out);
        out
    }
}

fn render_children(children: &[Node], depth: usize, out: &mut String) {
    let indent: String = "  ".repeat(depth);
    for c in children {
        match c {
            Node::File(f) => {
                out.push_str(&indent);
                out.push_str(&f.name);
                let suffix: String = format!(" ({} B)\n", f.size as u64);
                out.push_str(&suffix);
            }
            Node::Dir(d) => {
                out.push_str(&indent);
                out.push_str(&d.name);
                out.push_str("/\n");
                render_children(&d.children, depth + 1, out);
            }
        }
    }
}

// ===========================================================================
// Опции обхода
// ===========================================================================

#[derive(Debug, Clone)]
pub struct TreeOptions {
    /// Максимальная глубина (0 — только корень).
    pub max_depth: usize,
    /// Максимум файлов во всём дереве.
    pub max_files: usize,
    /// Имена директорий, которые нужно пропустить.
    pub skip_dirs: Vec<String>,
}

impl Default for TreeOptions {
    fn default() -> Self {
        let skip: Vec<String> = vec![
            ".git".to_string(),
            "node_modules".to_string(),
            "target".to_string(),
        ];
        Self {
            max_depth: usize::MAX,
            max_files: 10_000,
            skip_dirs: skip,
        }
    }
}

// ===========================================================================
// Построение дерева
// ===========================================================================

enum Kind {
    File,
    Dir,
}

struct FlatEntry {
    parent_path: String,
    name: String,
    kind: Kind,
    size: f64,
    mime: String,
}

/// Построить дерево директории.
///
/// Обход — итеративный, через `VecDeque` (как в `scan`).
/// Сборка дерева — синхронная рекурсия по уже собранному плоскому
/// списку: глубина ограничена, `Box::pin` не нужен.
pub async fn build_tree(
    root: &FileSystemDirectoryHandle,
    opts: &TreeOptions,
    mut on_progress: impl FnMut(usize),
    is_cancelled: impl Fn() -> bool,
) -> Result<DirInfo, JsValue> {
    let mut flat: Vec<FlatEntry> = Vec::new();
    let mut queue: VecDeque<(FileSystemDirectoryHandle, String, usize)> = VecDeque::new();
    queue.push_back((root.clone(), String::new(), 0));
    let mut file_count: usize = 0;

    while let Some((dir, prefix, depth)) = queue.pop_front() {
        if is_cancelled() || file_count >= opts.max_files {
            break;
        }
        if depth >= opts.max_depth {
            continue;
        }

        let dir_js: &JsValue = dir.as_ref();
        let values_fn_js = js_sys::Reflect::get(dir_js, &JsValue::from_str("values"))?;
        if values_fn_js.is_undefined() {
            continue;
        }
        let values_fn: js_sys::Function = match values_fn_js.dyn_into() {
            Ok(f) => f,
            Err(_) => continue,
        };
        let iter: JsValue = values_fn.call0(dir_js)?;

        loop {
            if is_cancelled() || file_count >= opts.max_files {
                break;
            }
            let entry: JsValue = match iter_next(&iter).await? {
                Some(v) => v,
                None => break,
            };

            let kind: String = js_sys::Reflect::get(&entry, &JsValue::from_str("kind"))?
                .as_string()
                .unwrap_or_default();
            let name: String = js_sys::Reflect::get(&entry, &JsValue::from_str("name"))?
                .as_string()
                .unwrap_or_default();

            if kind == "file" {
                let fh: FileSystemFileHandle = match entry.dyn_into() {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                let file = get_file(&fh).await?;
                let e = FlatEntry {
                    parent_path: prefix.clone(),
                    name,
                    kind: Kind::File,
                    size: file.size(),
                    mime: file.type_(),
                };
                flat.push(e);
                file_count += 1;
                on_progress(file_count);
            } else if kind == "directory" {
                let mut skip: bool = false;
                for s in &opts.skip_dirs {
                    if s == &name {
                        skip = true;
                        break;
                    }
                }
                if skip {
                    continue;
                }
                let dh: FileSystemDirectoryHandle = match entry.dyn_into() {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                let child_path: String = path_str::join(&prefix, &name);
                let e = FlatEntry {
                    parent_path: prefix.clone(),
                    name: name.clone(),
                    kind: Kind::Dir,
                    size: 0.0,
                    mime: String::new(),
                };
                flat.push(e);
                queue.push_back((dh, child_path, depth + 1));
            }
        }
    }

    let tree_name: String = root.name();
    Ok(assemble(tree_name, flat))
}

fn assemble(root_name: String, flat: Vec<FlatEntry>) -> DirInfo {
    let mut by_parent: HashMap<String, Vec<FlatEntry>> = HashMap::new();
    for e in flat {
        by_parent.entry(e.parent_path.clone()).or_default().push(e);
    }

    fn build_dir(
        name: String,
        path: String,
        by_parent: &mut HashMap<String, Vec<FlatEntry>>,
    ) -> DirInfo {
        let mut children: Vec<Node> = Vec::new();
        if let Some(entries) = by_parent.remove(&path) {
            for e in entries {
                match e.kind {
                    Kind::File => {
                        let n = Node::File(FileInfo {
                            name: e.name,
                            size: e.size,
                            mime: e.mime,
                        });
                        children.push(n);
                    }
                    Kind::Dir => {
                        let child_path: String = path_str::join(&path, &e.name);
                        let d = build_dir(e.name, child_path, by_parent);
                        children.push(Node::Dir(d));
                    }
                }
            }
        }
        // Детерминированный порядок: сначала папки, внутри группы — по алфавиту.
        children.sort_by(|a, b| a.name().cmp(b.name()));
        children.sort_by_key(|c| match c {
            Node::Dir(_) => 0u8,
            Node::File(_) => 1u8,
        });
        DirInfo { name, children }
    }

    build_dir(root_name, String::new(), &mut by_parent)
}

// ===========================================================================
// Навигация по путям
// ===========================================================================

pub enum Found {
    File(FileSystemFileHandle),
    Dir(FileSystemDirectoryHandle),
}

impl Found {
    pub fn as_file(&self) -> Option<&FileSystemFileHandle> {
        match self {
            Found::File(f) => Some(f),
            _ => None,
        }
    }
    pub fn as_dir(&self) -> Option<&FileSystemDirectoryHandle> {
        match self {
            Found::Dir(d) => Some(d),
            _ => None,
        }
    }
}

/// Пройти цепочку директорий по сегментам пути.
/// `create = false` — открыть существующую, `true` — идемпотентно создать.
async fn walk_dirs(
    root: &FileSystemDirectoryHandle,
    path: &str,
    create: bool,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    let segments: Vec<String> = path_str::segments(path);
    let mut current: FileSystemDirectoryHandle = root.clone();
    for seg in &segments {
        current = get_directory_handle(&current, seg, create).await?;
    }
    Ok(current)
}

/// Открыть директорию по относительному пути **без создания**.
pub async fn open_dir(
    root: &FileSystemDirectoryHandle,
    path: &str,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    walk_dirs(root, path, false).await
}

/// Идемпотентно создать цепочку директорий.
pub async fn ensure_path(
    root: &FileSystemDirectoryHandle,
    path: &str,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    walk_dirs(root, path, true).await
}

/// Найти файл или директорию по относительному пути.
pub async fn find_by_path(
    root: &FileSystemDirectoryHandle,
    path: &str,
) -> Result<Found, JsValue> {
    let (parent, name): (String, String) = path_str::split_parent(path);
    if name.is_empty() {
        return Ok(Found::Dir(root.clone()));
    }
    let dir: FileSystemDirectoryHandle = if parent.is_empty() {
        root.clone()
    } else {
        walk_dirs(root, &parent, false).await?
    };
    if let Ok(fh) = get_file_handle(&dir, &name, false).await {
        return Ok(Found::File(fh));
    }
    let dh: FileSystemDirectoryHandle = get_directory_handle(&dir, &name, false).await?;
    Ok(Found::Dir(dh))
}

/// Идемпотентно удалить запись по относительному пути.
pub async fn remove_path(
    root: &FileSystemDirectoryHandle,
    path: &str,
    recursive: bool,
) -> Result<(), JsValue> {
    let (parent, name): (String, String) = path_str::split_parent(path);
    if name.is_empty() {
        return Ok(());
    }
    let dir: FileSystemDirectoryHandle = if parent.is_empty() {
        root.clone()
    } else {
        match walk_dirs(root, &parent, false).await {
            Ok(d) => d,
            Err(_) => return Ok(()), // родителя нет → удалять нечего
        }
    };
    remove_entry(&dir, &name, recursive).await
}

/// Относительный путь `child` от `root` (тонкая обёртка над `resolve`).
pub async fn path_of(
    root: &FileSystemDirectoryHandle,
    child: &JsValue,
) -> Result<Option<String>, JsValue> {
    resolve_path(root, child).await
}