//! Интеллектуальный выбор способа отправки.
//!
//! Без лимитов по размеру — сервер пишет чанки на диск.

use wasm_bindgen::prelude::*;

pub const SIZE_FETCH_MAX: f64 = 5.0 * 1024.0 * 1024.0;
pub const SIZE_XHR_MAX: f64 = 100.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadMethod { Fetch, Xhr, Chunked }

impl UploadMethod {
    pub fn label(&self) -> &'static str {
        return match self {
            UploadMethod::Fetch => "fetch + FormData",
            UploadMethod::Xhr => "XMLHttpRequest + прогресс",
            UploadMethod::Chunked => "Chunked upload (по 1 МБ)",
        };
    }
    pub fn tag(&self) -> &'static str {
        return match self {
            UploadMethod::Fetch => "fetch",
            UploadMethod::Xhr => "xhr",
            UploadMethod::Chunked => "chunked",
        };
    }
    pub fn from_tag(tag: &str) -> Option<Self> {
        return match tag {
            "fetch" => Some(UploadMethod::Fetch),
            "xhr" => Some(UploadMethod::Xhr),
            "chunked" => Some(UploadMethod::Chunked),
            _ => None,
        };
    }
}

#[derive(Debug, Clone)]
pub struct Strategy {
    pub method: UploadMethod,
    pub reason: String,
    pub connection_type: String,
    pub size_mb: f64,
}

pub fn detect_connection_type() -> String {
    let window = match web_sys::window() { Some(w) => w, None => return "unknown".into() };
    let navigator = window.navigator();
    let nav_js: &JsValue = navigator.as_ref();
    let conn = match js_sys::Reflect::get(nav_js, &JsValue::from_str("connection")) {
        Ok(v) => v, Err(_) => return "unknown".into(),
    };
    if conn.is_undefined() || conn.is_null() { return "unknown".into(); }
    return match js_sys::Reflect::get(&conn, &JsValue::from_str("effectiveType")) {
        Ok(v) => v.as_string().unwrap_or_else(|| "unknown".into()),
        Err(_) => "unknown".into(),
    };
}

fn is_slow_connection(c: &str) -> bool {
    return matches!(c, "slow-2g" | "2g");
}

pub fn pick_strategy(file_size: f64, connection: &str) -> Strategy {
    let size_mb = file_size / (1024.0 * 1024.0);

    if is_slow_connection(connection) {
        return Strategy {
            method: UploadMethod::Chunked,
            reason: format!(
                "Соединение медленное ({connection}) — chunked upload \
                 переживает обрывы и умеет докачку.",
            ),
            connection_type: connection.to_string(),
            size_mb,
        };
    }
    if file_size < SIZE_FETCH_MAX {
        return Strategy {
            method: UploadMethod::Fetch,
            reason: format!(
                "Файл маленький ({:.1} МБ) — прогресс не нужен, \
                 простой fetch быстрее и проще.", size_mb,
            ),
            connection_type: connection.to_string(),
            size_mb,
        };
    }
    if file_size < SIZE_XHR_MAX {
        return Strategy {
            method: UploadMethod::Xhr,
            reason: format!(
                "Файл средний ({:.1} МБ) — нужен прогресс отправки \
                 и возможность отмены.", size_mb,
            ),
            connection_type: connection.to_string(),
            size_mb,
        };
    }
    return Strategy {
        method: UploadMethod::Chunked,
        reason: format!(
            "Файл большой ({:.1} МБ) — chunked upload на диске \
             работает для любого размера.", size_mb,
        ),
        connection_type: connection.to_string(),
        size_mb,
    };
}