//! Проверка возможностей браузера и аутентификатора.
//!
//! Все методы вызываются через `Reflect::get` + `Function::call*`
//! из-за unstable-статуса WebAuthn-типов в web-sys.

use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::WebAuthnError;
use super::WebAuthnErrorKind;

/// Проверяет, доступен ли WebAuthn в текущем браузере.
pub fn is_available() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let pkc = js_sys::Reflect::get(&window, &JsValue::from_str("PublicKeyCredential"))
        .ok()
        .unwrap_or(JsValue::UNDEFINED);
    return !pkc.is_undefined() && !pkc.is_null();
}

/// Возвращает `window.PublicKeyCredential` как `JsValue`.
fn pkc_object() -> Result<JsValue, WebAuthnError> {
    if !is_available() {
        return Err(WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "PublicKeyCredential not available".to_string(),
        ));
    }
    let window = web_sys::window().ok_or_else(|| {
        WebAuthnError::new(WebAuthnErrorKind::Unsupported, "no window".to_string())
    })?;
    let pkc = js_sys::Reflect::get(&window, &JsValue::from_str("PublicKeyCredential"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return Ok(pkc);
}

/// Вызывает статический метод `PublicKeyCredential.<name>()` без аргументов.
async fn call_pkc_async(method: &str) -> Result<bool, WebAuthnError> {
    let pkc = pkc_object()?;
    let func_js = js_sys::Reflect::get(&pkc, &JsValue::from_str(method))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    if func_js.is_undefined() || func_js.is_null() {
        return Err(WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            format_str(format_args!("{method}: not defined")),
        ));
    }
    let func: js_sys::Function = func_js.dyn_into().map_err(|_| {
        WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            format_str(format_args!("{method}: not a function")),
        )
    })?;
    let promise_value = func
        .call0(&pkc)
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let promise: js_sys::Promise = promise_value.dyn_into().map_err(|_| {
        WebAuthnError::new(
            WebAuthnErrorKind::Other,
            format_str(format_args!("{method}: not a promise")),
        )
    })?;
    let result = JsFuture::from(promise)
        .await
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return Ok(result.as_bool().unwrap_or(false));
}

use crate::format_str::format_str;

/// Проверяет наличие platform authenticator (Touch ID, Windows Hello).
pub async fn has_platform_authenticator() -> Result<bool, WebAuthnError> {
    return call_pkc_async("isUserVerifyingPlatformAuthenticatorAvailable").await;
}

/// Проверяет поддержку conditional mediation (autofill passkey).
///
/// Возвращает `Ok(false)`, если метод отсутствует (старые браузеры) —
/// это не ошибка, а отсутствие возможности.
pub async fn is_conditional_mediation_available() -> Result<bool, WebAuthnError> {
    let pkc = match pkc_object() {
        Ok(p) => p,
        Err(e) => return Err(e),
    };
    let func_js = js_sys::Reflect::get(&pkc, &JsValue::from_str("isConditionalMediationAvailable"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    if func_js.is_undefined() || func_js.is_null() {
        return Ok(false);
    }
    let func: js_sys::Function = match func_js.dyn_into() {
        Ok(f) => f,
        Err(_) => return Ok(false),
    };
    let promise_value = match func.call0(&pkc) {
        Ok(v) => v,
        Err(e) => return Err(WebAuthnError::from_js(&e)),
    };
    let promise: js_sys::Promise = match promise_value.dyn_into() {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    let result = JsFuture::from(promise)
        .await
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return Ok(result.as_bool().unwrap_or(false));
}

/// Возвращает строку с описанием возможностей — для отображения в UI.
pub async fn capability_summary() -> String {
    let mut parts: Vec<String> = Vec::new();
    if is_available() {
        parts.push("WebAuthn: ✅".to_string());
    } else {
        parts.push("WebAuthn: ❌".to_string());
        return parts.join(" · ");
    }
    match has_platform_authenticator().await {
        Ok(true) => parts.push("platform: ✅".to_string()),
        Ok(false) => parts.push("platform: ❌".to_string()),
        Err(_) => parts.push("platform: ?".to_string()),
    }
    match is_conditional_mediation_available().await {
        Ok(true) => parts.push("conditional: ✅".to_string()),
        Ok(false) => parts.push("conditional: ❌".to_string()),
        Err(_) => parts.push("conditional: ?".to_string()),
    }
    return parts.join(" · ");
}