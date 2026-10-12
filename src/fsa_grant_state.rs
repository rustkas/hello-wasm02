//! Модель состояния разрешения на handle.
//!
//! FSA использует **методы самого handle'а** (`queryPermission` /
//! `requestPermission`), а не `navigator.permissions`. Причина: разрешение
//! привязано к конкретному handle'у, а не к origin'у.
//!
//! Состояние — не бинарный флаг, а значение из четырёх:
//! * `Granted` — разрешение есть;
//! * `Prompt` — браузер спросит при `requestPermission`;
//! * `Denied` — пользователь отказал или браузер заблокировал;
//! * `Unknown` — API недоступен (браузер без FSA, старый handle).
//!
//! Разрешения **не сохраняются между сессиями**: после перезагрузки
//! handle можно восстановить из IndexedDB, но `queryPermission` вернёт
//! `Prompt` — потребуется новый user gesture.

use wasm_bindgen::prelude::*;

use crate::fsa_choose_options::format_str;

/// Режим доступа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantMode {
    Read,
    ReadWrite,
}

impl GrantMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GrantMode::Read => return "read",
            GrantMode::ReadWrite => return "readwrite",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            GrantMode::Read => return "чтение",
            GrantMode::ReadWrite => return "чтение и запись",
        }
    }
}

/// Состояние разрешения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantState {
    Granted,
    Prompt,
    Denied,
    Unknown,
}

impl GrantState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "granted" => return GrantState::Granted,
            "prompt" => return GrantState::Prompt,
            "denied" => return GrantState::Denied,
            _ => return GrantState::Unknown,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            GrantState::Granted => return "granted",
            GrantState::Prompt => return "prompt",
            GrantState::Denied => return "denied",
            GrantState::Unknown => return "unknown",
        }
    }
    pub fn is_ready(self) -> bool {
        return matches!(self, GrantState::Granted);
    }
    pub fn needs_user_action(self) -> bool {
        return matches!(self, GrantState::Prompt);
    }
    pub fn is_blocked(self) -> bool {
        return matches!(self, GrantState::Denied);
    }
}

/// Классифицировать JS-ошибку как состояние разрешения.
/// `SecurityError` / `NotAllowedError` — не отказ пользователя, а
/// «браузер не дал права спросить» — это `Prompt`.
pub fn from_js_error(e: &JsValue) -> GrantState {
    let name: String = js_sys::Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    match name.as_str() {
        "SecurityError" | "NotAllowedError" => return GrantState::Prompt,
        "NotFoundError" | "InvalidStateError" => return GrantState::Unknown,
        _ => return GrantState::Unknown,
    }
}

/// Человекочитаемое объяснение для UI.
/// Отдельная функция: rust-analyzer не выводит `format!` в хвостовой
/// позиции `match`-arm как `String`.
pub fn explain(state: GrantState, mode: GrantMode) -> String {
    match state {
        GrantState::Granted => {
            let s: String = format_str(format_args!(
                "✓ Разрешение на {} уже есть. Можно работать.",
                mode.label(),
            ));
            return s;
        }
        GrantState::Prompt => {
            let s: String = format_str(format_args!(
                "⚠ Требуется запрос на {}. Нажмите «Запросить» — \
                 браузер покажет диалог.",
                mode.label(),
            ));
            return s;
        }
        GrantState::Denied => {
            let s: String = format_str(format_args!(
                "✗ В {} отказано. Снимите запрет в настройках \
                 сайта (значок замка в адресной строке) и перезагрузите страницу.",
                mode.label(),
            ));
            return s;
        }
        GrantState::Unknown => {
            let s: String = format_str(format_args!(
                "— Состояние разрешения на {} неизвестно: \
                 handle не поддерживает queryPermission.",
                mode.label(),
            ));
            return s;
        }
    }
}

/// Легенда состояний — общий текст для UI.
pub fn legend() -> String {
    let lines: Vec<String> = vec![
        "granted — можно работать".to_string(),
        "prompt  — нужен жест пользователя: requestPermission".to_string(),
        "denied  — пользователь отказал: снимите запрет в настройках сайта".to_string(),
        "unknown — API недоступен: handle без queryPermission".to_string(),
    ];
    return lines.join("\n");
}