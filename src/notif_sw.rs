//! Service Worker notifications: `registration.showNotification()`.
//!
//! Ключевое отличие от page-notification: попап, показанный из SW,
//! **живёт вне вкладки**. Закрыл вкладку — попап остался. Клик по нему
//! приходит в SW событие `notificationclick`, а не в страницу.
//!
//! Это основа push-уведомлений: сервер шлёт push на SW, SW показывает
//! уведомление, пользователь кликает, SW получает управление.
//!
//! ПРАВИЛО: регистрация SW требует отдельный JS-файл (`web/sw.js`).
//! Путь относительный, scope ограничен директорией файла.
//!
//! ПРАВИЛО: `showNotification()` возвращает `Promise<void>`.
//!
//! ПРАВИЛО: `JsFuture::from(promise).await?` — результат всегда
//! `JsValue`. Явная аннотация `let result: JsValue` помогает
//! rust-analyzer вывести тип на host-таргете.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::notif_core::NotifyOptions;

// ============================================================
//                    ДОСТУП
// ============================================================

/// Возвращает `navigator.serviceWorker` — или ошибку.
fn service_worker() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let nav = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))
        .map_err(|e| JsValue::from_str(&format!("Reflect navigator: {e:?}")))?;
    let sw = js_sys::Reflect::get(&nav, &JsValue::from_str("serviceWorker"))
        .map_err(|e| JsValue::from_str(&format!("Reflect serviceWorker: {e:?}")))?;
    if sw.is_undefined() || sw.is_null() {
        return Err(JsValue::from_str(
            "Service Worker API недоступен (нужен secure context)",
        ));
    }
    return Ok(sw);
}

/// true, если Service Worker API доступен.
pub fn is_supported() -> bool {
    return service_worker().is_ok();
}

// ============================================================
//                    РЕГИСТРАЦИЯ
// ============================================================

/// Регистрирует Service Worker по указанному URL.
///
/// Возвращает `ServiceWorkerRegistration` (как `JsValue`). Идемпотентно:
/// повторный вызов с тем же URL возвращает ту же регистрацию.
pub async fn register(url: &str) -> Result<JsValue, JsValue> {
    let sw = service_worker()?;
    let register_fn = js_sys::Reflect::get(&sw, &JsValue::from_str("register"))
        .map_err(|e| JsValue::from_str(&format!("Reflect register: {e:?}")))?;
    let register_fn: js_sys::Function = register_fn
        .dyn_into()
        .map_err(|_| JsValue::from_str("register не функция"))?;
    let promise = register_fn
        .call1(&sw, &JsValue::from_str(url))
        .map_err(|e| JsValue::from_str(&format!("register: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("register не вернул Promise"))?;
    JsFuture::from(promise).await
}

/// Возвращает существующую регистрацию, если есть.
///
/// `getRegistration()` резолвится с `ServiceWorkerRegistration | undefined`.
/// `undefined` означает, что SW для этого scope не зарегистрирован.
pub async fn get_registration() -> Result<Option<JsValue>, JsValue> {
    let sw = service_worker()?;
    let get_fn = js_sys::Reflect::get(&sw, &JsValue::from_str("getRegistration"))
        .map_err(|e| JsValue::from_str(&format!("Reflect getRegistration: {e:?}")))?;
    let get_fn: js_sys::Function = get_fn
        .dyn_into()
        .map_err(|_| JsValue::from_str("getRegistration не функция"))?;
    let promise = get_fn
        .call0(&sw)
        .map_err(|e| JsValue::from_str(&format!("getRegistration: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("getRegistration не вернул Promise"))?;
    let result: JsValue = JsFuture::from(promise).await?;
    if result.is_undefined() || result.is_null() {
        return Ok(None);
    }
    return Ok(Some(result));
}

// ============================================================
//                    SHOW NOTIFICATION
// ============================================================

/// Показывает уведомление через Service Worker.
///
/// `registration.showNotification(title, options)` — попап живёт вне
/// вкладки. Даже если закрыть страницу — попап останется.
pub async fn show_via_registration(
    reg: &JsValue,
    opts: &NotifyOptions,
) -> Result<(), JsValue> {
    let show_fn = js_sys::Reflect::get(reg, &JsValue::from_str("showNotification"))
        .map_err(|e| JsValue::from_str(&format!("Reflect showNotification: {e:?}")))?;
    let show_fn: js_sys::Function = show_fn
        .dyn_into()
        .map_err(|_| JsValue::from_str("showNotification не функция"))?;

    let title_arg = JsValue::from_str(&opts.title);
    let options_arg = build_options_js(opts);
    let promise = show_fn
        .call2(reg, &title_arg, &options_arg)
        .map_err(|e| JsValue::from_str(&format!("showNotification: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("showNotification не вернул Promise"))?;
    let _result: JsValue = JsFuture::from(promise).await?;
    Ok(())
}

/// Возвращает количество активных SW-уведомлений.
///
/// `getNotifications()` возвращает все уведомления, показанные через
/// этот SW.
pub async fn count_via_registration(reg: &JsValue) -> Result<u32, JsValue> {
    let f = js_sys::Reflect::get(reg, &JsValue::from_str("getNotifications"))
        .map_err(|e| JsValue::from_str(&format!("Reflect getNotifications: {e:?}")))?;
    let f: js_sys::Function = f
        .dyn_into()
        .map_err(|_| JsValue::from_str("getNotifications не функция"))?;
    let promise = f
        .call0(reg)
        .map_err(|e| JsValue::from_str(&format!("getNotifications: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("getNotifications не вернул Promise"))?;
    let result: JsValue = JsFuture::from(promise).await?;
    let arr: js_sys::Array = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("getNotifications вернул не Array"))?;
    return Ok(arr.length());
}

/// Закрывает все активные SW-уведомления.
///
/// Вызывает `.close()` на каждом объекте из `getNotifications()`.
/// Возвращает количество закрытых.
pub async fn close_all_via_registration(reg: &JsValue) -> Result<u32, JsValue> {
    let f = js_sys::Reflect::get(reg, &JsValue::from_str("getNotifications"))
        .map_err(|e| JsValue::from_str(&format!("Reflect getNotifications: {e:?}")))?;
    let f: js_sys::Function = f
        .dyn_into()
        .map_err(|_| JsValue::from_str("getNotifications не функция"))?;
    let promise = f
        .call0(reg)
        .map_err(|e| JsValue::from_str(&format!("getNotifications: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("getNotifications не вернул Promise"))?;
    let result: JsValue = JsFuture::from(promise).await?;
    let arr: js_sys::Array = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("getNotifications вернул не Array"))?;

    let mut closed = 0u32;
    for i in 0..arr.length() {
        let n = arr.get(i);
        if let Ok(close_fn) = js_sys::Reflect::get(&n, &JsValue::from_str("close"))
            .and_then(|v| v.dyn_into::<js_sys::Function>())
        {
            let _ = close_fn.call0(&n);
            closed += 1;
        }
    }
    return Ok(closed);
}

// ============================================================
//                    HELPERS
// ============================================================

fn build_options_js(opts: &NotifyOptions) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("body"),
        &JsValue::from_str(&opts.body),
    );
    if !opts.tag.is_empty() {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("tag"),
            &JsValue::from_str(&opts.tag),
        );
    }
    if let Some(icon) = &opts.icon {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("icon"),
            &JsValue::from_str(icon),
        );
    }
    if opts.require_interaction {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("requireInteraction"),
            &JsValue::TRUE,
        );
    }
    return obj.into();
}