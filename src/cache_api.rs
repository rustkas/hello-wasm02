//! Обёртка над Cache API: открытие кэша, put/match/delete, список ключей.
//! Автономный модуль — не зависит от других модулей проекта.
//!
//! ВАЖНО: методы Cache/CacheStorage (open, put, match, delete, keys)
//! в web-sys возвращают `Promise` напрямую, без обёртки `Result`.
//! Ошибки ловятся на `JsFuture::from(...).await`. Метод `window.caches()`
//! и `Request::new_with_str` помечены [Throws] — у них есть `Result`.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Cache, CacheStorage, Request, Response};

pub const CACHE_NAME: &str = "hello-wasm-offline-v1";

fn caches() -> Result<CacheStorage, JsValue> {
    web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .caches()
        .map_err(|e| JsValue::from_str(&format!("caches: {e:?}")))
}

/// Открывает (или создаёт) именованный кэш.
pub async fn open_cache(name: &str) -> Result<Cache, JsValue> {
    let promise = caches()?.open(name);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await open: {e:?}")))?;
    Ok(value.unchecked_into::<Cache>())
}

/// Сохраняет пару Request→Response в кэш.
pub async fn put(cache: &Cache, url: &str, response: &Response) -> Result<(), JsValue> {
    let request = Request::new_with_str(url)
        .map_err(|e| JsValue::from_str(&format!("Request: {e:?}")))?;
    let promise = cache.put_with_request(&request, response);
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("await put: {e:?}")))
}

/// Читает Response из кэша. None — если записи нет.
pub async fn get(cache: &Cache, url: &str) -> Result<Option<Response>, JsValue> {
    let request = Request::new_with_str(url)
        .map_err(|e| JsValue::from_str(&format!("Request: {e:?}")))?;
    let promise = cache.match_with_request(&request);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await match: {e:?}")))?;
    if value.is_undefined() {
        Ok(None)
    } else {
        Ok(Some(value.unchecked_into::<Response>()))
    }
}

/// Удаляет запись. Идемпотентно.
pub async fn remove(cache: &Cache, url: &str) -> Result<bool, JsValue> {
    let request = Request::new_with_str(url)
        .map_err(|e| JsValue::from_str(&format!("Request: {e:?}")))?;
    let promise = cache.delete_with_request(&request);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await delete: {e:?}")))?;
    Ok(value.as_bool().unwrap_or(false))
}

/// Возвращает список URL, лежащих в кэше.
pub async fn keys(cache: &Cache) -> Result<Vec<String>, JsValue> {
    let promise = cache.keys();
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await keys: {e:?}")))?;
    let array: js_sys::Array = value.unchecked_into();
    let mut out = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let req: Request = array.get(i).unchecked_into();
        out.push(req.url());
    }
    Ok(out)
}

/// Удаляет весь именованный кэш. Идемпотентно.
pub async fn delete_cache(name: &str) -> Result<bool, JsValue> {
    let promise = caches()?.delete(name);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await delete_cache: {e:?}")))?;
    Ok(value.as_bool().unwrap_or(false))
}