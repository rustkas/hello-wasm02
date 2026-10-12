//! Регистрация passkey через `navigator.credentials.create()`.
//!
//! Все опции собираются как обычные JS-объекты (`js_sys::Object`),
//! а вызов идёт через `Reflect::get("create")` + `Function::call1`.
//! Это обходит проблему отсутствия типизированных WebAuthn-обёрток
//! в стабильном web-sys.

use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::WebAuthnError;
use super::WebAuthnErrorKind;
use super::encoding::{bytes_to_uint8array, jsvalue_to_bytes};
use super::model::{RegistrationResult, RpConfig};

/// Пользователь для регистрации.
pub struct RegistrationUser<'a> {
    /// Уникальный ID (в базе данных) — 16–64 байта.
    pub id: &'a [u8],
    /// Отображаемое имя (например, "Alice").
    pub name: &'a str,
    /// Полное имя (например, "Alice Doe").
    pub display_name: &'a str,
}

/// Собирает `{ publicKey: {...} }` и вызывает `credentials.create()`.
pub async fn register_passkey(
    config: &RpConfig,
    user: &RegistrationUser<'_>,
    challenge: &[u8],
) -> Result<RegistrationResult, WebAuthnError> {
    let credentials = super::credentials_container()?;
    let options = build_creation_options(config, user, challenge)?;

    let create_fn_js = js_sys::Reflect::get(&credentials, &JsValue::from_str("create"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    if create_fn_js.is_undefined() || create_fn_js.is_null() {
        return Err(WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "credentials.create not defined".to_string(),
        ));
    }
    let create_fn: js_sys::Function = create_fn_js.dyn_into().map_err(|_| {
        WebAuthnError::new(
            WebAuthnErrorKind::Unsupported,
            "credentials.create is not a function".to_string(),
        )
    })?;
    let promise_value = create_fn
        .call1(&credentials, &options)
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let promise: js_sys::Promise = promise_value.dyn_into().map_err(|_| {
        WebAuthnError::new(
            WebAuthnErrorKind::Other,
            "create: not a promise".to_string(),
        )
    })?;
    let credential = JsFuture::from(promise)
        .await
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return parse_creation_result(&credential);
}

/// Собирает `CredentialCreationOptions` как JS-объект.
fn build_creation_options(
    config: &RpConfig,
    user: &RegistrationUser<'_>,
    challenge: &[u8],
) -> Result<JsValue, WebAuthnError> {
    let public_key = js_sys::Object::new();

    // rp: { id, name }
    let rp = js_sys::Object::new();
    set_prop(&rp, "id", &JsValue::from_str(&config.rp_id))?;
    set_prop(&rp, "name", &JsValue::from_str(&config.rp_name))?;
    set_prop(&public_key, "rp", &rp)?;

    // user: { id, name, displayName }
    let user_obj = js_sys::Object::new();
    set_prop(&user_obj, "id", &bytes_to_uint8array(user.id))?;
    set_prop(&user_obj, "name", &JsValue::from_str(user.name))?;
    set_prop(&user_obj, "displayName", &JsValue::from_str(user.display_name))?;
    set_prop(&public_key, "user", &user_obj)?;

    // challenge: Uint8Array
    set_prop(&public_key, "challenge", &bytes_to_uint8array(challenge))?;

    // pubKeyCredParams: ES256 = -7, RS256 = -257
    let params = js_sys::Array::new();
    params.push(&make_cred_param("public-key", -7));
    params.push(&make_cred_param("public-key", -257));
    set_prop(&public_key, "pubKeyCredParams", &params)?;

    // timeout
    set_prop(&public_key, "timeout", &JsValue::from_f64(60_000.0))?;

    // authenticatorSelection
    let selection = js_sys::Object::new();
    set_prop(
        &selection,
        "authenticatorAttachment",
        &JsValue::from_str("platform"),
    )?;
    set_prop(
        &selection,
        "userVerification",
        &JsValue::from_str("preferred"),
    )?;
    set_prop(
        &selection,
        "residentKey",
        &JsValue::from_str("preferred"),
    )?;
    set_prop(&public_key, "authenticatorSelection", &selection)?;

    // attestation
    set_prop(&public_key, "attestation", &JsValue::from_str("none"))?;

    // Финальный options: { publicKey: {...} }
    let options = js_sys::Object::new();
    set_prop(&options, "publicKey", &public_key)?;
    return Ok(options.into());
}

/// Хелпер: `Reflect::set` с понятной ошибкой.
fn set_prop(
    obj: &js_sys::Object,
    key: &str,
    value: &JsValue,
) -> Result<(), WebAuthnError> {
    js_sys::Reflect::set(obj, &JsValue::from_str(key), value)
        .map_err(|e| WebAuthnError::from_js(&e))?;
    return Ok(());
}

/// Создаёт `{ type: "public-key", alg: -7 }`.
fn make_cred_param(type_str: &str, alg: i32) -> JsValue {
    let p = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &p,
        &JsValue::from_str("type"),
        &JsValue::from_str(type_str),
    );
    let _ = js_sys::Reflect::set(
        &p,
        &JsValue::from_str("alg"),
        &JsValue::from_f64(alg as f64),
    );
    return p.into();
}

/// Разбирает результат `create()`.
fn parse_creation_result(
    credential: &JsValue,
) -> Result<RegistrationResult, WebAuthnError> {
    // rawId
    let raw_id_val = js_sys::Reflect::get(credential, &JsValue::from_str("rawId"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let raw_id = jsvalue_to_bytes(&raw_id_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    // response
    let response = js_sys::Reflect::get(credential, &JsValue::from_str("response"))
        .map_err(|e| WebAuthnError::from_js(&e))?;

    // clientDataJSON
    let client_data_val = js_sys::Reflect::get(&response, &JsValue::from_str("clientDataJSON"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let client_data = jsvalue_to_bytes(&client_data_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    // attestationObject
    let attestation_val = js_sys::Reflect::get(&response, &JsValue::from_str("attestationObject"))
        .map_err(|e| WebAuthnError::from_js(&e))?;
    let attestation = jsvalue_to_bytes(&attestation_val).map_err(|e| {
        WebAuthnError::new(WebAuthnErrorKind::Other, e)
    })?;

    // authenticatorAttachment (опционально)
    let attachment = js_sys::Reflect::get(
        credential,
        &JsValue::from_str("authenticatorAttachment"),
    )
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_else(|| "unknown".to_string());

    return Ok(RegistrationResult {
        raw_id,
        client_data_json: client_data,
        attestation_object: attestation,
        attachment,
    });
}