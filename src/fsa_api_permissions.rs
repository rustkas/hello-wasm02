//! Permissions API для FSA.
//!
//! FSA использует не navigator.permissions, а методы handle'а:
//! handle.queryPermission({mode}) и handle.requestPermission({mode}).
//! Доступны режимы "read" и "readwrite".
//!
//! Разрешения не сохраняются между сессиями: после перезагрузки
//! queryPermission вернёт "prompt".

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
}

impl PermissionState {
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionState::Granted => "granted",
            PermissionState::Denied => "denied",
            PermissionState::Prompt => "prompt",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "granted" => PermissionState::Granted,
            "denied" => PermissionState::Denied,
            _ => PermissionState::Prompt,
        }
    }
}

fn build_descriptor(mode: &str) -> Result<JsValue, JsValue> {
    let descriptor = js_sys::Object::new();
    js_sys::Reflect::set(
        &descriptor,
        &JsValue::from_str("mode"),
        &JsValue::from_str(mode),
    )?;
    Ok(descriptor.into())
}

async fn call_permission(
    handle: &JsValue,
    method: &str,
    mode: &str,
) -> Result<PermissionState, JsValue> {
    let fn_js = js_sys::Reflect::get(handle, &JsValue::from_str(method))?;
    if fn_js.is_undefined() {
        return Err(JsValue::from_str(&format!("{method} недоступен")));
    }
    let func: js_sys::Function = fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не функция"))?;
    let descriptor = build_descriptor(mode)?;
    let promise: js_sys::Promise = func
        .call1(handle, &descriptor)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let value = JsFuture::from(promise).await?;
    let s = value.as_string().unwrap_or_default();
    Ok(PermissionState::from_str(&s))
}

/// queryPermission({mode}) — без запроса к пользователю.
pub async fn query_permission(
    handle: &JsValue,
    mode: &str,
) -> Result<PermissionState, JsValue> {
    call_permission(handle, "queryPermission", mode).await
}

/// requestPermission({mode}) — может показать диалог.
pub async fn request_permission(
    handle: &JsValue,
    mode: &str,
) -> Result<PermissionState, JsValue> {
    call_permission(handle, "requestPermission", mode).await
}