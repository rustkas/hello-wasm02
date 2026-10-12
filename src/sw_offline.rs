//! Offline-first: манифест ресурсов и предзагрузка через Cache API.
//! Работает из главного потока; Service Worker перехватывает fetch.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Cache, CacheStorage, Request, Response};

pub const CACHE_APP_SHELL: &str = "app-shell-v1";
pub const CACHE_RUNTIME: &str = "runtime-v1";

/// Манифест «app shell»: ресурсы, которые должны быть доступны offline.
pub const ASSETS: &[&str] = &[
    "/",
    "/index.html",
    "/pkg/app.js",
    "/pkg/app_bg.wasm",
];

/// Открывает именованный кэш.
pub async fn open_cache(name: &str) -> Result<Cache, JsValue> {
    let caches: CacheStorage = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .caches()
        .map_err(|e| JsValue::from_str(&format!("caches: {e:?}")))?;
    let promise = caches.open(name);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await open: {e:?}")))?;
    Ok(value.unchecked_into::<Cache>())
}

/// Предзагружает список ресурсов в кэш `app-shell`.
/// Возвращает число успешно загруженных.
pub async fn preload_app_shell() -> Result<usize, JsValue> {
    let cache = open_cache(CACHE_APP_SHELL).await?;
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let mut count = 0usize;

    for url in ASSETS {
        let request = Request::new_with_str(url)
            .map_err(|e| JsValue::from_str(&format!("Request {url}: {e:?}")))?;
        let promise = window.fetch_with_request(&request);
        let value = JsFuture::from(promise)
            .await
            .map_err(|e| JsValue::from_str(&format!("fetch {url}: {e:?}")))?;
        let response: Response = value
            .dyn_into()
            .map_err(|_| JsValue::from_str("не Response"))?;

        if !response.ok() {
            continue;
        }

        let for_cache = response
            .clone()
            .map_err(|e| JsValue::from_str(&format!("clone: {e:?}")))?;
        let put_promise = cache.put_with_request(&request, &for_cache);
        JsFuture::from(put_promise)
            .await
            .map_err(|e| JsValue::from_str(&format!("put {url}: {e:?}")))?;
        count += 1;
    }
    Ok(count)
}

/// Возвращает список URL в кэше.
pub async fn list_cached(cache_name: &str) -> Result<Vec<String>, JsValue> {
    let cache = open_cache(cache_name).await?;
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

/// Удаляет именованный кэш. Идемпотентно.
pub async fn delete_cache(name: &str) -> Result<bool, JsValue> {
    let caches: CacheStorage = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .caches()
        .map_err(|e| JsValue::from_str(&format!("caches: {e:?}")))?;
    let promise = caches.delete(name);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await delete: {e:?}")))?;
    Ok(value.as_bool().unwrap_or(false))
}