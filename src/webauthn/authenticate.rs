//! Аутентификация через `navigator.credentials.get()`.
//!
//! Поддерживает два режима:
//!   - обычный (`allowCredentials` со списком ID);
//!   - conditional (`mediation: "conditional"`) — passkey
//!     появляется прямо в autofill поля логина.

use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::WebAuthnError;
use super::WebAuthnErrorKind;
use super::encoding::{bytes_to_uint8array, jsvalue_to_bytes};
use super::model::{AuthResult, RpConfig};

/// Запускает `navigator.credentials.get()` со списком credential.
pub async fn authenticate_passkey(
    config: &RpConfig,
    challenge: &[u8],
    allow_credentials: &[Vec<u8>],
) -> Result<AuthResult, WebAuthnError> {
    let credentials = super::credentials_container()?;
    let options = build_request_options(config, challenge, allow_credentials)?;
    return call_get(&credentials, &options).await;
}

/// D2: conditional mediation — passkey в autofill.
///
/// `allowCredentials` пуст, `mediation: "conditional"`. Браузер
/// сам подставит подходящий passkey.
pub async fn authenticate_conditional(
    config: &RpConfig,
    challenge: &[u8],
) -> Result<AuthResult, WebAuthnError> {
    let credentials = super::credentials_container()?;
    let options = build_conditional_options(config, challenge)?;
    return call_get(&credentials, &options).await;
}

async fn call_get(
    credentials: &JsValue,
    options: &JsValue,
) -> Result<AuthResult, WebAuthnError> {
    let get_fn_js = js_sys::Reflect::get(credentials, &JsValue::from_str("get"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    if get_fn_js.is_undefined() || get_fn_js.is_null() {
        return Err(WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "credentials.get not defined".to_string(),
        ));
    }
    let get_fn: js_sys::Function = get_fn_js.dyn_into().map_err(|_| {
        WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "credentials.get is not a function".to_string(),
        )
    })?;
    let promise_value = get_fn
        .call1(credentials, options)
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let promise: js_sys::Promise = promise_value.dyn_into().map_err(|_| {
        WebAuthnError::new(
            WebAuthnErrorKind::Other,
            "get: not a promise".to_string(),
        )
    })?;
    let credential = JsFuture::from(promise)
        .await
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return parse_assertion(&credential);
}

fn build_request_options(
    config: &RpConfig,
    challenge: &[u8],
    allow_credentials: &[Vec<u8>],
) -> Result<JsValue, WebAuthnError> {
    let public_key = js_sys::Object::new();

    set_prop(&public_key, "challenge", &bytes_to_uint8array(challenge))?;
    set_prop(&public_key, "timeout", &JsValue::from_f64(60_000.0))?;
    set_prop(&public_key, "rpId", &JsValue::from_str(&config.rp_id))?;
    set_prop(
        &public_key,
        "userVerification",
        &JsValue::from_str("preferred"),
    )?;

    let allow = js_sys::Array::new();
    for cred_id in allow_credentials {
        let cred = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &cred,
            &JsValue::from_str("type"),
            &JsValue::from_str("public-key"),
        );
        let _ = js_sys::Reflect::set(
            &cred,
            &JsValue::from_str("id"),
            &bytes_to_uint8array(cred_id),
        );
        allow.push(&cred);
    }
    set_prop(&public_key, "allowCredentials", &allow)?;

    let options = js_sys::Object::new();
    set_prop(&options, "publicKey", &public_key)?;
    return Ok(options.into());
}

fn build_conditional_options(
    config: &RpConfig,
    challenge: &[u8],
) -> Result<JsValue, WebAuthnError> {
    let public_key = js_sys::Object::new();
    set_prop(&public_key, "challenge", &bytes_to_uint8array(challenge))?;
    set_prop(&public_key, "timeout", &JsValue::from_f64(60_000.0))?;
    set_prop(&public_key, "rpId", &JsValue::from_str(&config.rp_id))?;
    set_prop(
        &public_key,
        "userVerification",
        &JsValue::from_str("preferred"),
    )?;

    let options = js_sys::Object::new();
    set_prop(&options, "publicKey", &public_key)?;
    // mediation: "conditional" — включает autofill-режим.
    set_prop(&options, "mediation", &JsValue::from_str("conditional"))?;
    return Ok(options.into());
}

fn set_prop(
    obj: &js_sys::Object,
    key: &str,
    value: &JsValue,
) -> Result<(), WebAuthnError> {
    js_sys::Reflect::set(obj, &JsValue::from_str(key), value)
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return Ok(());
}

fn parse_assertion(credential: &JsValue) -> Result<AuthResult, WebAuthnError> {
    let raw_id_val = js_sys::Reflect::get(credential, &JsValue::from_str("rawId"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let raw_id = jsvalue_to_bytes(&raw_id_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    let response = js_sys::Reflect::get(credential, &JsValue::from_str("response"))
        .map_err(|e| WebAuthnError::from_js(&e))?;

    let client_data_val = js_sys::Reflect::get(&response, &JsValue::from_str("clientDataJSON"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let client_data = jsvalue_to_bytes(&client_data_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    let auth_data_val = js_sys::Reflect::get(&response, &JsValue::from_str("authenticatorData"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let auth_data = jsvalue_to_bytes(&auth_data_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    let sig_val = js_sys::Reflect::get(&response, &JsValue::from_str("signature"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let signature = jsvalue_to_bytes(&sig_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    let user_handle = js_sys::Reflect::get(&response, &JsValue::from_str("userHandle"))
        .ok()
        .filter(|v| !v.is_null() && !v.is_undefined())
        .and_then(|v| jsvalue_to_bytes(&v).ok());

    return Ok(AuthResult {
        raw_id,
        client_data_json: client_data,
        authenticator_data: auth_data,
        signature,
        user_handle,
    });
}