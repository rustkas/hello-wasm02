//! Preflight: убедиться в разрешении до операции.
//!
//! Схема:
//! 1. `query(handle, mode)` — узнать состояние **без** диалога.
//! 2. Если `Prompt` — `request(handle, mode)`. Это может показать диалог.
//! 3. `ensure(handle, mode)` — комбинирует оба шага и возвращает
//!    осмысленный `GrantOutcome`, пригодный для UI.
//!
//! Правило проекта: перед вызовом метода через Reflect проверяем, что
//! приёмник — объект и метод определён. Иначе получим `TypeError` в
//! рантайме вместо осмысленного состояния.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::fsa_choose_options::format_str;
use crate::fsa_grant_state::{explain, GrantMode, GrantState};

/// Результат preflight.
#[derive(Debug)]
pub enum GrantOutcome {
    /// Разрешение есть, можно продолжать.
    Ready(GrantState),
    /// Нужно действие пользователя — например, диалог отменён.
    NeedsUserAction(GrantState, String),
    /// Заблокировано — `denied` или `Unknown`.
    Blocked(GrantState, String),
}

impl GrantOutcome {
    pub fn is_ready(&self) -> bool {
        return matches!(self, GrantOutcome::Ready(_));
    }
    pub fn state(&self) -> GrantState {
        match self {
            GrantOutcome::Ready(s) => return *s,
            GrantOutcome::NeedsUserAction(s, _) => return *s,
            GrantOutcome::Blocked(s, _) => return *s,
        }
    }
    pub fn message(&self) -> String {
        match self {
            GrantOutcome::Ready(s) => {
                let m: String = format_str(format_args!(
                    "✓ Разрешение: {}",
                    s.as_str(),
                ));
                return m;
            }
            GrantOutcome::NeedsUserAction(_, m) => return m.clone(),
            GrantOutcome::Blocked(_, m) => return m.clone(),
        }
    }
}

/// Приёмник Reflect-вызова — объект?
fn is_reflect_receiver(handle: &JsValue) -> bool {
    if handle.is_null() {
        return false;
    }
    if handle.is_undefined() {
        return false;
    }
    return handle.is_object();
}

/// Общая часть `query` и `request`.
async fn call_permission(
    handle: &JsValue,
    method: &str,
    mode: GrantMode,
) -> Result<GrantState, JsValue> {
    if !is_reflect_receiver(handle) {
        return Ok(GrantState::Unknown);
    }

    let fn_js = js_sys::Reflect::get(handle, &JsValue::from_str(method))?;
    if fn_js.is_undefined() {
        return Ok(GrantState::Unknown);
    }
    let func: js_sys::Function = fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не функция"))?;

    let descriptor = js_sys::Object::new();
    js_sys::Reflect::set(
        &descriptor,
        &JsValue::from_str("mode"),
        &JsValue::from_str(mode.as_str()),
    )?;

    let promise: js_sys::Promise = func
        .call1(handle, &descriptor)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let value = JsFuture::from(promise).await?;
    let s = value.as_string().unwrap_or_default();
    return Ok(GrantState::from_str(&s));
}

/// Узнать состояние без диалога.
pub async fn query(handle: &JsValue, mode: GrantMode) -> Result<GrantState, JsValue> {
    return call_permission(handle, "queryPermission", mode).await;
}

/// Запросить разрешение. Может показать диалог — требует user gesture.
/// Идемпотентен: если состояние уже `granted` или `denied`,
/// вернёт текущее без диалога.
pub async fn request(handle: &JsValue, mode: GrantMode) -> Result<GrantState, JsValue> {
    return call_permission(handle, "requestPermission", mode).await;
}

/// Убедиться в разрешении перед операцией.
pub async fn ensure(handle: &JsValue, mode: GrantMode) -> Result<GrantOutcome, JsValue> {
    let queried = query(handle, mode).await?;

    if queried.is_ready() {
        return Ok(GrantOutcome::Ready(queried));
    }
    if queried == GrantState::Unknown {
        return Ok(GrantOutcome::Blocked(
            queried,
            explain(GrantState::Unknown, mode),
        ));
    }
    if queried.is_blocked() {
        return Ok(GrantOutcome::Blocked(
            queried,
            explain(GrantState::Denied, mode),
        ));
    }

    // queried == Prompt.
    let requested = request(handle, mode).await?;
    if requested.is_ready() {
        return Ok(GrantOutcome::Ready(requested));
    }
    if requested.needs_user_action() {
        let msg: String = format_str(format_args!(
            "Запрос на {} не завершён. Нажмите кнопку ещё раз и подтвердите диалог.",
            mode.label(),
        ));
        return Ok(GrantOutcome::NeedsUserAction(requested, msg));
    }
    return Ok(GrantOutcome::Blocked(
        requested,
        explain(requested, mode),
    ));
}