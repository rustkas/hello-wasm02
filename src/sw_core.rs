//! Service Worker: регистрация, обновление, удаление.
//! Автономный модуль — не зависит от других модулей проекта.
//!
//! ВАЖНО (web-sys 0.3.105):
//!   • Navigator::service_worker() возвращает ServiceWorkerContainer напрямую.
//!     Отсутствие API (не-HTTPS) проверяем через Reflect.
//!   • register() возвращает Promise без Result.
//!   • ready() / update() / unregister() возвращают Result<Promise, JsValue> — [Throws].

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ServiceWorkerContainer, ServiceWorkerRegistration};

pub const SW_PATH: &str = "/sw.js";

fn container() -> Result<ServiceWorkerContainer, JsValue> {
    let nav = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .navigator();

    // `serviceWorker` есть в объекте navigator только в secure context.
    let value = js_sys::Reflect::get(&nav, &JsValue::from_str("serviceWorker"))
        .map_err(|e| JsValue::from_str(&format!("Reflect: {e:?}")))?;

    if value.is_undefined() || value.is_null() {
        return Err(JsValue::from_str(
            "Service Worker не поддерживается (нужен HTTPS или localhost)",
        ));
    }

    Ok(value.unchecked_into::<ServiceWorkerContainer>())
}

/// Регистрирует Service Worker по пути `SW_PATH`.
pub async fn register() -> Result<ServiceWorkerRegistration, JsValue> {
    // register() возвращает Promise напрямую.
    let promise = container()?.register(SW_PATH);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await register: {e:?}")))?;
    Ok(value.unchecked_into::<ServiceWorkerRegistration>())
}

/// Ждёт, пока регистрация станет активной.
pub async fn ready() -> Result<ServiceWorkerRegistration, JsValue> {
    // ready() возвращает Result<Promise, JsValue> — [Throws].
    let promise = container()?
        .ready()
        .map_err(|e| JsValue::from_str(&format!("ready: {e:?}")))?;
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await ready: {e:?}")))?;
    Ok(value.unchecked_into::<ServiceWorkerRegistration>())
}

/// Принудительно проверяет обновление SW.
pub async fn update(reg: &ServiceWorkerRegistration) -> Result<(), JsValue> {
    // update() возвращает Result<Promise, JsValue> — [Throws].
    let promise = reg
        .update()
        .map_err(|e| JsValue::from_str(&format!("update: {e:?}")))?;
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("await update: {e:?}")))
}

/// Удаляет регистрацию. Идемпотентно.
pub async fn unregister(reg: &ServiceWorkerRegistration) -> Result<bool, JsValue> {
    // unregister() возвращает Result<Promise, JsValue> — [Throws].
    let promise = reg
        .unregister()
        .map_err(|e| JsValue::from_str(&format!("unregister: {e:?}")))?;
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await unregister: {e:?}")))?;
    Ok(value.as_bool().unwrap_or(false))
}

/// Проверяет, управляет ли SW текущей страницей (активен ли перехват fetch).
pub fn has_controller() -> bool {
    js_sys::Reflect::get(
        &web_sys::window()
            .map(|w| w.navigator())
            .map(JsValue::from)
            .unwrap_or(JsValue::UNDEFINED),
        &JsValue::from_str("serviceWorker"),
    )
    .ok()
    .filter(|v| !v.is_undefined() && !v.is_null())
    .and_then(|v| {
        let c: ServiceWorkerContainer = v.unchecked_into();
        c.controller()
    })
    .is_some()
}

/// Описание текущего состояния регистрации.
#[derive(Debug, Clone, PartialEq)]
pub struct SwStatus {
    pub scope: String,
    pub has_active: bool,
    pub has_waiting: bool,
    pub has_installing: bool,
}

/// Возвращает снимок состояния регистрации.
pub fn status(reg: &ServiceWorkerRegistration) -> SwStatus {
    SwStatus {
        scope: reg.scope(),
        has_active: reg.active().is_some(),
        has_waiting: reg.waiting().is_some(),
        has_installing: reg.installing().is_some(),
    }
}