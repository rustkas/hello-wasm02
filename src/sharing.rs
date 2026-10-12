//! Web Share API: поделиться файлом.
//!
//! **Требования:** HTTPS или localhost + user gesture.
//! Работает на мобильных браузерах (Android Chrome, iOS Safari)
//! и на некоторых десктопных (Edge, Safari macOS).
//! В Firefox не поддерживается.
//!
//! **User gesture обязателен.** `navigator.share()` вызывается
//! только из обработчика клика — иначе браузер отклонит запрос
//! с `NotAllowedError`.
//!
//! **`ShareData` через `Reflect`.** Тип `ShareData` в web-sys
//! не экспортирован. Строим объект `{ files, title, text }`
//! через `js_sys::Object` + `Reflect::set` — это работает
//! независимо от версии web-sys.
//!
//! **Отмена пользователем** — не ошибка. Промис реджектится
//! с `AbortError`; используйте [`is_abort_error`] для проверки.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::File;

/// Проверяет, поддерживает ли браузер Web Share API.
///
/// Проверка наличия `navigator.share` — достаточная.
/// На некоторых платформах метод есть, но `canShare({files})`
/// возвращает `false` — тогда шаринг файлов невозможен.
pub fn is_supported() -> bool {
    let navigator = match web_sys::window() {
        Some(w) => w.navigator(),
        None => return false,
    };
    let nav_js: &JsValue = navigator.as_ref();
    return js_sys::Reflect::get(nav_js, &JsValue::from_str("share"))
        .map(|v| !v.is_undefined())
        .unwrap_or(false);
}

/// Строит объект `ShareData` из списка файлов и текстовых полей.
///
/// Формат: `{ files: [...], title: "...", text: "..." }`.
fn build_share_data(
    files: &[File],
    title: &str,
    text: &str,
) -> Result<js_sys::Object, JsValue> {
    let data = js_sys::Object::new();

    if !files.is_empty() {
        let arr = js_sys::Array::new();
        for f in files {
            arr.push(f.as_ref());
        }
        js_sys::Reflect::set(
            &data,
            &JsValue::from_str("files"),
            &arr,
        )?;
    }

    if !title.is_empty() {
        js_sys::Reflect::set(
            &data,
            &JsValue::from_str("title"),
            &JsValue::from_str(title),
        )?;
    }

    if !text.is_empty() {
        js_sys::Reflect::set(
            &data,
            &JsValue::from_str("text"),
            &JsValue::from_str(text),
        )?;
    }

    return Ok(data);
}

/// Проверяет, можно ли поделиться указанными файлами.
///
/// Возвращает `false`, если:
/// * Web Share API не поддерживается;
/// * список файлов пуст;
/// * браузер не может расшарить файлы этого типа.
pub fn can_share_files(files: &[File]) -> bool {
    if files.is_empty() {
        return false;
    }

    let navigator = match web_sys::window() {
        Some(w) => w.navigator(),
        None => return false,
    };
    let nav_js: &JsValue = navigator.as_ref();

    // Проверяем наличие navigator.canShare.
    let can_share_fn_js = match js_sys::Reflect::get(
        nav_js,
        &JsValue::from_str("canShare"),
    ) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if can_share_fn_js.is_undefined() {
        return false;
    }
    let can_share_fn: js_sys::Function = match can_share_fn_js.dyn_into() {
        Ok(f) => f,
        Err(_) => return false,
    };

    // Строим данные — только files, без title/text.
    let data = match build_share_data(files, "", "") {
        Ok(d) => d,
        Err(_) => return false,
    };

    match can_share_fn.call1(nav_js, &data) {
        Ok(v) => return v.as_bool().unwrap_or(false),
        Err(_) => return false,
    }
}

/// Открывает системный диалог шаринга.
///
/// Возвращает `Ok(())`, если пользователь поделился
/// (или закрыл диалог успешно). `Err(AbortError)`, если
/// пользователь отменил. Другие `Err` — технические проблемы.
///
/// **User gesture обязателен.** Вызывать только из обработчика
/// клика — иначе `NotAllowedError`.
pub async fn share_files(
    files: &[File],
    title: &str,
    text: &str,
) -> Result<(), JsValue> {
    let navigator = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .navigator();
    let nav_js: &JsValue = navigator.as_ref();

    let share_fn_js = js_sys::Reflect::get(
        nav_js,
        &JsValue::from_str("share"),
    )?;
    if share_fn_js.is_undefined() {
        return Err(JsValue::from_str("navigator.share недоступен"));
    }
    let share_fn: js_sys::Function = share_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("share не функция"))?;

    let data = build_share_data(files, title, text)?;

    let promise_js = share_fn.call1(nav_js, &data)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("share не Promise"))?;

    let _ = JsFuture::from(promise).await?;
    return Ok(());
}

/// Проверяет, была ли ошибка вызвана отменой шаринга.
///
/// Пользователь нажал «Отмена» в системном диалоге — браузер
/// реджектит промис с `AbortError`. Это **не ошибка**, а нормальный
/// сценарий. Обрабатываем отдельно от технических проблем.
pub fn is_abort_error(e: &JsValue) -> bool {
    // Сначала — как DOMException.
    if let Some(exc) = e.dyn_ref::<web_sys::DomException>() {
        return exc.name() == "AbortError";
    }
    // Fallback — как строка (в headless-браузерах может не быть
    // DOMException).
    if let Some(s) = e.as_string() {
        return s.contains("AbortError") || s.contains("abort");
    }
    return false;
}