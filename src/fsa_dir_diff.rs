//! Diff двух состояний директории: снимок → изменение → снимок → diff.
//!
//! Ценность:
//! * `Snapshot` даёт стабильные ключи (пути) для дельта-обновлений.
//! * `diff` — **чистая** функция без I/O. Тестируется host-тестами.
//! * `snapshot` — тонкая обёртка над `fsa_api_directory::scan`.
//! * `describe` — человекочитаемая сводка для UI/логов.
//!
//! Почему не через `scan` дважды и не в лоб: `scan` возвращает
//! `Vec<Entry>` в порядке обхода, который не гарантирован между
//! вызовами. Прямое сравнение двух `Vec` даст ложные срабатывания.
//! `BTreeMap` решает это раз и навсегда.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use wasm_bindgen::prelude::*;
use web_sys::FileSystemDirectoryHandle;

use crate::fsa_api_directory::{scan, Entry};

/// Метаданные одного файла в снимке.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMeta {
    pub size: f64,
    pub mime: String,
}

/// Снимок директории: путь → метаданные.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub entries: BTreeMap<String, FileMeta>,
}

impl Snapshot {
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        let mut map = BTreeMap::new();
        for e in entries {
            map.insert(
                e.path,
                FileMeta {
                    size: e.size,
                    mime: e.mime,
                },
            );
        }
        Self { entries: map }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Разница между двумя снимками.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Diff {
    /// Файлы, которых не было в old, но есть в new.
    pub added: Vec<String>,
    /// Файлы, которые были в old, но исчезли в new.
    pub removed: Vec<String>,
    /// Файлы, у которых изменился размер: (path, old_size, new_size).
    pub size_changed: Vec<(String, f64, f64)>,
    /// Число путей, которые есть в обоих снимках и не изменились.
    pub unchanged_count: usize,
}

impl Diff {
    pub fn total_changes(&self) -> usize {
        self.added.len() + self.removed.len() + self.size_changed.len()
    }

    /// Человекочитаемая сводка — для лога или для текстового UI.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.push(format!(
            "diff: +{} / -{} / ~{} / ={}",
            self.added.len(),
            self.removed.len(),
            self.size_changed.len(),
            self.unchanged_count
        ));
        for p in &self.added {
            out.push(format!("  + {p}"));
        }
        for p in &self.removed {
            out.push(format!("  - {p}"));
        }
        for (p, old, new) in &self.size_changed {
            out.push(format!("  ~ {p}: {old} → {new}"));
        }
        out
    }
}

/// Снять снимок директории.
///
/// Параметры совпадают с `scan`, потому что это его обёртка —
/// вызывающий код не должен помнить два разных API.
pub async fn snapshot(
    root: &FileSystemDirectoryHandle,
    max_files: usize,
    on_progress: impl FnMut(usize),
    is_cancelled: impl Fn() -> bool,
) -> Result<Snapshot, JsValue> {
    let entries = scan(root, max_files, on_progress, is_cancelled).await?;
    Ok(Snapshot::from_entries(entries))
}

/// Чистая функция: сравнить два снимка.
///
/// Не делает I/O, работает только с `BTreeMap` — тестируется
/// обычным `cargo test`.
pub fn diff(old: &Snapshot, new: &Snapshot) -> Diff {
    let old_keys: BTreeSet<&String> = old.entries.keys().collect();
    let new_keys: BTreeSet<&String> = new.entries.keys().collect();

    let added: Vec<String> = new_keys
        .difference(&old_keys)
        .map(|s| (*s).clone())
        .collect();
    let removed: Vec<String> = old_keys
        .difference(&new_keys)
        .map(|s| (*s).clone())
        .collect();

    let mut size_changed: Vec<(String, f64, f64)> = Vec::new();
    let mut unchanged_count = 0;
    for (path, old_meta) in &old.entries {
        if let Some(new_meta) = new.entries.get(path) {
            if (old_meta.size - new_meta.size).abs() > f64::EPSILON {
                size_changed.push((path.clone(), old_meta.size, new_meta.size));
            } else {
                unchanged_count += 1;
            }
        }
    }

    Diff {
        added,
        removed,
        size_changed,
        unchanged_count,
    }
}