//! Handle — долгоживущая ссылка; File — снимок.
//!
//! Модуль не дублирует `fsa_api_handle` / `fsa_api_file`, а раскрывает
//! их **семантику**:
//! * `describe` — метаданные самого handle'а (kind, name).
//! * `take_snapshot` — метаданные **текущего** содержимого. Снимок может
//!   устареть, если файл изменился после вызова.
//! * `same_target` — identity через `isSameEntry`, а не по имени.
//! * `is_snapshot_stale` — чистая функция.
//!
//! Плюс форматтеры для UI: весь `format!` живёт здесь, чтобы rust-analyzer
//! не путался с типами в `app.rs`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileSystemFileHandle};

/// Обёртка, обходящая баг rust-analyzer с `format!` в позиции
/// хвостового выражения `if/else`, `match`-arm или функции.
///
/// RA выводит такое `format!` как `()`, что ломает функцию с сигнатурой
/// `-> String`. `format_str` принимает `Arguments` и объявлена как
/// `-> String`; тип фиксирован и не теряется.
///
/// Использование:
///     let s = format_str(format_args!("Hello {}", name));
#[inline]
pub fn format_str(args: std::fmt::Arguments<'_>) -> String {
    std::fmt::format(args)
}

/// Вид handle'а.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    File,
    Directory,
    Unknown,
}

impl HandleKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "file" => HandleKind::File,
            "directory" => HandleKind::Directory,
            _ => HandleKind::Unknown,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            HandleKind::File => "file",
            HandleKind::Directory => "directory",
            HandleKind::Unknown => "unknown",
        }
    }
}

/// Метаданные самого handle'а — не содержимого.
#[derive(Debug, Clone, PartialEq)]
pub struct HandleInfo {
    pub kind: HandleKind,
    pub name: String,
}

/// Извлекает kind и name из handle'а. Не делает чтения содержимого.
pub fn describe(handle: &JsValue) -> HandleInfo {
    let kind_str: String = js_sys::Reflect::get(handle, &JsValue::from_str("kind"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let name: String = js_sys::Reflect::get(handle, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    HandleInfo {
        kind: HandleKind::from_str(&kind_str),
        name,
    }
}

/// Метаданные содержимого файла на момент вызова `getFile()`.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotInfo {
    pub name: String,
    pub size: f64,
    pub mime: String,
    pub last_modified_ms: f64,
}

/// Снимает снимок содержимого файла.
pub async fn take_snapshot(handle: &FileSystemFileHandle) -> Result<SnapshotInfo, JsValue> {
    let handle_js: &JsValue = handle.as_ref();
    let get_file_js = js_sys::Reflect::get(handle_js, &JsValue::from_str("getFile"))?;
    if get_file_js.is_undefined() {
        return Err(JsValue::from_str("getFile недоступен"));
    }
    let get_file: js_sys::Function = get_file_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFile не функция"))?;
    let promise_js = get_file.call0(handle_js)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFile не Promise"))?;
    let file_value: JsValue = JsFuture::from(promise).await?;
    let file: File = file_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не File"))?;
    Ok(SnapshotInfo {
        name: file.name(),
        size: file.size(),
        mime: file.type_(),
        last_modified_ms: file.last_modified(),
    })
}

/// Identity: один ли это и тот же файл. Сравниваем не имя, а цель.
pub async fn same_target(a: &JsValue, b: &JsValue) -> Result<bool, JsValue> {
    let is_same_js = js_sys::Reflect::get(a, &JsValue::from_str("isSameEntry"))?;
    if is_same_js.is_undefined() {
        return Err(JsValue::from_str("isSameEntry недоступен"));
    }
    let is_same: js_sys::Function = is_same_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("isSameEntry не функция"))?;
    let promise_js = is_same.call1(a, b)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("isSameEntry не Promise"))?;
    let value = JsFuture::from(promise).await?;
    Ok(value.as_bool().unwrap_or(false))
}

/// Чистая функция: устарел ли снимок относительно свежего.
pub fn is_snapshot_stale(previous: &SnapshotInfo, fresh: &SnapshotInfo) -> bool {
    previous.last_modified_ms != fresh.last_modified_ms
        || previous.size != fresh.size
}

// ---------------------------------------------------------------------------
// Форматтеры для UI
// ---------------------------------------------------------------------------

/// Метаданные handle'а для UI.
pub fn format_handle_info(info: &HandleInfo) -> String {
    format_str(format_args!(
        "kind: {}\nname: {}",
        info.kind.as_str(),
        info.name
    ))
}

/// Человекочитаемый размер файла.
pub fn format_size(bytes: f64) -> String {
    if bytes < 1024.0 {
        return format_str(format_args!("{:.0} Б", bytes));
    }
    if bytes < 1024.0 * 1024.0 {
        return format_str(format_args!("{:.1} КБ", bytes / 1024.0));
    }
    if bytes < 1024.0 * 1024.0 * 1024.0 {
        return format_str(format_args!("{:.2} МБ", bytes / (1024.0 * 1024.0)));
    }
    format_str(format_args!("{:.2} ГБ", bytes / (1024.0 * 1024.0 * 1024.0)))
}

/// Строка пары handle'ов с вердиктом identity.
pub fn format_identity_pair(old_name: &str, new_name: &str, same: bool) -> String {
    let verdict: String = if same {
        "✅ isSameEntry = true: это тот же самый файл.".to_string()
    } else {
        "❌ isSameEntry = false: разные файлы.".to_string()
    };
    format_str(format_args!("Первый: {}\nВторой: {}\n{}", old_name, new_name, verdict))
}

/// Снимок содержимого для UI: человекочитаемый размер,
/// счётчик пользовательских снимков (0 = автоопрос) и пометка stale.
pub fn format_snapshot(snap: &SnapshotInfo, stale: Option<bool>, counter: u32) -> String {
    let mime: &str = if snap.mime.is_empty() { "—" } else { snap.mime.as_str() };
    let counter_line: String = if counter > 0 {
        format_str(format_args!("# снимок №{counter}"))
    } else {
        "# автообновление".to_string()
    };
    let mut lines: Vec<String> = Vec::with_capacity(6);
    lines.push(counter_line);
    lines.push(format!("name:         {}", snap.name));
    lines.push(format!("size:         {}", format_size(snap.size)));
    lines.push(format!("mime:         {}", mime));
    lines.push(format!("lastModified: {:.0} ms", snap.last_modified_ms));
    if let Some(s) = stale {
        let note: String = if s {
            "⚠️ Изменено с момента снимка.".to_string()
        } else {
            "✅ Совпадает с baseline.".to_string()
        };
        lines.push(note);
    }
    lines.join("\n")
}

/// Полезная нагрузка для «Изменить содержимое»: строка с таймстемпом.
pub fn format_modification_payload(ts: f64) -> String {
    format_str(format_args!("modified at {}\n", ts))
}

/// Человекочитаемое объяснение ошибки FSA.
pub fn explain_error(e: &JsValue) -> String {
    let raw: String = e.as_string().unwrap_or_else(|| format!("{e:?}"));
    if raw.contains("SecurityError")
        || raw.contains("gesture")
        || raw.contains("User activation")
    {
        return "Браузер требует жест пользователя — нажмите кнопку ещё раз."
            .to_string();
    }
    if raw.contains("AbortError") {
        return "Диалог закрыт.".to_string();
    }
    if raw.contains("NotFoundError") {
        return "Файл не найден.".to_string();
    }
    format_str(format_args!("Ошибка FSA: {raw}"))
}