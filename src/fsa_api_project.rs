//! Практический проект: «Аудит папки».
//!
//! * Итеративный обход директории (не рекурсия).
//! * SHA-256 первых 64 КиБ каждого файла.
//! * Экспорт отчёта в CSV.
//!
//! Yield раз в 16 мс — прогресс, live-лог и отмена видны в UI.

use std::collections::VecDeque;

use gloo_timers::future::TimeoutFuture;
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

/// Запись отчёта.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditRow {
    pub path: String,
    pub size: f64,
    pub mime: String,
    pub hash_prefix: String,
}

/// Результат аудита.
pub struct AuditReport {
    pub rows: Vec<AuditRow>,
    pub truncated: bool,
}

const HASH_PREFIX_BYTES: f64 = 64.0 * 1024.0;

/// Хеш первых 64 КиБ файла.
async fn file_hash_prefix(file: &web_sys::File) -> Result<String, JsValue> {
    let end = file.size().min(HASH_PREFIX_BYTES);
    let slice = file.slice_with_f64_and_f64(0.0, end)?;
    let promise: js_sys::Promise = slice.array_buffer();
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let full = hex::encode(hasher.finalize());
    Ok(full[..16].to_string())
}

/// Аудит директории с лимитом файлов.
///
/// `on_file(count, path, size)` вызывается после каждого найденного
/// файла — для live-лога и прогресса в UI.
pub async fn audit_directory(
    root: &FileSystemDirectoryHandle,
    max_files: usize,
    mut on_file: impl FnMut(usize, &str, f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<AuditReport, JsValue> {
    let mut rows: Vec<AuditRow> = Vec::new();
    let mut queue: VecDeque<(FileSystemDirectoryHandle, String)> = VecDeque::new();
    queue.push_back((root.clone(), String::new()));
    let mut last_yield = 0.0_f64;

    while let Some((dir, prefix)) = queue.pop_front() {
        if is_cancelled() || rows.len() >= max_files {
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
            if is_cancelled() || rows.len() >= max_files {
                break;
            }
            let entry = match crate::fsa_api_directory::iter_next(&iter).await? {
                Some(v) => v,
                None => break,
            };

            let kind = js_sys::Reflect::get(&entry, &JsValue::from_str("kind"))?
                .as_string()
                .unwrap_or_default();
            let name = js_sys::Reflect::get(&entry, &JsValue::from_str("name"))?
                .as_string()
                .unwrap_or_default();

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
                let size = file.size();
                let hash_prefix = file_hash_prefix(&file)
                    .await
                    .unwrap_or_else(|_| "error".into());
                rows.push(AuditRow {
                    path: full_path.clone(),
                    size,
                    mime: file.type_(),
                    hash_prefix,
                });
                on_file(rows.len(), &full_path, size);

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

    let truncated = rows.len() >= max_files;
    Ok(AuditReport { rows, truncated })
}

/// Сериализация отчёта в CSV.
pub fn report_to_csv(report: &AuditReport) -> String {
    let mut out = String::with_capacity(report.rows.len() * 96);
    out.push_str("path,size_bytes,mime,sha256_prefix\n");
    for row in &report.rows {
        out.push_str(&csv_escape(&row.path));
        out.push(',');
        out.push_str(&format!("{:.0}", row.size));
        out.push(',');
        out.push_str(&csv_escape(&row.mime));
        out.push(',');
        out.push_str(&row.hash_prefix);
        out.push('\n');
    }
    if report.truncated {
        out.push_str("# truncated by SCAN_LIMIT\n");
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let mut escaped = String::with_capacity(s.len() + 2);
        escaped.push('"');
        for ch in s.chars() {
            if ch == '"' {
                escaped.push('"');
            }
            escaped.push(ch);
        }
        escaped.push('"');
        escaped
    } else {
        s.to_string()
    }
}