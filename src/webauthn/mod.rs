//! Глава 41: Web Authentication — WebAuthn и passkeys.
//!
//! WebAuthn — расширение Credential Management API, позволяющее
//! создавать и использовать публичные ключи для аутентификации
//! без пароля.
//!
//! ПРАВИЛО: `web-sys` не экспортирует `CredentialsContainer`,
//! `CredentialCreationOptions`, `CredentialRequestOptions` без
//! флага `--cfg=web_sys_unstable_apis`. Мы вызываем JS-методы
//! через `Reflect::get` + `Function::call*` — это универсальный
//! способ работать с любым JS API.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

pub mod capability;
pub mod register;
pub mod authenticate;
pub mod encoding;
pub mod model;
pub mod parse; 

use wasm_bindgen::JsValue;
use crate::format_str::format_str;

/// Универсальная ошибка WebAuthn-операций.
#[derive(Debug, Clone)]
pub struct WebAuthnError {
    pub kind: WebAuthnErrorKind,
    pub message: String,
}

/// Классификация ошибок — важна для UI: одна и та же
/// ошибка может быть «пользователь отменил» или «аппаратная
/// проблема», и реакция разная.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebAuthnErrorKind {
    Unsupported,
    Cancelled,
    Timeout,
    InvalidState,
    Config,
    Other,
}

impl WebAuthnError {
    pub fn new(kind: WebAuthnErrorKind, msg: String) -> Self {
        return Self { kind, message: msg };
    }

    /// Преобразует DOMException в наш тип.
    ///
    /// `NotAllowedError` покрывает два случая: отмена пользователем
    /// и таймаут. Различаем их по сообщению.
    pub fn from_js(value: &JsValue) -> Self {
        let name = js_sys::Reflect::get(value, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let msg = js_sys::Reflect::get(value, &JsValue::from_str("message"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| format_str(format_args!("{value:?}")));

        let kind = match name.as_str() {
            "NotAllowedError" => {
                if msg.to_lowercase().contains("timeout") {
                    WebAuthnErrorKind::Timeout
                } else {
                    WebAuthnErrorKind::Cancelled
                }
            }
            "InvalidStateError" => WebAuthnErrorKind::InvalidState,
            "SecurityError" | "NotSupportedError" => WebAuthnErrorKind::Config,
            _ => WebAuthnErrorKind::Other,
        };
        return Self { kind, message: msg };
    }

    /// Человекочитаемое объяснение для UI.
    pub fn user_hint(&self) -> &'static str {
        return match self.kind {
            WebAuthnErrorKind::Unsupported => {
                "Браузер не поддерживает WebAuthn. Попробуйте Chrome, Firefox или Safari последних версий."
            }
            WebAuthnErrorKind::Cancelled => {
                "Операция отменена. Возможно, вы закрыли диалог или не подтвердили действие."
            }
            WebAuthnErrorKind::Timeout => {
                "Время ожидания истекло. Попробуйте ещё раз — возможно, аутентификатор не ответил вовремя."
            }
            WebAuthnErrorKind::InvalidState => {
                "Этот аутентификатор уже зарегистрирован для данного аккаунта. Используйте другой или войдите."
            }
            WebAuthnErrorKind::Config => {
                "Проблема с конфигурацией Relying Party. Проверьте, что origin совпадает с настройками сервера."
            }
            WebAuthnErrorKind::Other => {
                "Произошла неизвестная ошибка. Подробности в консоли."
            }
        };
    }
}

impl std::fmt::Display for WebAuthnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.message);
    }
}

/// Возвращает `navigator.credentials` как `JsValue`.
///
/// Не используем типизированный `web_sys::CredentialsContainer`:
/// этот тип требует `--cfg=web_sys_unstable_apis`. Работаем с
/// сырым JS-объектом через `Reflect::get` и `Function::call*`.
pub fn credentials_container() -> Result<JsValue, WebAuthnError> {
    let window = web_sys::window()
        .ok_or_else(|| WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "no window".to_string(),
        ))?;
    let navigator = window.navigator();
    let credentials = js_sys::Reflect::get(
        &navigator,
        &JsValue::from_str("credentials"),
    ).map_err(|e| WebAuthnError::from_js(&e))?;
    if credentials.is_undefined() || credentials.is_null() {
        return Err(WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "navigator.credentials unavailable".to_string(),
        ));
    }
    return Ok(credentials);
}