//! Квоты и StorageManager.
//!
//! `navigator.storage` — четыре метода:
//! * `estimate()` — `{ usage, quota, usageDetails }`;
//! * `persist()` — запросить персистентность (bool);
//! * `persisted()` — проверить текущий статус (bool);
//! * `getDirectory()` — корень OPFS (глава 19.8).
//!
//! **Правило:** `usage`/`quota` — оценки, не точные значения.
//! Браузер округляет ради защиты от fingerprinting. Не удивляйтесь,
//! если после записи 100 КБ `usage` не изменится.
//!
//! **Правило:** `persist()` может вернуть `false` — это не ошибка,
//! а отказ браузера. Если приложение хранит важные данные, при
//! отказе нужно предупредить пользователя.
//!
//! **Правило WASM-инференса.** `format!` в tail-позиции `if/else`/
//! `match`/блока иногда выводится как `()` → E0308. Поэтому все
//! строковые выражения идут через `format_str(format_args!(…))` —
//! тип возврата однозначен (`String`).

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

// ===========================================================================
// Строковые хелперы (правило WASM-инференса)
// ---------------------------------------------------------------------------
// `format_str` объявлена ДО первого использования — так проще читать
// файл и не наткнуться на «expected String, found ()».
// ===========================================================================

#[inline]
fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

fn fmt_bytes(n: f64) -> String {
    let bytes = n.max(0.0);
    let s: String = if bytes < 1024.0 {
        format_str(format_args!("{} B", bytes as u64))
    } else if bytes < 1024.0 * 1024.0 {
        format_str(format_args!("{:.1} KB", bytes / 1024.0))
    } else if bytes < 1024.0 * 1024.0 * 1024.0 {
        format_str(format_args!("{:.2} MB", bytes / (1024.0 * 1024.0)))
    } else {
        format_str(format_args!("{:.2} GB", bytes / (1024.0 * 1024.0 * 1024.0)))
    };
    s
}

// ===========================================================================
// Модель
// ===========================================================================

#[derive(Debug, Clone, Default)]
pub struct StorageEstimate {
    pub usage: f64,
    pub quota: f64,
    pub usage_details: Vec<(String, f64)>,
}

impl StorageEstimate {
    /// Доля занятого места: 0.0..=1.0. Если quota == 0 — 0.
    pub fn fill_ratio(&self) -> f64 {
        if self.quota <= 0.0 {
            return 0.0;
        }
        let r = self.usage / self.quota;
        let clamped: f64 = if r < 0.0 {
            0.0
        } else if r > 1.0 {
            1.0
        } else {
            r
        };
        clamped
    }

    /// Человекочитаемое описание для логов.
    ///
    /// `format_str(format_args!(…))` вместо `format!` в tail-позиции —
    /// тип возврата однозначен, E0308 не возникает.
    pub fn summary(&self) -> String {
        let s: String = format_str(format_args!(
            "usage={} quota={}",
            fmt_bytes(self.usage),
            fmt_bytes(self.quota)
        ));
        s
    }
}

// ===========================================================================
// Внутренний хелпер: вызов метода navigator.storage.<name>()
// ===========================================================================

async fn call_storage_method(name: &str) -> Result<JsValue, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let navigator_js = JsValue::from(window.navigator());

    let storage_js = js_sys::Reflect::get(&navigator_js, &JsValue::from_str("storage"))?;
    if storage_js.is_undefined() {
        return Err(JsValue::from_str("navigator.storage недоступен"));
    }

    let fn_js = js_sys::Reflect::get(&storage_js, &JsValue::from_str(name))?;
    if fn_js.is_undefined() {
        let msg = format_str(format_args!("storage.{name} недоступен"));
        return Err(JsValue::from_str(&msg));
    }
    let func: js_sys::Function = fn_js.dyn_into().map_err(|_| {
        let msg = format_str(format_args!("storage.{name} не функция"));
        JsValue::from_str(&msg)
    })?;
    let promise: js_sys::Promise = func.call0(&storage_js)?.dyn_into().map_err(|_| {
        let msg = format_str(format_args!("storage.{name} не Promise"));
        JsValue::from_str(&msg)
    })?;
    JsFuture::from(promise).await
}

// ===========================================================================
// estimate
// ===========================================================================

/// Текущее состояние хранилища origin'а.
///
/// Возвращает `{ usage, quota, usageDetails }`. `usage`/`quota` —
/// оценки (браузер округляет). `usageDetails` может быть пустым.
pub async fn estimate() -> Result<StorageEstimate, JsValue> {
    let value = call_storage_method("estimate").await?;

    let usage = js_sys::Reflect::get(&value, &JsValue::from_str("usage"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let quota = js_sys::Reflect::get(&value, &JsValue::from_str("quota"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let usage_details = parse_usage_details(&value);

    Ok(StorageEstimate {
        usage,
        quota,
        usage_details,
    })
}

fn parse_usage_details(value: &JsValue) -> Vec<(String, f64)> {
    let details = match js_sys::Reflect::get(value, &JsValue::from_str("usageDetails")) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    if details.is_undefined() || details.is_null() {
        return Vec::new();
    }

    let obj = js_sys::Object::from(details);
    let keys = js_sys::Object::keys(&obj);
    let mut out: Vec<(String, f64)> = Vec::new();
    for i in 0..keys.length() {
        let key = match keys.get(i).as_string() {
            Some(s) => s,
            None => continue,
        };
        let val = js_sys::Reflect::get(&obj, &JsValue::from_str(&key))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        out.push((key, val));
    }
    out
}

// ===========================================================================
// persist / persisted
// ===========================================================================

/// Проверить, предоставлена ли origin'у персистентность.
pub async fn persisted() -> bool {
    match call_storage_method("persisted").await {
        Ok(v) => v.as_bool().unwrap_or(false),
        Err(_) => false,
    }
}

/// Запросить персистентность.
///
/// Возвращает `true`, если браузер предоставил её (или она уже была).
/// `false` — отказ или API недоступен. Отказ — не ошибка.
pub async fn request_persist() -> bool {
    match call_storage_method("persist").await {
        Ok(v) => v.as_bool().unwrap_or(false),
        Err(_) => false,
    }
}

/// Проверка: доступен ли `navigator.storage.estimate`.
pub fn quota_available() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let navigator_js = JsValue::from(window.navigator());
    let storage_js = match js_sys::Reflect::get(&navigator_js, &JsValue::from_str("storage")) {
        Ok(s) => s,
        Err(_) => return false,
    };
    if storage_js.is_undefined() {
        return false;
    }
    js_sys::Reflect::has(&storage_js, &JsValue::from_str("estimate")).unwrap_or(false)
}