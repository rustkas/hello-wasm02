//! 47. Обработка ошибок: извлечение структурированной информации из
//! `JsValue` и преобразование её в прикладные категории.
//!
//! ## Зачем это нужно
//!
//! Когда JS бросает исключение (или Promise отклоняется), Rust видит
//! его как opaque `JsValue`. Ни `name`, ни `message`, ни `stack` не
//! доступны напрямую — их надо извлечь через `Reflect::get`. Этот
//! модуль инкапсулирует извлечение и преобразование в прикладные
//! категории с готовыми user-facing сообщениями.

use js_sys::Reflect;
use wasm_bindgen::prelude::*;

use crate::video_frame::format_str;

// =============================================================================
// Severity
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

// =============================================================================
// AppError — прикладная категория
// =============================================================================

#[derive(Clone, Debug)]
pub enum AppError {
    /// Пользователь отменил операцию (`AbortError`).
    Cancelled,
    /// Отказ в доступе / разрешениях (`NotAllowedError`, `SecurityError`).
    PermissionDenied,
    /// Объект не найден (`NotFoundError` вне контекста отмены).
    NotFound,
    /// Некорректное состояние (`InvalidStateError`).
    InvalidState,
    /// Сетевой сбой / таймаут (`NetworkError`, `TimeoutError`).
    Network,
    /// Не поддерживается средой (`NotSupportedError`).
    NotSupported,
    /// Некорректный пользовательский ввод (`RangeError`, `SyntaxError`).
    InvalidInput,
    /// Ошибка программиста (`TypeError`, `ReferenceError`).
    Programmer,
    /// Прикладная ошибка — `throw new Error("...")` без подкласса.
    /// Осмысленная ошибка, брошенная намеренно.
    Generic { message: String },
    /// Нестандартный rejection — `Promise.reject('string')` / `42` / `true`.
    /// Обычно означает баг в JS-коде, где вместо `Error` отдают примитив.
    NonStandardRejection { raw: String },
    /// Неизвестная ошибка — категория-ловушка для действительно непонятных случаев.
    Unknown { name: String, message: String },
}

impl AppError {
    /// Преобразует структурированную информацию об ошибке в категорию.
    ///
    /// ## Осторожно: одинаковые имена — разные смыслы
    ///
    /// `NotFoundError` в Web Bluetooth означает «пользователь отменил
    /// выбор», а в `fetch` — «ресурс не найден». Одна и та же строка
    /// `name` даёт разные прикладные категории в зависимости от API.
    /// Здесь используется наиболее распространённое сопоставление;
    /// для специфичных API вызывающий код может переопределить
    /// категорию вручную.
    pub fn from_info(info: &JsErrorInfo) -> Self {
        return match info.name.as_str() {
            "AbortError" => Self::Cancelled,
            "NotFoundError" => Self::NotFound,
            "NotAllowedError" | "SecurityError" | "PermissionDeniedError" => {
                Self::PermissionDenied
            }
            "InvalidStateError" | "InvalidModificationError" => Self::InvalidState,
            "NetworkError" | "TimeoutError" => Self::Network,
            "NotSupportedError" => Self::NotSupported,
            "RangeError" | "SyntaxError" => Self::InvalidInput,
            "TypeError" | "ReferenceError" => Self::Programmer,
            // `Error` без подкласса — осмысленная прикладная ошибка.
            "Error" => Self::Generic {
                message: info.message.clone(),
            },
            // Примитивы вместо `Error` — нестандартный rejection.
            "String" | "Number" | "Boolean" => Self::NonStandardRejection {
                raw: info.message.clone(),
            },
            _ => Self::Unknown {
                name: info.name.clone(),
                message: info.message.clone(),
            },
        };
    }

    pub fn severity(&self) -> Severity {
        return match self {
            Self::Cancelled => Severity::Info,
            Self::PermissionDenied
            | Self::NotFound
            | Self::InvalidState
            | Self::InvalidInput
            | Self::Generic { .. } => Severity::Warning,
            Self::Network
            | Self::NotSupported
            | Self::Programmer
            | Self::NonStandardRejection { .. }
            | Self::Unknown { .. } => Severity::Error,
        };
    }

    pub fn category_label(&self) -> &'static str {
        return match self {
            Self::Cancelled => "Отменено",
            Self::PermissionDenied => "Нет доступа",
            Self::NotFound => "Не найдено",
            Self::InvalidState => "Некорректное состояние",
            Self::Network => "Сетевой сбой",
            Self::NotSupported => "Не поддерживается",
            Self::InvalidInput => "Неверный ввод",
            Self::Programmer => "Ошибка программиста",
            Self::Generic { .. } => "Прикладная ошибка",
            Self::NonStandardRejection { .. } => "Нестандартный rejection",
            Self::Unknown { .. } => "Неизвестно",
        };
    }

    pub fn css_class(&self) -> &'static str {
        return match self {
            Self::Cancelled => "err-card-cat-cancelled",
            Self::PermissionDenied => "err-card-cat-permission",
            Self::NotFound => "err-card-cat-notfound",
            Self::InvalidState => "err-card-cat-invalidstate",
            Self::Network => "err-card-cat-network",
            Self::NotSupported => "err-card-cat-unsupported",
            Self::InvalidInput => "err-card-cat-invalidinput",
            Self::Programmer => "err-card-cat-programmer",
            Self::Generic { .. } => "err-card-cat-generic",
            Self::NonStandardRejection { .. } => "err-card-cat-nonstandard",
            Self::Unknown { .. } => "err-card-cat-unknown",
        };
    }

    /// User-facing сообщение. Никаких «TypeError: x is not a function» —
    /// пользователь должен видеть понятный текст.
    pub fn user_message(&self) -> String {
        return match self {
            Self::Cancelled => String::from("Операция отменена пользователем"),
            Self::PermissionDenied => String::from("Нет разрешения на выполнение операции"),
            Self::NotFound => String::from("Запрашиваемый объект не найден"),
            Self::InvalidState => String::from("Операция невозможна в текущем состоянии"),
            Self::Network => String::from("Сетевой сбой — проверьте соединение"),
            Self::NotSupported => String::from("Операция не поддерживается в этом браузере"),
            Self::InvalidInput => String::from("Проверьте введённые данные"),
            Self::Programmer => {
                String::from("Внутренняя ошибка приложения. Обратитесь в поддержку.")
            }
            Self::Generic { message } => format_str(format_args!("{message}")),
            Self::NonStandardRejection { raw } => format_str(format_args!(
                "Нестандартная ошибка: {raw}"
            )),
            Self::Unknown { message, .. } => {
                format_str(format_args!("Неизвестная ошибка: {message}"))
            }
        };
    }
}

// =============================================================================
// JsErrorInfo — извлечённая структура
// =============================================================================

#[derive(Clone, Debug)]
pub struct JsErrorInfo {
    pub name: String,
    pub message: String,
    pub stack: Option<String>,
    pub code: Option<i32>,
    /// Если оригинальное значение было примитивом (строка, число),
    /// это поле содержит его текстовое представление.
    pub raw: Option<String>,
}

impl JsErrorInfo {
    fn primitive(name: &str, message: String) -> Self {
        return Self {
            name: String::from(name),
            message: message.clone(),
            stack: None,
            code: None,
            raw: Some(message),
        };
    }
}

/// Legacy-таблица `DOMException.code` по имени.
///
/// ## Зачем fallback
///
/// В Chrome `DOMException.code` — это getter на прототипе, но
/// `Reflect::get(value, "code")` в некоторых версиях (особенно
/// headless) возвращает `undefined` или `0`. Таблица восстанавливает
/// код по имени — это стандартные значения из WebIDL
/// (`NotFoundError` = 8, `SecurityError` = 18, `AbortError` = 20 и т.д.).
///
/// ## Где это НЕ сработает
///
/// Для кастомных DOMException с нестандартным именем — вернём `None`.
/// Для `Error` (не DOMException) — тоже `None`, потому что у обычного
/// Error нет числового кода.
fn dom_exception_code(name: &str) -> Option<i32> {
    return match name {
        "IndexSizeError" => Some(1),
        "HierarchyRequestError" => Some(3),
        "WrongDocumentError" => Some(4),
        "InvalidCharacterError" => Some(5),
        "NoModificationAllowedError" => Some(7),
        "NotFoundError" => Some(8),
        "NotSupportedError" => Some(9),
        "InUseAttributeError" => Some(10),
        "InvalidStateError" => Some(11),
        "SyntaxError" => Some(12),
        "InvalidModificationError" => Some(13),
        "NamespaceError" => Some(14),
        "InvalidAccessError" => Some(15),
        "TypeMismatchError" => Some(17),
        "SecurityError" => Some(18),
        "NetworkError" => Some(19),
        "AbortError" => Some(20),
        "URLMismatchError" => Some(21),
        "QuotaExceededError" => Some(22),
        "TimeoutError" => Some(23),
        "InvalidNodeTypeError" => Some(24),
        "DataCloneError" => Some(25),
        _ => None,
    };
}

/// Извлекает структурированную информацию из `JsValue` ошибки.
///
/// ## Обрабатываемые случаи
///
/// - `undefined` / `null` — Promise.reject с этими значениями;
/// - примитивы (строка, число, boolean) — нестандартный rejection;
/// - объекты с `name`/`message`/`stack`/`code` — `Error`, `DOMException`.
///
/// ## Что НЕ паникует
///
/// Функция безопасна для любого `JsValue`. `Reflect::get` на примитиве
/// возвращает `Err`, который мы игнорируем; при этом сам примитив
/// сохраняется в `raw`.
pub fn extract_error_info(value: &JsValue) -> JsErrorInfo {
    // Специальные случаи.
    if value.is_undefined() {
        return JsErrorInfo::primitive("Undefined", String::from("Promise rejected with undefined"));
    }
    if value.is_null() {
        return JsErrorInfo::primitive("Null", String::from("Promise rejected with null"));
    }

    // Примитивы.
    if let Some(s) = value.as_string() {
        return JsErrorInfo::primitive("String", s);
    }
    if let Some(n) = value.as_f64() {
        return JsErrorInfo::primitive("Number", format_str(format_args!("{n}")));
    }
    if let Some(b) = value.as_bool() {
        return JsErrorInfo::primitive("Boolean", format_str(format_args!("{b}")));
    }

    // Объект (Error, DOMException, произвольный объект).
    let name = Reflect::get(value, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| String::from("Error"));

    let message = Reflect::get(value, &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| String::from("(без сообщения)"));

    let stack = Reflect::get(value, &JsValue::from_str("stack"))
        .ok()
        .and_then(|v| v.as_string());

    // DOMException.code:
    //   шаг 1 — попытка через Reflect::get (в не-headless Chrome работает);
    //   шаг 2 — fallback по имени (для headless и кастомных сборок).
    // Фильтр `!= 0` отсекает значение по умолчанию для не-DOMException.
    let code = Reflect::get(value, &JsValue::from_str("code"))
        .ok()
        .and_then(|v| {
            if let Some(n) = v.as_f64() {
                return Some(n as i32);
            }
            v.as_string().and_then(|s| s.parse::<i32>().ok())
        })
        .filter(|&n| n != 0)
        .or_else(|| dom_exception_code(&name));

    return JsErrorInfo {
        name,
        message,
        stack,
        code,
        raw: None,
    };
}

// =============================================================================
// Утилиты
// =============================================================================

/// Обрезает стек до `max_lines` строк, добавляя пометку, если
/// строк было больше.
pub fn truncate_stack(stack: &str, max_lines: usize) -> String {
    let all_lines: Vec<&str> = stack.lines().collect();
    let total = all_lines.len();
    let take = total.min(max_lines);
    let mut out = all_lines[..take].join("\n");
    if total > max_lines {
        out.push_str(&format_str(format_args!("\n… (+{} строк)", total - max_lines)));
    }
    return out;
}

/// Строка вида "TypeError: message" для логов.
pub fn format_error_brief(info: &JsErrorInfo) -> String {
    return format_str(format_args!("{}: {}", info.name, info.message));
}