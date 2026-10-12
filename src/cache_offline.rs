//! Практический проект Главы 25: offline-репозиторий ресурсов.
//!
//! Загружает ресурс по URL, кэширует, читает из кэша,
//! применяет одну из трёх стратегий:
//!   • Cache First — кэш → сеть.
//!   • Network First — сеть → кэш.
//!   • Stale-While-Revalidate — кэш немедленно + фоном обновить.
//!
//! ВАЖНО: `Response` — одноразовый. Перед `put` всегда `clone()`.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{Request, Response};

use crate::cache_api;

/// Стратегия кэширования.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Strategy {
    /// Кэш → сеть (редко меняющиеся ресурсы).
    CacheFirst,
    /// Сеть → кэш (часто меняющиеся данные).
    NetworkFirst,
    /// Кэш немедленно + фоном обновить (ленты, списки).
    StaleWhileRevalidate,
}

/// Загружает URL через fetch. Возвращает Response и сохраняет в кэш.
///
/// Кэширует только успешные ответы (`response.ok()`), чтобы 404/500
/// не «залипали» в офлайне.
async fn fetch_and_cache(url: &str, cache: &web_sys::Cache) -> Result<Response, JsValue> {
    let request = Request::new_with_str(url)
        .map_err(|e| JsValue::from_str(&format!("Request: {e:?}")))?;
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;

    // Window::fetch_with_request возвращает Promise напрямую.
    let promise = window.fetch_with_request(&request);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("fetch: {e:?}")))?;
    let response: Response = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Response"))?;

    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }

    // Response одноразовый: clone до использования.
    let for_cache = response
        .clone()
        .map_err(|e| JsValue::from_str(&format!("clone: {e:?}")))?;
    cache_api::put(cache, url, &for_cache).await?;
    Ok(response)
}

/// Cache First: кэш → сеть.
pub async fn cache_first(url: &str, cache: &web_sys::Cache) -> Result<Response, JsValue> {
    if let Some(r) = cache_api::get(cache, url).await? {
        return Ok(r);
    }
    fetch_and_cache(url, cache).await
}

/// Network First: сеть → кэш (fallback).
pub async fn network_first(url: &str, cache: &web_sys::Cache) -> Result<Response, JsValue> {
    match fetch_and_cache(url, cache).await {
        Ok(r) => Ok(r),
        Err(_) => cache_api::get(cache, url)
            .await?
            .ok_or_else(|| JsValue::from_str("нет ни сети, ни кэша")),
    }
}

/// Stale-While-Revalidate: отдать из кэша немедленно + фоном обновить.
pub async fn stale_while_revalidate(
    url: &str,
    cache: &web_sys::Cache,
) -> Result<Response, JsValue> {
    let cached = cache_api::get(cache, url).await?;

    // Фоновая ревалидация.
    let cache_clone = cache.clone();
    let url_clone = url.to_string();
    spawn_local(async move {
        let _ = fetch_and_cache(&url_clone, &cache_clone).await;
    });

    cached.ok_or_else(|| JsValue::from_str("кэш пуст, ревалидация запущена"))
}

/// Диспетчер по стратегии.
pub async fn fetch_with_strategy(
    url: &str,
    cache: &web_sys::Cache,
    strategy: Strategy,
) -> Result<Response, JsValue> {
    match strategy {
        Strategy::CacheFirst => cache_first(url, cache).await,
        Strategy::NetworkFirst => network_first(url, cache).await,
        Strategy::StaleWhileRevalidate => stale_while_revalidate(url, cache).await,
    }
}

/// Сохраняет текстовый Response в кэш — без сети.
/// Полезно для предзагрузки «app shell» и ручного заполнения кэша.
///
/// Требует feature `ResponseInit` в web-sys (см. Cargo.toml).
pub async fn put_text(
    cache: &web_sys::Cache,
    url: &str,
    text: &str,
    content_type: &str,
) -> Result<(), JsValue> {
    let headers = web_sys::Headers::new()
        .map_err(|e| JsValue::from_str(&format!("Headers: {e:?}")))?;
    headers
        .set("Content-Type", content_type)
        .map_err(|e| JsValue::from_str(&format!("set header: {e:?}")))?;

    let init = web_sys::ResponseInit::new();
    init.set_status(200);
    init.set_headers(&headers);

    let response = Response::new_with_opt_str_and_init(Some(text), &init)
        .map_err(|e| JsValue::from_str(&format!("Response: {e:?}")))?;

    cache_api::put(cache, url, &response).await
}