//! Разрешения Clipboard API через Permissions API.
//!
//! Clipboard — одно из немногих API, где разрешения запрашиваются
//! **автоматически** при первом вызове `read`/`write`, а не через
//! явный `request()`. Однако `navigator.permissions.query()` позволяет
//! узнать текущее состояние до вызова:
//!
//!   • `granted` — разрешение выдано, операция пройдёт без диалога.
//!   • `prompt` — браузер спросит при первом вызове.
//!   • `denied` — операция отклонена (пользователь или политика).
//!
//! Для `clipboard-write` в Chrome и Firefox разрешение выдаётся
//! автоматически при наличии user gesture — `prompt` не показывается.
//! Для `clipboard-read` Chrome показывает диалог.
//!
//! ПРАВИЛО: `Permissions.query()` возвращает `Promise<PermissionStatus>`.
//! `PermissionStatus.state` — строка.
//!
//! ПРАВИЛО: не маскируйте ошибку `query()` под `Unknown`. Если браузер
//! не поддерживает конкретное имя разрешения — покажите причину,
//! иначе UI превращается в «неизвестно, и непонятно почему».

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Состояние разрешения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermState {
    Granted,
    Prompt,
    Denied,
    Unknown,
}

impl PermState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "granted" => PermState::Granted,
            "prompt" => PermState::Prompt,
            "denied" => PermState::Denied,
            _ => PermState::Unknown,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            PermState::Granted => "granted",
            PermState::Prompt => "prompt",
            PermState::Denied => "denied",
            PermState::Unknown => "unknown",
        }
    }
    pub fn short(&self) -> &'static str {
        match self {
            PermState::Granted => "разрешено",
            PermState::Prompt => "будет запрошено",
            PermState::Denied => "запрещено",
            PermState::Unknown => "неизвестно",
        }
    }
}

/// Результат запроса разрешения: состояние или причина отказа.
#[derive(Debug, Clone)]
pub enum QueryOutcome {
    State(PermState),
    /// `Permissions.query()` бросил исключение: имя не поддерживается,
    /// не тот контекст, нет secure context.
    Failed(String),
}

impl QueryOutcome {
    pub fn state_or_unknown(&self) -> PermState {
        match self {
            QueryOutcome::State(s) => *s,
            QueryOutcome::Failed(_) => PermState::Unknown,
        }
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            QueryOutcome::State(_) => None,
            QueryOutcome::Failed(s) => Some(s.as_str()),
        }
    }
}

/// true, если Permissions API доступен.
pub fn is_available() -> bool {
    return permissions().is_ok();
}

fn permissions() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let nav = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))
        .map_err(|e| JsValue::from_str(&format!("Reflect navigator: {e:?}")))?;
    let perms = js_sys::Reflect::get(&nav, &JsValue::from_str("permissions"))
        .map_err(|e| JsValue::from_str(&format!("Reflect permissions: {e:?}")))?;
    if perms.is_undefined() || perms.is_null() {
        return Err(JsValue::from_str("Permissions API недоступен"));
    }
    Ok(perms)
}

/// Запрашивает текущее состояние разрешения. Маскирует ошибку под `Unknown`.
///
/// Для диагностики используйте `query_strict` — она возвращает `QueryOutcome`
/// с причиной отказа.
pub async fn query(perm_name: &str) -> Result<PermState, JsValue> {
    match query_inner(perm_name).await {
        Ok(s) => Ok(s),
        Err(msg) => Err(JsValue::from_str(&msg)),
    }
}

/// Строгий вариант `query`: не маскирует ошибку под `Unknown`,
/// возвращает причину отказа.
pub async fn query_strict(perm_name: &str) -> QueryOutcome {
    match query_inner(perm_name).await {
        Ok(s) => QueryOutcome::State(s),
        Err(msg) => QueryOutcome::Failed(msg),
    }
}

async fn query_inner(perm_name: &str) -> Result<PermState, String> {
    let perms = permissions()
        .map_err(|e| crate::clip_core::js_error_message(&e))?;
    let query_fn = method(&perms, "query")
        .map_err(|e| crate::clip_core::js_error_message(&e))?;

    let desc = js_sys::Object::new();
    js_sys::Reflect::set(&desc, &JsValue::from_str("name"), &JsValue::from_str(perm_name))
        .map_err(|e| format!("set name: {e:?}"))?;

    let promise = query_fn
        .call1(&perms, &desc)
        .map_err(|e| format!("query: {e:?}"))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| "query не вернул Promise".to_string())?;

    let status = JsFuture::from(promise).await.map_err(|e| {
        let msg = crate::clip_core::js_error_message(&e);
        // Firefox бросает TypeError: "'clipboard-read' (value of 'name'
        // member of PermissionDescriptor) is not a valid value for
        // enumeration PermissionName." Это не ошибка пользователя —
        // просто браузер не знает такого разрешения. Даём короткую
        // формулировку вместо пятисотсимвольного стектрейса.
        if msg.contains("not a valid value")
            || msg.contains("not a valid permission")
        {
            return format!("браузер не поддерживает '{perm_name}' в Permissions API");
        }
        return msg;
    })?;

    let state = js_sys::Reflect::get(&status, &JsValue::from_str("state"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(PermState::from_str(&state))
}

/// Запрашивает оба разрешения одним вызовом. Маскирует ошибки.
pub async fn query_both() -> (PermState, PermState) {
    let read = query("clipboard-read").await.unwrap_or(PermState::Unknown);
    let write = query("clipboard-write").await.unwrap_or(PermState::Unknown);
    (read, write)
}

/// Снимок состояния Clipboard-разрешений.
#[derive(Debug, Clone)]
pub struct ClipPermsInfo {
    pub api_available: bool,
    pub perms_api_available: bool,
    pub read: PermState,
    pub write: PermState,
}

/// Возвращает снимок разрешений. Никогда не паникует: если
/// Permissions API недоступен — `Unknown` для всех полей.
pub async fn probe() -> ClipPermsInfo {
    let api_available = crate::clip_core::is_available();
    let perms_available = is_available();

    if !perms_available {
        return ClipPermsInfo {
            api_available,
            perms_api_available: false,
            read: PermState::Unknown,
            write: PermState::Unknown,
        };
    }
    let (read, write) = query_both().await;
    ClipPermsInfo {
        api_available,
        perms_api_available: true,
        read,
        write,
    }
}

/// Расширенный снимок: содержит причины, если запрос провалился.
#[derive(Debug, Clone)]
pub struct ClipPermsProbe {
    pub api_available: bool,
    pub perms_api_available: bool,
    pub read: QueryOutcome,
    pub write: QueryOutcome,
}

/// Строгий probe: не маскирует ошибки под `Unknown`.
pub async fn probe_strict() -> ClipPermsProbe {
    let api_available = crate::clip_core::is_available();
    if !is_available() {
        return ClipPermsProbe {
            api_available,
            perms_api_available: false,
            read: QueryOutcome::Failed("Permissions API недоступен".into()),
            write: QueryOutcome::Failed("Permissions API недоступен".into()),
        };
    }
    let read = query_strict("clipboard-read").await;
    let write = query_strict("clipboard-write").await;
    ClipPermsProbe {
        api_available,
        perms_api_available: true,
        read,
        write,
    }
}



fn method(obj: &JsValue, name: &str) -> Result<js_sys::Function, JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("Reflect {name}: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{name} не функция")))
}