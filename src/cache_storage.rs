//! Cache API: хранение Blob как Response.
//!
//! **Зачем.** Cache API — самый простой способ хранить «файлы как
//! ответы сервера». Работает поверх стандартного `Request`/`Response`,
//! поддерживает Range-запросы, MIME-типы и заголовки.
//!
//! **Модель.** Открываем именованный кэш (`caches.open("my-cache")`)
//! и кладём в него пары «ключ → Response». Ключ — произвольный URL
//! или `Request`, значение — `Response`, тело которого может быть
//! `Blob`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, Cache, CacheStorage, Request, RequestInit, Response};

/// Открывает (или создаёт) именованный кэш.
pub async fn open_cache(name: &str) -> Result<Cache, JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?;
    let caches: CacheStorage = window
        .caches()
        .map_err(|_| JsValue::from_str("caches недоступен"))?;

    let promise = caches.open(name);
    let value = JsFuture::from(promise).await?;
    return value
        .dyn_into::<Cache>()
        .map_err(|_| JsValue::from_str("не Cache"));
}

/// Превращает Blob в Response с указанным MIME-типом.
///
/// Response — то, что Cache API умеет хранить. Заголовок
/// `Content-Type` сохраняется в Response и вернётся при чтении.
fn blob_to_response(blob: &Blob) -> Result<Response, JsValue> {
    return Response::new_with_opt_blob(Some(blob));
}

/// Кладёт Blob в кэш под указанным ключом (URL).
///
/// Если запись с таким ключом уже существует — она перезаписывается.
pub async fn put_blob(
    cache_name: &str,
    key: &str,
    blob: &Blob,
) -> Result<(), JsValue> {
    let cache = open_cache(cache_name).await?;

    // Request нужен для ключа. Используем GET без тела.
    let opts = RequestInit::new();
    opts.set_method("GET");
    let request = Request::new_with_str_and_init(key, &opts)?;

    let response = blob_to_response(blob)?;
    let promise = cache.put_with_request(&request, &response);
    let _ = JsFuture::from(promise).await?;
    return Ok(());
}

/// Читает Blob из кэша по ключу.
///
/// Возвращает `Ok(None)`, если записи нет.
pub async fn get_blob(
    cache_name: &str,
    key: &str,
) -> Result<Option<Blob>, JsValue> {
    let cache = open_cache(cache_name).await?;

    let opts = RequestInit::new();
    opts.set_method("GET");
    let request = Request::new_with_str_and_init(key, &opts)?;

    let promise = cache.match_with_request(&request);
    let value = JsFuture::from(promise).await?;

    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }

    let response: Response = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Response"))?;
    let blob_promise = response.blob()?;
    let blob_value = JsFuture::from(blob_promise).await?;
    let blob: Blob = blob_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Blob"))?;
    return Ok(Some(blob));
}

/// Удаляет запись из кэша.
///
/// Возвращает `true`, если запись была и удалена.
/// `false`, если записи не было — идемпотентно.
pub async fn delete_blob(
    cache_name: &str,
    key: &str,
) -> Result<bool, JsValue> {
    let cache = open_cache(cache_name).await?;

    let opts = RequestInit::new();
    opts.set_method("GET");
    let request = Request::new_with_str_and_init(key, &opts)?;

    let promise = cache.delete_with_request(&request);
    let value = JsFuture::from(promise).await?;
    return Ok(value.as_bool().unwrap_or(false));
}

/// Возвращает список URL, хранящихся в кэше.
pub async fn list_keys(cache_name: &str) -> Result<Vec<String>, JsValue> {
    let cache = open_cache(cache_name).await?;

    let promise = cache.keys();
    let value = JsFuture::from(promise).await?;
    let array: js_sys::Array = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("keys не Array"))?;

    let mut out: Vec<String> = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let item = array.get(i);
        // Request или строка.
        if let Some(s) = item.as_string() {
            out.push(s);
            continue;
        }
        let req: Request = match item.dyn_into() {
            Ok(r) => r,
            Err(_) => continue,
        };
        out.push(req.url());
    }
    return Ok(out);
}

/// Удаляет весь кэш с указанным именем.
pub async fn delete_cache(name: &str) -> Result<bool, JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?;
    let caches = window
        .caches()
        .map_err(|_| JsValue::from_str("caches недоступен"))?;
    let promise = caches.delete(name);
    let value = JsFuture::from(promise).await?;
    return Ok(value.as_bool().unwrap_or(false));
}