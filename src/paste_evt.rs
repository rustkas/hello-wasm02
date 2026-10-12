//! Событие `paste`: доступ к буферу обмена без разрешений.
//!
//! `ClipboardEvent` (события `copy`, `cut`, `paste`) — второй способ
//! доступа к буферу обмена. В отличие от Async Clipboard API:
//!
//!   • НЕ требует разрешения `clipboard-read`;
//!   • данные доступны СИНХРОННО, внутри обработчика;
//!   • сразу даёт все MIME-типы буфера: `text/plain`, `text/html`,
//!     `text/uri-list`, `Files`;
//!   • `text/html` приходит БЕЗ CF_HTML-обёртки (в отличие от Async API).
//!
//! ПРАВИЛО: `clipboardData` валиден только внутри обработчика. После
//! `return` браузер очищает `DataTransfer`. Все данные нужно извлечь
//! синхронно.
//!
//! ПРАВИЛО: файлы читаем через `clipboardData.items`, а НЕ через
//! `clipboardData.files`. На Windows Chrome часто кладёт в буфер только
//! «primary» файл; `items` может содержать больше (или тоже один —
//! см. диагностику).
//!
//! ПРАВИЛО: `web-sys` не всегда экспортирует типизированный
//! `ClipboardEvent.clipboard_data()`. Извлекаем `clipboardData` через
//! `Reflect::get` — это работает независимо от версии web-sys.

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

// ============================================================
//                    ПУБЛИЧНЫЕ ТИПЫ
// ============================================================

/// Метаданные файла, скопированного из ОС.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub last_modified: f64,
}

/// Один item из `clipboardData.items`.
#[derive(Debug, Clone, Default)]
pub struct ItemDump {
    pub index: u32,
    /// `"string"` или `"file"`.
    pub kind: String,
    /// Для kind=string — MIME-тип; для kind=file — обычно пусто.
    pub mime: String,
    /// Метаданные файла, если item.file.
    pub file_name: Option<String>,
    pub file_size: Option<u64>,
    pub file_mime: Option<String>,
    pub file_last_modified: Option<f64>,
}

/// Полный дамп `DataTransfer` для диагностики.
#[derive(Debug, Clone, Default)]
pub struct DataTransferDump {
    /// `clipboardData.items.length`.
    pub items_count: u32,
    /// `clipboardData.files.length`.
    pub files_count: u32,
    /// `clipboardData.types` — список MIME-типов буфера.
    pub types: Vec<String>,
    /// Все items с метаданными.
    pub items: Vec<ItemDump>,
    /// Файлы через `.files` — то, что Chrome отдаёт напрямую.
    pub files_from_files_list: Vec<FileMeta>,
    /// Файлы через `.items` с `kind === "file"` и `getAsFile()`.
    pub files_from_items: Vec<FileMeta>,
}

impl DataTransferDump {
        /// true, если `.items` содержит больше файлов, чем `.files` API.
    ///
    /// Расхождение имеет смысл **только для файлов**. Для plain-текста
    /// `items=1, files=0` — это норма: один string-item, ноль файлов.
    ///
    /// Также важно понимать: если браузер отдал один файл из трёх —
    /// расхождения **нет**. Данные согласованы, но неполны.
    /// Это ограничение браузера (см. `explain_multifile_limitation`).
    pub fn has_discrepancy(&self) -> bool {
        self.files_from_items.len() > self.files_from_files_list.len()
    }
}

/// Снимок содержимого `ClipboardEvent.clipboardData`.
#[derive(Debug, Clone, Default)]
pub struct PastePayload {
    pub plain: String,
    pub html: Option<String>,
    pub uris: Vec<String>,
    pub files: Vec<FileMeta>,
    pub types: Vec<String>,
    /// Диагностический дамп — показываем в UI для отладки.
    pub diagnostic: DataTransferDump,
}

impl PastePayload {
    pub fn is_empty(&self) -> bool {
        self.plain.is_empty()
            && self.html.as_deref().map_or(true, |s| s.is_empty())
            && self.uris.is_empty()
            && self.files.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.plain.is_empty() {
            parts.push(format!("plain: {} симв.", self.plain.chars().count()));
        }
        if let Some(h) = &self.html {
            if !h.is_empty() {
                parts.push(format!("html: {} симв.", h.chars().count()));
            }
        }
        if !self.uris.is_empty() {
            parts.push(format!("uri: {}", self.uris.len()));
        }
        if !self.files.is_empty() {
            parts.push(format!("files: {}", self.files.len()));
        }
        if parts.is_empty() {
            return "пусто".to_string();
        }
        return parts.join(" · ");
    }
}

// ============================================================
//                    ИЗВЛЕЧЕНИЕ
// ============================================================

pub fn extract_from_event(ev: &web_sys::Event) -> Option<PastePayload> {
    let dt = get_data_transfer(ev)?;
    return Some(extract_from_data_transfer(&dt));
}

pub fn extract_from_data_transfer(dt: &web_sys::DataTransfer) -> PastePayload {
    let types = read_types(dt);
    let plain = dt.get_data("text/plain").unwrap_or_default();

    let html_raw = dt.get_data("text/html").unwrap_or_default();
    let html = if html_raw.is_empty() {
        None
    } else {
        Some(strip_cf_html_wrapper(&html_raw).to_string())
    };

    let uri_raw = dt.get_data("text/uri-list").unwrap_or_default();
    let uris = parse_uri_list(&uri_raw);

    // Основной источник файлов — items.
    let files_from_items = read_files_via_items(dt);
    // Вспомогательный источник — для диагностики.
    let files_from_files_list = read_files_via_files_list(dt);

    // Итоговый набор файлов: берём максимум из двух источников.
    // Если items дал больше — используем его. Если меньше (например,
    // Safari отдаёт files, но не items) — используем files_list.
    let files = if files_from_items.len() >= files_from_files_list.len() {
        files_from_items.clone()
    } else {
        files_from_files_list.clone()
    };

    let diagnostic = dump_data_transfer(
        dt,
        types.clone(),
        files_from_items,
        files_from_files_list,
    );

    PastePayload {
        plain,
        html,
        uris,
        files,
        types,
        diagnostic,
    }
}

/// Разбирает `text/uri-list` по RFC 2483.
pub fn parse_uri_list(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        out.push(t.to_string());
    }
    out
}

/// Извлекает содержимое из CF_HTML-обёртки, если она есть.
pub fn strip_cf_html_wrapper(s: &str) -> &str {
    const START: &str = "<!--StartFragment-->";
    const END: &str = "<!--EndFragment-->";
    if let Some(i) = s.find(START) {
        let after = &s[i + START.len()..];
        if let Some(j) = after.find(END) {
            return after[..j].trim();
        }
    }
    s
}

// ============================================================
//                    ДИАГНОСТИКА
// ============================================================

fn dump_data_transfer(
    dt: &web_sys::DataTransfer,
    types: Vec<String>,
    files_from_items: Vec<FileMeta>,
    files_from_files_list: Vec<FileMeta>,
) -> DataTransferDump {
    let items = read_items_dump(dt);
    DataTransferDump {
        items_count: items.len() as u32,
        files_count: files_from_files_list.len() as u32,
        types,
        items,
        files_from_files_list,
        files_from_items,
    }
}

/// Читает `clipboardData.items` с полными метаданными.
fn read_items_dump(dt: &web_sys::DataTransfer) -> Vec<ItemDump> {
    let items = match js_sys::Reflect::get(dt, &JsValue::from_str("items")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Vec::new(),
    };
    let length = js_sys::Reflect::get(&items, &JsValue::from_str("length"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32;

    let mut out = Vec::with_capacity(length as usize);
    for i in 0..length {
        let item = match call_item_method(&items, i) {
            Some(v) if !v.is_undefined() && !v.is_null() => v,
            _ => continue,
        };

        let kind = js_sys::Reflect::get(&item, &JsValue::from_str("kind"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let mime = js_sys::Reflect::get(&item, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();

        let mut dump = ItemDump {
            index: i,
            kind,
            mime,
            ..Default::default()
        };

        if dump.kind == "file" {
            if let Some(file) = call_get_as_file(&item) {
                dump.file_name = read_file_string(&file, "name");
                dump.file_size = read_file_f64(&file, "size").map(|v| v as u64);
                dump.file_mime = read_file_string(&file, "type");
                dump.file_last_modified = read_file_f64(&file, "lastModified");
            }
        }

        out.push(dump);
    }
    out
}

fn call_get_as_file(item: &JsValue) -> Option<JsValue> {
    let f: js_sys::Function = js_sys::Reflect::get(item, &JsValue::from_str("getAsFile"))
        .ok()?
        .dyn_into()
        .ok()?;
    let file = f.call0(item).ok()?;
    if file.is_null() || file.is_undefined() {
        return None;
    }
    Some(file)
}

fn read_file_string(file: &JsValue, key: &str) -> Option<String> {
    js_sys::Reflect::get(file, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_string())
}

fn read_file_f64(file: &JsValue, key: &str) -> Option<f64> {
    js_sys::Reflect::get(file, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_f64())
}

// ============================================================
//                    ВНУТРЕННЕЕ
// ============================================================

fn get_data_transfer(ev: &web_sys::Event) -> Option<web_sys::DataTransfer> {
    let cd = js_sys::Reflect::get(ev, &JsValue::from_str("clipboardData")).ok()?;
    if cd.is_undefined() || cd.is_null() {
        return None;
    }
    return cd.dyn_into::<web_sys::DataTransfer>().ok();
}

fn read_types(dt: &web_sys::DataTransfer) -> Vec<String> {
    let types = match js_sys::Reflect::get(dt, &JsValue::from_str("types")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Vec::new(),
    };
    let length = js_sys::Reflect::get(&types, &JsValue::from_str("length"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32;
    let mut out = Vec::with_capacity(length as usize);
    for i in 0..length {
        let item = js_sys::Reflect::get(&types, &JsValue::from_f64(i as f64))
            .unwrap_or(JsValue::UNDEFINED);
        if let Some(s) = item.as_string() {
            out.push(s);
        }
    }
    out
}

/// Читает файлы через `clipboardData.files` (FileList).
fn read_files_via_files_list(dt: &web_sys::DataTransfer) -> Vec<FileMeta> {
    let files = match js_sys::Reflect::get(dt, &JsValue::from_str("files")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Vec::new(),
    };
    let length = js_sys::Reflect::get(&files, &JsValue::from_str("length"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32;

    let mut out = Vec::with_capacity(length as usize);
    for i in 0..length {
        let item = js_sys::Reflect::get(&files, &JsValue::from_f64(i as f64))
            .unwrap_or(JsValue::UNDEFINED);
        if item.is_undefined() || item.is_null() {
            continue;
        }
        if let Some(meta) = extract_file_meta(&item) {
            out.push(meta);
        }
    }
    out
}

/// Читает файлы через `clipboardData.items` с `kind === "file"`.
fn read_files_via_items(dt: &web_sys::DataTransfer) -> Vec<FileMeta> {
    let items = match js_sys::Reflect::get(dt, &JsValue::from_str("items")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Vec::new(),
    };
    let length = js_sys::Reflect::get(&items, &JsValue::from_str("length"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32;

    let mut out = Vec::with_capacity(length as usize);
    for i in 0..length {
        let item = match call_item_method(&items, i) {
            Some(v) if !v.is_undefined() && !v.is_null() => v,
            _ => continue,
        };
        let kind = js_sys::Reflect::get(&item, &JsValue::from_str("kind"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if kind != "file" {
            continue;
        }
        if let Some(file) = call_get_as_file(&item) {
            if let Some(meta) = extract_file_meta(&file) {
                out.push(meta);
            }
        }
    }
    out
}

fn extract_file_meta(file: &JsValue) -> Option<FileMeta> {
    let name = read_file_string(file, "name").unwrap_or_default();
    if name.is_empty() {
        return None;
    }
    Some(FileMeta {
        name,
        size: read_file_f64(file, "size").unwrap_or(0.0) as u64,
        mime: read_file_string(file, "type").unwrap_or_default(),
        last_modified: read_file_f64(file, "lastModified").unwrap_or(0.0),
    })
}

fn call_item_method(items: &JsValue, i: u32) -> Option<JsValue> {
    if let Ok(f) = js_sys::Reflect::get(items, &JsValue::from_str("item"))
        .and_then(|v| v.dyn_into::<js_sys::Function>())
    {
        if let Ok(v) = f.call1(items, &JsValue::from_f64(i as f64)) {
            return Some(v);
        }
    }
    js_sys::Reflect::get(items, &JsValue::from_f64(i as f64)).ok()
}