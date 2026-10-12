//! Исполнение picker'ов с осмысленной классификацией ошибок.
//!
//! Возвращает `PickOutcome<T>`:
//! * `Picked(T)` — пользователь выбрал;
//! * `Cancelled` — диалог закрыт (**не ошибка!**);
//! * `Blocked(reason)` — браузер требует user gesture или FSA недоступен;
//! * `Failed(reason)` — реальная ошибка.
//!
//! `Capabilities::detect()` — модель доступных методов. Разные браузеры
//! могут иметь разный набор: только `showOpenFilePicker`, все три, ни одного.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

use crate::fsa_choose_options::format_str;

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// Что из FSA доступно в этом браузере.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub open: bool,
    pub save: bool,
    pub directory: bool,
}

impl Capabilities {
    pub fn detect() -> Self {
        Capabilities {
            open: has_method("showOpenFilePicker"),
            save: has_method("showSaveFilePicker"),
            directory: has_method("showDirectoryPicker"),
        }
    }

    pub fn any(&self) -> bool {
        self.open || self.save || self.directory
    }

    pub fn all(&self) -> bool {
        self.open && self.save && self.directory
    }

    /// Короткая строка для UI.
    pub fn summary(&self) -> String {
        format_str(format_args!(
            "open {} | save {} | directory {}",
            mark(self.open),
            mark(self.save),
            mark(self.directory),
        ))
    }
}

/// Отдельная функция с явной сигнатурой — rust-analyzer не теряет тип
/// в замыкании с хвостовым `if/else`.
fn mark(b: bool) -> &'static str {
    if b {
        return "✓";
    }
    "—"
}

fn has_method(name: &str) -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let w: &JsValue = window.as_ref();
    js_sys::Reflect::get(w, &JsValue::from_str(name))
        .map(|v| !v.is_undefined())
        .unwrap_or(false)
}

/// Совместимость: раньше проверяли только `showOpenFilePicker`.
pub fn is_fsa_available() -> bool {
    has_method("showOpenFilePicker")
}

// ---------------------------------------------------------------------------
// PickOutcome
// ---------------------------------------------------------------------------

/// Результат вызова picker'а.
#[derive(Debug)]
pub enum PickOutcome<T> {
    Picked(T),
    Cancelled,
    Blocked(String),
    Failed(String),
}

impl<T> PickOutcome<T> {
    pub fn is_picked(&self) -> bool {
       return matches!(self, PickOutcome::Picked(_))
    }
    pub fn is_cancelled(&self) -> bool {
       return matches!(self, PickOutcome::Cancelled)
    }
    /// Короткая метка для UI.
    pub fn kind_label(&self) -> &'static str {
        match self {
            PickOutcome::Picked(_) => "OK",
            PickOutcome::Cancelled => "отменено",
            PickOutcome::Blocked(_) => "заблокировано",
            PickOutcome::Failed(_) => "ошибка",
        }
    }
}

// ---------------------------------------------------------------------------
// Ошибки и классификация
// ---------------------------------------------------------------------------

enum CallError {
    Cancelled,
    Blocked(String),
    Failed(String),
}

impl<T> From<CallError> for PickOutcome<T> {
    fn from(e: CallError) -> Self {
        match e {
            CallError::Cancelled => PickOutcome::Cancelled,
            CallError::Blocked(m) => PickOutcome::Blocked(m),
            CallError::Failed(m) => PickOutcome::Failed(m),
        }
    }
}

/// Подсказка «что делать» к сообщению `Blocked`.
fn blocked_retry_hint() -> String {
    format_str(format_args!(
        "Браузер требует жест пользователя.\n\
         Нажмите кнопку ещё раз — диалог должен открыться.",
    ))
}

/// Классифицирует raw ошибку JS в осмысленное состояние.
fn classify_error(e: &JsValue) -> CallError {
    let name: String = js_sys::Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    if name == "AbortError" {
        return CallError::Cancelled;
    }
    if name == "SecurityError" || name == "NotAllowedError" {
        return CallError::Blocked(blocked_retry_hint());
    }
    let raw: String = e
        .as_string()
        .unwrap_or_else(|| format_str(format_args!("{e:?}")));
    CallError::Failed(format_str(format_args!("Ошибка picker'а: {raw}")))
}

// ---------------------------------------------------------------------------
// Вызовы
// ---------------------------------------------------------------------------

async fn call_picker(method: &str, options: JsValue) -> Result<JsValue, CallError> {
    let window = web_sys::window()
        .ok_or_else(|| CallError::Failed(format_str(format_args!("no window"))))?;
    let w: &JsValue = window.as_ref();

    let fn_js = js_sys::Reflect::get(w, &JsValue::from_str(method))
        .map_err(|e| CallError::Failed(format_str(format_args!("{e:?}"))))?;
    if fn_js.is_undefined() {
        return Err(CallError::Blocked(format_str(format_args!(
            "{method} недоступен — FSA не поддерживается",
        ))));
    }
    let func: js_sys::Function = fn_js.dyn_into().map_err(|_| {
        CallError::Failed(format_str(format_args!("{method} не функция")))
    })?;

    let promise_js = func.call1(w, &options).map_err(|e| classify_error(&e))?;
    let promise: js_sys::Promise = promise_js.dyn_into().map_err(|_| {
        CallError::Failed(format_str(format_args!("{method} не Promise")))
    })?;

    JsFuture::from(promise).await.map_err(|e| classify_error(&e))
}

pub async fn open_files(options: JsValue) -> PickOutcome<Vec<FileSystemFileHandle>> {
    let value = match call_picker("showOpenFilePicker", options).await {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let array = match value.dyn_into::<js_sys::Array>() {
        Ok(a) => a,
        Err(_) => {
            return PickOutcome::Failed(format_str(format_args!(
                "showOpenFilePicker вернул не Array",
            )));
        }
    };
    let mut handles: Vec<FileSystemFileHandle> =
        Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let h: FileSystemFileHandle = match array.get(i).dyn_into() {
            Ok(h) => h,
            Err(_) => {
                return PickOutcome::Failed(format_str(format_args!(
                    "элемент {i} не FileSystemFileHandle",
                )));
            }
        };
        handles.push(h);
    }
    PickOutcome::Picked(handles)
}

pub async fn save_file(options: JsValue) -> PickOutcome<FileSystemFileHandle> {
    let value = match call_picker("showSaveFilePicker", options).await {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    match value.dyn_into::<FileSystemFileHandle>() {
        Ok(h) => PickOutcome::Picked(h),
        Err(_) => PickOutcome::Failed(format_str(format_args!(
            "showSaveFilePicker вернул не FileSystemFileHandle",
        ))),
    }
}

pub async fn pick_directory(
    options: JsValue,
) -> PickOutcome<FileSystemDirectoryHandle> {
    let value = match call_picker("showDirectoryPicker", options).await {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    match value.dyn_into::<FileSystemDirectoryHandle>() {
        Ok(h) => PickOutcome::Picked(h),
        Err(_) => PickOutcome::Failed(format_str(format_args!(
            "showDirectoryPicker вернул не FileSystemDirectoryHandle",
        ))),
    }
}

/// Проверка «директорию можно выбирать?» — единая точка.
pub fn ensure_directory_available() -> Option<String> {
    if has_method("showDirectoryPicker") {
        return None;
    }
    Some(crate::fsa_choose_fallback::directory_unsupported_message())
}