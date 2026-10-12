//! Помощники для работы с Fetch API.
//!
//! ## Что в этом модуле
//!
//! - `make_get_request` — создание GET-запроса.
//! - `send_request` — отправка и проверка `ok()`.
//! - `get`, `get_json`, `get_text` — высокоуровневые функции.
//! - Утилиты для чтения свойств `Response`.
//!
//! ## Что будет дальше
//!
//! В следующих подглавах модуль расширится:
//! - 16.2 — POST, PUT, DELETE, тело запроса.
//! - 16.3 — заголовки, case-insensitivity.
//! - 16.4 — аутентификация.
//! - 16.5 — классификация ошибок.
//! - 16.6 — retry.
//! - 16.7 — timeout и Abort.
//! - 16.8 — параллельные запросы.
//! - 16.9 — типы ответов, binary data.
//! - 16.10 — Streams.
//! - 16.11 — CORS.
//! - 16.12 — API client.
#![allow(dead_code)]

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use js_sys::Array;
use web_sys::{
    FormData, Headers, ReadableStream, ReadableStreamDefaultReader, Request, RequestInit,
    RequestMode, Response, UrlSearchParams,
};

use futures::stream::{self, StreamExt};
use js_sys::Promise;

use crate::auth::TokenStoreHandle;

// ============================================================
// Создание запросов
// ============================================================

/// Создаёт GET-запрос.
///
/// `mode` — CORS-режим. Для запросов на другой origin используйте
/// `RequestMode::Cors`. Для запросов на тот же origin —
/// `RequestMode::SameOrigin`.
pub fn make_get_request(url: &str, mode: RequestMode) -> Result<Request, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(mode);

    return Request::new_with_str_and_init(url, &opts);
}

// ============================================================
// Отправка запросов
// ============================================================

/// Отправляет запрос и возвращает `Response`.
///
/// Проверяет `response.ok()`: если ответ не 2xx — возвращает `Err`
/// с информацией о статусе.
pub async fn send_request(request: &Request) -> Result<Response, JsValue> {
    let window = web_sys::window().expect("no window");
    let resp_value = JsFuture::from(window.fetch_with_request(request)).await?;

    if !resp_value.is_instance_of::<Response>() {
        return Err(JsValue::from_str("response is not a Response object"));
    }

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("failed to cast JsValue to Response"))?;

    if !resp.ok() {
        let status = resp.status();
        let status_text = resp.status_text();
        return Err(JsValue::from_str(&format!(
            "HTTP {} {}",
            status, status_text
        )));
    }

    return Ok(resp);
}

// ============================================================
// Высокоуровневые функции
// ============================================================

/// GET-запрос, возвращает `Response`.
pub async fn get(url: &str) -> Result<Response, JsValue> {
    let request = make_get_request(url, RequestMode::Cors)?;
    return send_request(&request).await;
}

/// GET-запрос, возвращает текст.
pub async fn get_text(url: &str) -> Result<String, JsValue> {
    let resp = get(url).await?;
    let text_promise = resp.text()?;
    let text_value = JsFuture::from(text_promise).await?;
    return Ok(text_value.as_string().unwrap_or_default());
}

/// GET-запрос, возвращает десериализованный JSON.
pub async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, JsValue> {
    let resp = get(url).await?;
    let json_promise = resp.json()?;
    let json_value = JsFuture::from(json_promise).await?;
    return deserialize_json(&json_value);
}

// ============================================================
// JSON helpers
// ============================================================

/// Десериализует `JsValue` в `T` через JSON.
///
/// Промежуточный шаг: `JsValue` → JSON-строка → `T`.
fn deserialize_json<T: DeserializeOwned>(value: &JsValue) -> Result<T, JsValue> {
    let json_string = js_sys::JSON::stringify(value)
        .map_err(|_| JsValue::from_str("JSON.stringify failed"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("JSON.stringify result is not a string"))?;

    return serde_json::from_str(&json_string)
        .map_err(|e| JsValue::from_str(&format!("deserialize error: {}", e)));
}

// ============================================================
// Свойства Response
// ============================================================

/// Возвращает HTTP-статус ответа.
pub fn response_status(resp: &Response) -> u16 {
    return resp.status();
}

/// Возвращает `true`, если статус 2xx.
pub fn response_is_ok(resp: &Response) -> bool {
    return resp.ok();
}

/// Возвращает URL ответа.
pub fn response_url(resp: &Response) -> String {
    return resp.url();
}

// ============================================================
// Создание запросов: общие методы
// ============================================================

/// Создаёт запрос с указанным методом.
///
/// Базовый конструктор для всех методов без тела.
pub fn make_request_with_method(
    url: &str,
    method: &str,
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_mode(mode);

    return Request::new_with_str_and_init(url, &opts);
}

// ============================================================
// POST: с JSON-телом
// ============================================================

/// Создаёт POST-запрос с JSON-телом.
///
/// `T` должен реализовать `Serialize`. Тело сериализуется через
/// `serde_json::to_string`. Устанавливается заголовок
/// `Content-Type: application/json`.
pub fn make_post_json_request<T: Serialize>(
    url: &str,
    body: &T,
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let json = serde_json::to_string(body)
        .map_err(|e| JsValue::from_str(&format!("serialize error: {}", e)))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(mode);
    opts.set_body(&JsValue::from_str(&json));

    let request = Request::new_with_str_and_init(url, &opts)?;
    request.headers().set("Content-Type", "application/json")?;

    return Ok(request);
}

// ============================================================
// POST: с form-urlencoded телом
// ============================================================

/// Создаёт POST-запрос с телом `application/x-www-form-urlencoded`.
///
/// Использует `URLSearchParams` — стандартный способ кодирования
/// пар ключ-значение в URL-подобную строку.
pub fn make_post_form_urlencoded_request(
    url: &str,
    params: &[(&str, &str)],
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let form_params = UrlSearchParams::new()?;
    for (name, value) in params {
        form_params.append(name, value);
    }

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(mode);
    opts.set_body(form_params.as_ref());

    let request = Request::new_with_str_and_init(url, &opts)?;
    request
        .headers()
        .set("Content-Type", "application/x-www-form-urlencoded")?;

    return Ok(request);
}

// ============================================================
// POST: с multipart/form-data телом
// ============================================================

/// Создаёт POST-запрос с телом `multipart/form-data`.
///
/// `FormData` позволяет отправлять текстовые поля и файлы.
/// **Важно:** `Content-Type` устанавливать не нужно — браузер
/// добавит его автоматически с правильным `boundary`.
pub fn make_post_form_data_request(
    url: &str,
    fields: &[(&str, &str)],
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let form_data = FormData::new()?;
    for (name, value) in fields {
        form_data.append_with_str(name, value)?;
    }

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(mode);
    opts.set_body(form_data.as_ref());

    return Request::new_with_str_and_init(url, &opts);
}

// ============================================================
// PUT, PATCH, DELETE
// ============================================================

/// Создаёт PUT-запрос (без тела).
pub fn make_put_request(url: &str, mode: RequestMode) -> Result<Request, JsValue> {
    return make_request_with_method(url, "PUT", mode);
}

/// Создаёт PATCH-запрос (без тела).
pub fn make_patch_request(url: &str, mode: RequestMode) -> Result<Request, JsValue> {
    return make_request_with_method(url, "PATCH", mode);
}

/// Создаёт DELETE-запрос.
pub fn make_delete_request(url: &str, mode: RequestMode) -> Result<Request, JsValue> {
    return make_request_with_method(url, "DELETE", mode);
}

/// Создаёт PUT-запрос с JSON-телом.
pub fn make_put_json_request<T: Serialize>(
    url: &str,
    body: &T,
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let json = serde_json::to_string(body)
        .map_err(|e| JsValue::from_str(&format!("serialize error: {}", e)))?;

    let opts = RequestInit::new();
    opts.set_method("PUT");
    opts.set_mode(mode);
    opts.set_body(&JsValue::from_str(&json));

    let request = Request::new_with_str_and_init(url, &opts)?;
    request.headers().set("Content-Type", "application/json")?;

    return Ok(request);
}

/// Создаёт PATCH-запрос с JSON-телом.
pub fn make_patch_json_request<T: Serialize>(
    url: &str,
    body: &T,
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let json = serde_json::to_string(body)
        .map_err(|e| JsValue::from_str(&format!("serialize error: {}", e)))?;

    let opts = RequestInit::new();
    opts.set_method("PATCH");
    opts.set_mode(mode);
    opts.set_body(&JsValue::from_str(&json));

    let request = Request::new_with_str_and_init(url, &opts)?;
    request.headers().set("Content-Type", "application/json")?;

    return Ok(request);
}

// ============================================================
// Высокоуровневые функции для POST / PUT / PATCH / DELETE
// ============================================================

/// POST-запрос с JSON-телом, возвращает `Response`.
pub async fn post_json<T: Serialize>(url: &str, body: &T) -> Result<Response, JsValue> {
    let request = make_post_json_request(url, body, RequestMode::Cors)?;
    return send_request(&request).await;
}

/// POST-запрос с JSON-телом, возвращает десериализованный JSON.
pub async fn post_json_response<T: Serialize, R: DeserializeOwned>(
    url: &str,
    body: &T,
) -> Result<R, JsValue> {
    let resp = post_json(url, body).await?;
    let json_promise = resp.json()?;
    let json_value = JsFuture::from(json_promise).await?;
    return deserialize_json(&json_value);
}

/// POST-запрос с `application/x-www-form-urlencoded`, возвращает `Response`.
pub async fn post_form_urlencoded(url: &str, params: &[(&str, &str)]) -> Result<Response, JsValue> {
    let request = make_post_form_urlencoded_request(url, params, RequestMode::Cors)?;
    return send_request(&request).await;
}

/// POST-запрос с `multipart/form-data`, возвращает `Response`.
pub async fn post_form_data(url: &str, fields: &[(&str, &str)]) -> Result<Response, JsValue> {
    let request = make_post_form_data_request(url, fields, RequestMode::Cors)?;
    return send_request(&request).await;
}

/// PUT-запрос с JSON-телом, возвращает `Response`.
pub async fn put_json<T: Serialize>(url: &str, body: &T) -> Result<Response, JsValue> {
    let request = make_put_json_request(url, body, RequestMode::Cors)?;
    return send_request(&request).await;
}

/// PATCH-запрос с JSON-телом, возвращает `Response`.
pub async fn patch_json<T: Serialize>(url: &str, body: &T) -> Result<Response, JsValue> {
    let request = make_patch_json_request(url, body, RequestMode::Cors)?;
    return send_request(&request).await;
}

/// DELETE-запрос, возвращает `Response`.
pub async fn delete_request(url: &str) -> Result<Response, JsValue> {
    let request = make_delete_request(url, RequestMode::Cors)?;
    return send_request(&request).await;
}

// ============================================================
// Headers: создание
// ============================================================

/// Создаёт `Headers` с заданными заголовками.
///
/// Используется для установки общих заголовков на несколько
/// запросов или для тестирования.
pub fn make_headers(headers: &[(&str, &str)]) -> Result<Headers, JsValue> {
    let result = Headers::new()?;
    for (name, value) in headers {
        result.set(name, value)?;
    }
    return Ok(result);
}

// ============================================================
// Запросы с заголовками
// ============================================================

/// Создаёт GET-запрос с кастомными заголовками.
pub fn make_request_with_headers(
    url: &str,
    headers: &[(&str, &str)],
    mode: RequestMode,
) -> Result<Request, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(mode);

    let request = Request::new_with_str_and_init(url, &opts)?;

    for (name, value) in headers {
        request.headers().set(name, value)?;
    }

    return Ok(request);
}

/// GET-запрос с кастомными заголовками, возвращает `Response`.
pub async fn get_with_headers(url: &str, headers: &[(&str, &str)]) -> Result<Response, JsValue> {
    let request = make_request_with_headers(url, headers, RequestMode::Cors)?;
    return send_request(&request).await;
}

// ============================================================
// Headers: чтение из ответа
// ============================================================

/// Читает значение заголовка из `Response`.
///
/// Имя заголовка регистронезависимо: `"Content-Type"`,
/// `"content-type"`, `"CONTENT-TYPE"` — одно и то же.
///
/// Возвращает `None`, если заголовка нет или он недоступен
/// из-за CORS-ограничений.
pub fn get_response_header(resp: &Response, name: &str) -> Option<String> {
    return resp.headers().get(name).ok().flatten();
}

/// Проверяет наличие заголовка в `Response`.
///
/// Имя регистронезависимо. Возвращает `false`, если заголовка
/// нет или он недоступен из-за CORS.
pub fn has_response_header(resp: &Response, name: &str) -> bool {
    return resp.headers().has(name).unwrap_or(false);
}

/// Возвращает все доступные заголовки из `Response` как `Vec<(name, value)>`.
///
/// Перебирает `Headers.entries()`. Доступны только те заголовки,
/// которые разрешены CORS-правилами (safelisted + expose).
///
/// `Headers::entries()` возвращает `js_sys::Iterator` напрямую.
/// `Iterator::next()` возвращает `Result<IteratorNext, JsValue>`.
/// `IteratorNext` содержит методы `done()` и `value()`.
pub fn get_all_response_headers(resp: &Response) -> Vec<(String, String)> {
    let headers = resp.headers();
    let entries: js_sys::Iterator = headers.entries();

    let mut result = Vec::new();

    loop {
        let next = match entries.next() {
            Ok(n) => n,
            Err(_) => break,
        };

        if next.done() {
            break;
        }

        let value = next.value();

        // Каждая запись — массив из двух элементов: [name, value]
        if let Ok(arr) = value.dyn_into::<Array>() {
            if arr.length() == 2 {
                let name = arr.get(0).as_string().unwrap_or_default();
                let val = arr.get(1).as_string().unwrap_or_default();
                result.push((name, val));
            }
        }
    }

    return result;
}

/// POST с JSON-телом, возвращает сырой Response (без десериализации).
pub async fn post_json_raw(
    url: &str,
    body: &serde_json::Value,
) -> Result<web_sys::Response, JsValue> {
    let body_str = serde_json::to_string(body).map_err(|e| JsValue::from_str(&e.to_string()))?;
    post_json_raw_string(url, &body_str).await
}

pub async fn post_json_raw_string(url: &str, body: &str) -> Result<web_sys::Response, JsValue> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_body(&JsValue::from_str(body));

    let request = web_sys::Request::new_with_str_and_init(url, &opts)?;
    request.headers().set("Content-Type", "application/json")?;

    let window = web_sys::window().unwrap();
    let resp_value =
        wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await?;

    resp_value.dyn_into::<web_sys::Response>()
}

/// Десериализует Response как JSON.
pub async fn response_json(resp: &web_sys::Response) -> Result<serde_json::Value, JsValue> {
    let promise = resp.json()?;
    let json: JsValue = wasm_bindgen_futures::JsFuture::from(promise).await?;
    let value: serde_json::Value =
        serde_wasm_bindgen::from_value(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(value)
}

/// Запрос с Bearer-токеном.
pub async fn make_authenticated_request(
    url: &str,
    token: &str,
) -> Result<web_sys::Response, JsValue> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);

    let request = web_sys::Request::new_with_str_and_init(url, &opts)?;
    request
        .headers()
        .set("Authorization", &format!("Bearer {token}"))?;

    let window = web_sys::window().unwrap();
    let resp_value =
        wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await?;

    resp_value.dyn_into::<web_sys::Response>()
}

/// Обёртка с автоматическим refresh при 401.
///
/// Если запрос вернул 401 — пытается обновить токен и повторить запрос один раз.
pub async fn send_with_auth_retry(
    store: &TokenStoreHandle,
    url: &str,
) -> Result<web_sys::Response, JsValue> {
    let token = store
        .borrow()
        .access_token
        .clone()
        .ok_or_else(|| JsValue::from_str("no access token"))?;

    let resp = make_authenticated_request(url, &token).await?;

    if resp.status() != 401 {
        return Ok(resp);
    }

    // Токен истёк — пробуем refresh.
    let refreshed = crate::auth::try_refresh(store).await?;
    if !refreshed {
        return Ok(resp); // refresh не удался — возвращаем исходный 401
    }

    let new_token = store.borrow().access_token.clone().unwrap();
    make_authenticated_request(url, &new_token).await
}

// ============================================================
// Классификация ошибок
// ============================================================

/// Классифицированная ошибка fetch-запроса.
///
/// Отличается от «сырого» `JsValue` тем, что каждый вариант несёт
/// информацию, достаточную для принятия решения: повторять запрос,
/// показать сообщение пользователю или молча игнорировать.
#[derive(Debug, Clone)]
pub enum FetchError {
    /// Сетевая ошибка: DNS не разрешился, CORS заблокировал,
    /// соединение оборвалось. `fetch` не вернул `Response` вообще.
    /// В JS это обычно `TypeError`.
    Network(String),

    /// Запрос дошёл до сервера, но тот ответил 4xx или 5xx.
    /// Первое поле — статус, второе — тело ответа (может быть пустым).
    Http(u16, String),

    /// Запрос отменён через `AbortController`.
    /// В JS это `DOMException` с `name === "AbortError"`.
    Aborted,

    ///  сколько ждали
    Timeout(u32),

    /// Ответ получен, но не удалось разобрать тело (JSON-парсинг и т.п.).
    Parse(String),
}

impl FetchError {
    /// Человекочитаемое описание — для UI.
    pub fn user_message(&self) -> String {
        match self {
            FetchError::Network(msg) => {
                return format!("Сетевая ошибка: нет соединения с сервером. ({msg})")
            }
            FetchError::Http(status, _) if *status >= 500 => {
                return format!("Ошибка сервера: {status}. Попробуйте позже.")
            }
            FetchError::Http(status, _) if *status == 404 => {
                return format!("Ресурс не найден: {status}.")
            }
            FetchError::Http(status, _) => {
                return format!("Запрос отклонён: {status}.")
            }
            FetchError::Aborted => "Запрос отменён.".to_string(),
            FetchError::Timeout(ms) => format!("Таймаут: сервер не ответил за {ms} мс."),
            FetchError::Parse(msg) => format!("Ошибка разбора ответа: {msg}"),
        }
    }

    /// Метод-обёртка над свободной функцией `is_retriable`.
    ///
    /// Позволяет писать `error.is_retryable()` вместо
    /// `fetch::is_retriable(&error)`. Обратите внимание на написание:
    /// метод — `is_retryable` (через «а»), функция — `is_retriable`
    /// (через «и»). Это исторически сложилось: функция появилась
    /// раньше, а метод добавлен для удобства.
    pub fn is_retryable(&self) -> bool {
        is_retriable(self)
    }
}

/// Определяет, является ли `JsValue` ошибкой типа `AbortError`.
///
/// `AbortError` — это `DOMException` с `name === "AbortError"`.
/// Проверяем через `Reflect::get`, потому что у `JsValue` нет
/// прямого доступа к свойствам.
fn is_abort_error(value: &JsValue) -> bool {
    let name = js_sys::Reflect::get(value, &JsValue::from_str("name"));
    return matches!(name, Ok(n) if n.as_string().as_deref() == Some("AbortError"));
}

/// Классифицирует «сырую» ошибку от `fetch` в `FetchError`.
///
/// Сюда попадают только те ошибки, которые возникают **до** получения
/// `Response` — сетевые сбои и отмены. HTTP-ошибки классифицируются
/// отдельно, по статусу ответа.
pub fn classify_error(err: JsValue) -> FetchError {
    if is_abort_error(&err) {
        return FetchError::Aborted;
    }

    // TypeError в JS — это почти всегда сетевая ошибка:
    // DNS, CORS, обрыв соединения.
    if err.is_instance_of::<js_sys::TypeError>() {
        let msg = err
            .dyn_ref::<js_sys::Error>()
            .and_then(|e| e.message().as_string())
            .unwrap_or_else(|| "unknown network error".to_string());
        return FetchError::Network(msg);
    }

    // Всё остальное — тоже сетевая ошибка, но без деталей.
    let msg = format!("{err:?}");
    FetchError::Network(msg)
}

/// Отправляет GET-запрос и возвращает классифицированный результат.
///
/// В отличие от `get_json`, эта функция **не** пытается разобрать тело
/// как JSON. Но при HTTP-статусе ≥ 400 она **читает тело как текст**
/// и кладёт его во второй элемент `FetchError::Http`.
pub async fn send_request_classified(url: &str) -> Result<web_sys::Response, FetchError> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);

    let request = web_sys::Request::new_with_str_and_init(url, &opts).map_err(classify_error)?;

    let window =
        web_sys::window().ok_or_else(|| FetchError::Network("no window object".to_string()))?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(classify_error)?;

    let response: web_sys::Response = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(classify_error)?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok(response)
}

pub async fn read_error_body(response: &web_sys::Response) -> String {
    if let Ok(promise) = response.text() {
        if let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await {
            if let Some(s) = value.as_string() {
                return s;
            }
        }
    }
    String::new()
}

// ============================================================
// Retry и восстановление
// ============================================================

/// Конфигурация стратегии retry.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Максимальное число попыток, включая первую.
    pub max_attempts: u32,
    /// Базовая задержка в миллисекундах.
    pub base_delay_ms: u64,
    /// Верхняя граница задержки.
    pub max_delay_ms: u64,
    /// Доля jitter: 0.0 — без случайности, 0.5 — до +50% к задержке.
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            jitter_factor: 0.3,
        }
    }
}

/// Стоит ли повторять запрос при данной ошибке.
///
/// Сетевые ошибки, 5xx, 429 (Too Many Requests) и 408 (Request Timeout)
/// считаются временными. 4xx (кроме 408 и 429) — постоянными,
/// повтор только навредит.
pub fn is_retriable(error: &FetchError) -> bool {
    match error {
        FetchError::Network(_) => true,
        FetchError::Http(status, _) => *status >= 500 || *status == 429 || *status == 408,
        FetchError::Aborted => false,
        FetchError::Timeout(_) => true, // ← таймаут временный, повторяем
        FetchError::Parse(_) => false,
    }
}

/// Вычисляет задержку перед следующей попыткой.
///
/// Формула: `min(base * 2^attempt, max) + jitter`.
/// `attempt` нумеруется с нуля: первая задержка равна `base`,
/// вторая — `2 * base`, третья — `4 * base` и так далее.
pub fn calculate_backoff(attempt: u32, config: &RetryConfig) -> u64 {
    let exponential = config.base_delay_ms.saturating_mul(1u64 << attempt.min(20));
    let clamped = exponential.min(config.max_delay_ms);

    if config.jitter_factor <= 0.0 {
        return clamped;
    }

    let jitter_range = (clamped as f64) * config.jitter_factor;
    let jitter = (js_sys::Math::random() * jitter_range) as u64;
    clamped + jitter
}

/// Читает заголовок `Retry-After` и переводит его в миллисекунды.
///
/// Поддерживается только числовая форма (`Retry-After: 120`).
/// HTTP-дата игнорируется — для учебного примера это избыточно.
fn parse_retry_after(response: &web_sys::Response) -> Option<u64> {
    let value = response.headers().get("Retry-After").ok().flatten()?;
    value.trim().parse::<u64>().ok().map(|s| s * 1000)
}

/// Одна попытка запроса. Возвращает `Err` с флагом `Retry-After`.
async fn send_once(url: &str) -> Result<web_sys::Response, (FetchError, Option<u64>)> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);

    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| (classify_error(e), None))?;

    let window =
        web_sys::window().ok_or_else(|| (FetchError::Network("no window object".into()), None))?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| (classify_error(e), None))?;

    let response: web_sys::Response = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(|e| (classify_error(e), None))?;

    if response.ok() {
        return Ok(response);
    }

    let status = response.status();
    let retry_after = parse_retry_after(&response);
    let body = read_error_body(&response).await;
    Err((FetchError::Http(status, body), retry_after))
}

/// Отправляет GET-запрос с автоматическим retry.
///
/// Повторяет запрос при временных ошибках, используя экспоненциальный
/// backoff с jitter. Если сервер прислал `Retry-After`, его значение
/// имеет приоритет над вычисленной задержкой.
pub async fn send_with_retry(
    url: &str,
    config: &RetryConfig,
) -> Result<web_sys::Response, FetchError> {
    let mut last_error: Option<FetchError> = None;

    for attempt in 0..config.max_attempts {
        match send_once(url).await {
            Ok(resp) => return Ok(resp),
            Err((error, retry_after)) => {
                let is_last = attempt + 1 >= config.max_attempts;

                if !is_retriable(&error) || is_last {
                    return Err(error);
                }

                let delay = retry_after.unwrap_or_else(|| calculate_backoff(attempt, config));

                gloo_timers::future::TimeoutFuture::new(delay.min(u32::MAX as u64) as u32).await;

                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| FetchError::Network("retry loop exited unexpectedly".into())))
}

// ============================================================
// Timeout и отмена
// ============================================================

/// Создаёт новый `AbortController`.
///
/// Возвращает контроллер, сигнал которого можно передать в запрос.
pub fn make_abort_controller() -> Result<web_sys::AbortController, JsValue> {
    web_sys::AbortController::new()
}

/// Создаёт `AbortController` с автоматическим таймаутом.
///
/// Возвращает `(controller, timeout_id)`. Через `timeout_ms` миллисекунд
/// контроллер будет автоматически отменён. `timeout_id` нужен для
/// очистки таймера, если запрос завершится раньше.
///
/// **Важно:** в браузере нет надёжного способа отменить `setTimeout`
/// из Rust без хранения идентификатора. Мы возвращаем его вызывающему,
/// чтобы тот мог вызвать `clear_timeout(timeout_id)` после успешного
/// завершения запроса.
pub fn make_timeout_controller(
    timeout_ms: u32,
) -> Result<(web_sys::AbortController, i32), JsValue> {
    let controller = web_sys::AbortController::new()?;
    let controller_clone = controller.clone();

    let callback = Closure::once_into_js(move || {
        controller_clone.abort();
    });

    let window = web_sys::window().expect("no window");
    let timeout_id = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        callback.unchecked_ref(),
        timeout_ms as i32,
    )?;

    Ok((controller, timeout_id))
}

/// Отменяет ранее установленный таймаут.
///
/// Вызывать после успешного завершения запроса, чтобы `abort()`
/// не сработал зря. Если этого не сделать, запрос завершится,
/// а через некоторое время сработает таймер и отменит **следующий**
/// запрос, если `AbortController` был переиспользован.
pub fn clear_timeout(timeout_id: i32) {
    if let Some(window) = web_sys::window() {
        window.clear_timeout_with_handle(timeout_id);
    }
}

/// GET-запрос с таймаутом.
///
/// Если сервер не ответит за `timeout_ms` миллисекунд, запрос будет
/// отменён. Ошибка отмены вернётся как `FetchError::Aborted`.
///
/// **Ограничение:** таймаут покрывает только получение заголовков.
/// Чтение тела ответа (`response.text()`, `response.json()`) уже не
/// ограничено этим таймаутом, потому что `clear_timeout` вызывается
/// сразу после `fetch`. Если нужно ограничить и чтение тела,
/// используйте `send_cancellable` вручную.
pub async fn get_with_timeout(url: &str, timeout_ms: u32) -> Result<web_sys::Response, FetchError> {
    let (controller, timeout_id) = make_timeout_controller(timeout_ms).map_err(classify_error)?;
    let signal = controller.signal();

    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_signal(Some(&signal));

    let request = web_sys::Request::new_with_str_and_init(url, &opts).map_err(|e| {
        clear_timeout(timeout_id);
        classify_error(e)
    })?;

    let window = web_sys::window().ok_or_else(|| {
        clear_timeout(timeout_id);
        FetchError::Network("no window object".into())
    })?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| {
            clear_timeout(timeout_id);
            classify_error(e)
        })?;

    // Заголовки получены — таймер больше не нужен.
    clear_timeout(timeout_id);

    let response: web_sys::Response = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(classify_error)?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok(response)
}

/// Отправляет запрос с возможностью ручной отмены.
///
/// Возвращает `(Response, AbortController)`. Контроллер нужен
/// вызывающему, чтобы отменить запрос, пока он ещё в полёте.
/// Если запрос уже завершён, вызов `abort()` на контроллере
/// не даст эффекта — это безопасно.
///
/// `timeout_ms` — опциональный таймаут. `None` означает «без таймаута,
/// только ручная отмена».
pub async fn send_cancellable(
    url: &str,
    timeout_ms: Option<u32>,
) -> Result<(web_sys::Response, web_sys::AbortController), FetchError> {
    let controller = web_sys::AbortController::new().map_err(classify_error)?;

    let timeout_id = match timeout_ms {
        Some(ms) => {
            let controller_for_abort = controller.clone();
            let callback = Closure::once_into_js(move || {
                controller_for_abort.abort();
            });
            let window = web_sys::window().expect("no window");
            let id = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.unchecked_ref(),
                    ms as i32,
                )
                .map_err(classify_error)?;
            Some(id)
        }
        None => None,
    };

    let signal = controller.signal();

    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_signal(Some(&signal));

    let request = web_sys::Request::new_with_str_and_init(url, &opts).map_err(|e| {
        if let Some(id) = timeout_id {
            clear_timeout(id);
        }
        classify_error(e)
    })?;

    let window = web_sys::window().ok_or_else(|| {
        if let Some(id) = timeout_id {
            clear_timeout(id);
        }
        FetchError::Network("no window object".into())
    })?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| {
            if let Some(id) = timeout_id {
                clear_timeout(id);
            }
            classify_error(e)
        })?;

    // Заголовки получены — таймер больше не нужен.
    if let Some(id) = timeout_id {
        clear_timeout(id);
    }

    let response: web_sys::Response = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(classify_error)?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok((response, controller))
}

/// Отправляет GET-запрос с заданным сигналом отмены.
///
/// В отличие от `send_cancellable`, сигнал передаётся снаружи —
/// это позволяет вызывающему коду сохранить `AbortController`
/// и отменить запрос в любой момент, пока он в полёте.
pub async fn send_with_signal(
    url: &str,
    signal: &web_sys::AbortSignal,
) -> Result<web_sys::Response, FetchError> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_signal(Some(signal));

    let request = web_sys::Request::new_with_str_and_init(url, &opts).map_err(classify_error)?;

    let window = web_sys::window().ok_or_else(|| FetchError::Network("no window object".into()))?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(classify_error)?;

    let response: web_sys::Response = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(classify_error)?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok(response)
}

// ============================================================
// Параллельные запросы
// ============================================================

/// Отправляет несколько GET-запросов параллельно через `Promise.all`.
///
/// Возвращает `Vec<Response>` в том же порядке, что и URL-ы.
/// При **HTTP-статусе ≥ 400** хотя бы у одного запроса весь вызов
/// падает с `FetchError::Http`. Сетевые ошибки тоже приводят к `Err`.
///
/// Важно: `Promise.all` сам по себе не падает на 404/500 — `fetch`
/// резолвит промис даже при ошибочных статусах. Проверку `ok()`
/// выполняем мы, после того как промис разрешился.
pub async fn get_all(urls: &[&str]) -> Result<Vec<web_sys::Response>, FetchError> {
    let promises = js_sys::Array::new();
    for url in urls {
        let promise = fetch_as_promise(url);
        promises.push(&promise);
    }

    let all_promise = Promise::all(&promises);
    let result = JsFuture::from(all_promise).await.map_err(classify_error)?;

    let array: js_sys::Array = result
        .dyn_into()
        .map_err(|_| FetchError::Parse("Promise.all result is not an array".into()))?;

    let mut responses = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let value = array.get(i);
        let response: web_sys::Response = value
            .dyn_into()
            .map_err(|_| FetchError::Parse(format!("element {i} is not a Response")))?;

        // fetch резолвит промис даже при 404/500 — проверяем ok() сами.
        if !response.ok() {
            let status = response.status();
            let body = read_error_body(&response).await;
            return Err(FetchError::Http(status, body));
        }

        responses.push(response);
    }

    Ok(responses)
}

/// Отправляет несколько GET-запросов параллельно и не падает при ошибках.
///
/// Возвращает `Vec<Result<Response, FetchError>>` — по одному результату
/// на каждый URL. HTTP-статусы ≥ 400 попадают в `Err(FetchError::Http)`,
/// сетевые сбои — в `Err(FetchError::Network)`, успешные — в `Ok`.
///
/// Аналог `Promise.allSettled` с проверкой `response.ok()`.
pub async fn get_all_settled(
    urls: &[&str],
) -> Result<Vec<Result<web_sys::Response, FetchError>>, FetchError> {
    let promises = js_sys::Array::new();
    for url in urls {
        let promise = fetch_as_promise(url);
        promises.push(&promise);
    }

    let all_promise = Promise::all_settled(&promises);
    let result = JsFuture::from(all_promise).await.map_err(classify_error)?;

    let array: js_sys::Array = result
        .dyn_into()
        .map_err(|_| FetchError::Parse("Promise.allSettled result is not an array".into()))?;

    let mut results = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let entry = array.get(i);
        let status = js_sys::Reflect::get(&entry, &JsValue::from_str("status"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();

        if status == "fulfilled" {
            let value = js_sys::Reflect::get(&entry, &JsValue::from_str("value"))
                .unwrap_or(JsValue::UNDEFINED);
            let response: web_sys::Response = value.dyn_into().map_err(|_| {
                FetchError::Parse(format!("fulfilled element {i} is not a Response"))
            })?;

            // Promise.allSettled считает fulfilled-промис успехом,
            // даже если HTTP-статус 404 или 500. Проверяем ok() сами.
            if !response.ok() {
                let status = response.status();
                let body = read_error_body(&response).await;
                results.push(Err(FetchError::Http(status, body)));
            } else {
                results.push(Ok(response));
            }
        } else {
            // rejected — сетевой сбой или abort.
            let reason = js_sys::Reflect::get(&entry, &JsValue::from_str("reason"))
                .unwrap_or(JsValue::UNDEFINED);
            results.push(Err(classify_error(reason)));
        }
    }

    Ok(results)
}

/// Отправляет несколько GET-запросов и возвращает первый завершившийся.
///
/// Аналог `Promise.race`. Если первый завершившийся запрос упал,
/// вернётся ошибка — даже если остальные ещё в полёте.
pub async fn get_race(urls: &[&str]) -> Result<web_sys::Response, FetchError> {
    let promises = js_sys::Array::new();
    for url in urls {
        let promise = fetch_as_promise(url);
        promises.push(&promise);
    }

    let race_promise = Promise::race(&promises);

    let result = JsFuture::from(race_promise).await.map_err(classify_error)?;

    let response: web_sys::Response = result
        .dyn_into()
        .map_err(|_| FetchError::Parse("Promise.race result is not a Response".into()))?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok(response)
}

/// Отправляет несколько GET-запросов с ограничением параллельности.
///
/// `limit` — максимальное количество одновременных запросов.
/// Если URL-ов больше, они будут запускаться по мере освобождения слотов.
/// Порядок результатов соответствует порядку URL-ов.
/// Отправляет несколько GET-запросов с ограничением параллельности.
pub async fn get_all_limited(
    urls: &[&str],
    limit: usize,
) -> Vec<Result<web_sys::Response, FetchError>> {
    let futures = urls.iter().map(|url| {
        let url = url.to_string();
        async move {
            let opts = web_sys::RequestInit::new();
            opts.set_method("GET");
            opts.set_mode(web_sys::RequestMode::Cors);

            let request =
                web_sys::Request::new_with_str_and_init(&url, &opts).map_err(classify_error)?;

            send_request(&request).await.map_err(classify_error)
        }
    });

    stream::iter(futures)
        .buffer_unordered(limit)
        .collect::<Vec<_>>()
        .await
}

/// Вспомогательная функция: превращает fetch-запрос в JS Promise.
///
/// `Promise.all`/`race`/`allSettled` работают с промисами, а не с
/// Rust Future. Эта функция создаёт `web_sys::Request` и возвращает
/// `Promise` от `fetch`.
fn fetch_as_promise(url: &str) -> Promise {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);

    let request =
        web_sys::Request::new_with_str_and_init(url, &opts).expect("failed to create request");

    let window = web_sys::window().expect("no window");
    window.fetch_with_request(&request)
}

// ============================================================
// Типы ответов и binary data
// ============================================================

/// GET-запрос, возвращает `ArrayBuffer` (сырые байты).
///
/// `ArrayBuffer` — это низкоуровневый буфер, с которым работает
/// JavaScript. Чтобы прочитать байты в Rust, оберните его в
/// `Uint8Array` и вызовите `to_vec()`.
pub async fn get_array_buffer(url: &str) -> Result<js_sys::ArrayBuffer, FetchError> {
    let resp = send_request_classified(url).await?;
    let promise = resp.array_buffer().map_err(classify_error)?;
    let value = JsFuture::from(promise).await.map_err(classify_error)?;

    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| FetchError::Parse("array_buffer result is not an ArrayBuffer".into()))?;

    Ok(buffer)
}

/// GET-запрос, возвращает `Vec<u8>` (сырые байты в Rust).
///
/// Внутри: `Response.arrayBuffer()` → `JsValue` → `Uint8Array` → `Vec<u8>`.
/// Это стандартный способ получить бинарные данные в Rust + WASM.
pub async fn get_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    let buffer = get_array_buffer(url).await?;
    let uint8 = js_sys::Uint8Array::new(&buffer);
    Ok(uint8.to_vec())
}

/// GET-запрос, возвращает `Blob`.
///
/// `Blob` — это объект, представляющий файл или изображение.
/// Его можно отдать в `<img>`, `<video>`, или создать ссылку
/// для скачивания через `URL.createObjectURL`.
pub async fn get_blob(url: &str) -> Result<web_sys::Blob, FetchError> {
    let resp = send_request_classified(url).await?;
    let promise = resp.blob().map_err(classify_error)?;
    let value = JsFuture::from(promise).await.map_err(classify_error)?;

    let blob: web_sys::Blob = value
        .dyn_into()
        .map_err(|_| FetchError::Parse("blob result is not a Blob".into()))?;

    Ok(blob)
}

/// GET-запрос, возвращает `FormData`.
///
/// `FormData` обычно используется для отправки данных, но `fetch`
/// умеет и читать его из ответа — если сервер вернул `multipart/form-data`.
pub async fn get_form_data(url: &str) -> Result<web_sys::FormData, FetchError> {
    let resp = send_request_classified(url).await?;
    let promise = resp.form_data().map_err(classify_error)?;
    let value = JsFuture::from(promise).await.map_err(classify_error)?;

    let form: web_sys::FormData = value
        .dyn_into()
        .map_err(|_| FetchError::Parse("form_data result is not a FormData".into()))?;

    Ok(form)
}

// ============================================================
// Скачивание blob-ресурса
// ============================================================

/// Скачивает ресурс по URL и сохраняет его как файл.
///
/// Работает для blob-URL, созданных **в том же origin**, что и
/// приложение. Для blob-URL из другого origin браузер отклонит
/// запрос — это ограничение безопасности, а не ошибка кода.
///
/// Механика:
/// 1. `fetch` получает `Response`.
/// 2. `response.blob()` превращает тело в `Blob`.
/// 3. `URL.createObjectURL` создаёт временную ссылку на этот blob.
/// 4. Программный клик по `<a download>` запускает скачивание.
/// 5. `URL.revokeObjectURL` освобождает ссылку.
pub async fn download_url(url: &str, filename: &str) -> Result<(), FetchError> {
    let resp = send_request_classified(url).await?;
    let promise = resp.blob().map_err(classify_error)?;
    let value = JsFuture::from(promise).await.map_err(classify_error)?;

    let blob: web_sys::Blob = value
        .dyn_into()
        .map_err(|_| FetchError::Parse("blob result is not a Blob".into()))?;

    let object_url = web_sys::Url::create_object_url_with_blob(&blob).map_err(classify_error)?;

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| FetchError::Network("no document".into()))?;

    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(classify_error)?
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to cast to HtmlAnchorElement".into()))?;

    anchor.set_href(&object_url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display:none").ok();

    let body = document
        .body()
        .ok_or_else(|| FetchError::Network("no document.body".into()))?;
    body.append_child(&anchor).map_err(classify_error)?;

    anchor.click();

    // Убираем временный элемент и освобождаем object URL.
    body.remove_child(&anchor).map_err(classify_error)?;
    web_sys::Url::revoke_object_url(&object_url).map_err(classify_error)?;

    Ok(())
}

// ============================================================
// Data URL
// ============================================================

/// Разбирает `data:`-URL и возвращает `(mime_type, bytes)`.
///
/// Формат: `data:<mime>;base64,<data>` или `data:<mime>,<data>`.
/// Поддерживаются оба варианта. Base64 декодируется, URL-encoded
/// тоже обрабатывается — через простой percent-decode.
///
/// **Почему не через `fetch`.** `data:`-URL не требует сетевого
/// запроса, все данные уже в строке. `fetch` тоже сработает, но
/// тогда мы получим `Response` и будем читать его через `blob()`,
/// что избыточно.
pub fn parse_data_url(url: &str) -> Result<(String, Vec<u8>), FetchError> {
    let rest = url
        .strip_prefix("data:")
        .ok_or_else(|| FetchError::Parse("not a data URL".into()))?;

    // Разделяем метаданные и тело по первой запятой.
    let comma = rest
        .find(',')
        .ok_or_else(|| FetchError::Parse("data URL has no comma".into()))?;

    let (meta, payload) = rest.split_at(comma);
    let payload = &payload[1..]; // убираем саму запятую

    // meta может быть "video/webm;base64" или "video/webm;charset=utf-8".
    let mut parts = meta.split(';');
    let mime = parts.next().unwrap_or("text/plain").to_string();
    let is_base64 = parts.any(|p| p.trim() == "base64");

    if is_base64 {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let bytes = STANDARD
            .decode(payload)
            .map_err(|e| FetchError::Parse(format!("base64 decode error: {e}")))?;
        Ok((mime, bytes))
    } else {
        // URL-encoded (для текстовых data URL).
        let bytes = percent_decode(payload.as_bytes());
        Ok((mime, bytes))
    }
}

/// Простой percent-decode для `data:`-URL без base64.
fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            let hi = (input[i + 1] as char).to_digit(16);
            let lo = (input[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

/// Скачивает `data:`-URL как файл.
///
/// В отличие от `download_url`, здесь нет сетевого запроса:
/// данные уже в строке. Мы декодируем base64, упаковываем байты
/// в `Blob` и запускаем скачивание через `<a download>`.
pub async fn download_data_url(url: &str, filename: &str) -> Result<usize, FetchError> {
    let (mime, bytes) = parse_data_url(url)?;

    // Создаём Uint8Array из байтов и оборачиваем в Blob.
    let uint8 = js_sys::Uint8Array::from(bytes.as_slice());
    let array = js_sys::Array::new();
    array.push(&uint8.buffer());

    let options = web_sys::BlobPropertyBag::new();
    options.set_type(&mime);

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)
        .map_err(classify_error)?;

    let size = bytes.len();
    let object_url = web_sys::Url::create_object_url_with_blob(&blob).map_err(classify_error)?;

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| FetchError::Network("no document".into()))?;

    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(classify_error)?
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to cast to HtmlAnchorElement".into()))?;

    anchor.set_href(&object_url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display:none").ok();

    let body = document
        .body()
        .ok_or_else(|| FetchError::Network("no document.body".into()))?;
    body.append_child(&anchor).map_err(classify_error)?;
    anchor.click();
    body.remove_child(&anchor).map_err(classify_error)?;
    web_sys::Url::revoke_object_url(&object_url).map_err(classify_error)?;

    Ok(size)
}

// ============================================================
// Streams: чтение ответа по частям
// ============================================================

/// Отправляет GET-запрос и возвращает `ReadableStream` из тела ответа.
///
/// `response.body()` возвращает `Option<ReadableStream>`: `None` — если
/// тело пустое (например, ответ 204 No Content). Мы считаем это ошибкой.
///
/// **Важно:** поток можно прочитать только один раз. После `get_stream`
/// тело ответа «заблокировано» — повторный вызов `resp.json()` или
/// `resp.text()` на том же `Response` упадёт с ошибкой.
pub async fn get_stream(url: &str) -> Result<ReadableStream, FetchError> {
    let resp = send_request_classified(url).await?;

    resp.body()
        .ok_or_else(|| FetchError::Parse("response has no body (204 or empty)".into()))
}

/// Читает весь поток в `Vec<u8>`.
///
/// Проходит цикл: `reader.read()` → `{ done, value }`. Пока `done == false`,
/// каждый `value` — это `Uint8Array`, который превращается в байты и
/// добавляется к результату.
///
/// **Память.** Этот способ держит весь файл в памяти. Для больших файлов
/// используйте `read_stream_with_callback`, чтобы обрабатывать чанки
/// по мере поступления.
pub async fn read_stream_to_vec(stream: &ReadableStream) -> Result<Vec<u8>, FetchError> {
    let reader: ReadableStreamDefaultReader = stream
        .get_reader()
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to get stream reader".into()))?;

    let mut result = Vec::new();

    loop {
        let promise = reader.read();
        let read_result = JsFuture::from(promise).await.map_err(classify_error)?;

        // read_result — объект { value: any, done: bool }.
        let done = js_sys::Reflect::get(&read_result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if done {
            break;
        }

        let value = js_sys::Reflect::get(&read_result, &JsValue::from_str("value"))
            .map_err(classify_error)?;

        // Для бинарного потока value — это Uint8Array.
        let chunk: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| FetchError::Parse("chunk is not a Uint8Array".into()))?;

        result.extend_from_slice(&chunk.to_vec());
    }

    // Освобождаем читателя, чтобы поток можно было использовать повторно.
    reader.release_lock();

    Ok(result)
}

/// Читает поток и вызывает `on_chunk` на каждый chunk.
///
/// `on_chunk` получает два аргумента: накопленное количество байт
/// и размер текущего чанка. Это позволяет UI-коду обновлять
/// прогресс-бар без хранения всего файла в памяти.
///
/// **Backpressure.** `JsFuture::from(reader.read()).await` автоматически
/// приостанавливает чтение, пока обработчик не завершится. Если
/// `on_chunk` работает медленно, поток не «убежит» вперёд.
pub async fn read_stream_with_callback<F>(
    stream: &ReadableStream,
    mut on_chunk: F,
) -> Result<usize, FetchError>
where
    F: FnMut(usize, usize),
{
    let reader: ReadableStreamDefaultReader = stream
        .get_reader()
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to get stream reader".into()))?;

    let mut total: usize = 0;

    loop {
        let promise = reader.read();
        let read_result = JsFuture::from(promise).await.map_err(classify_error)?;

        let done = js_sys::Reflect::get(&read_result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if done {
            break;
        }

        let value = js_sys::Reflect::get(&read_result, &JsValue::from_str("value"))
            .map_err(classify_error)?;

        let chunk: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| FetchError::Parse("chunk is not a Uint8Array".into()))?;

        let chunk_size = chunk.length() as usize;
        total += chunk_size;
        on_chunk(total, chunk_size);
    }

    reader.release_lock();

    Ok(total)
}

/// Читает поток и возвращает список чанков.
///
/// Каждый элемент — `Vec<u8>` с байтами одного чанка. Удобно для
/// тестов и для случаев, когда чанки нужно обработать отдельно
/// (например, сохранить каждый в свой файл).
pub async fn read_stream_chunks(stream: &ReadableStream) -> Result<Vec<Vec<u8>>, FetchError> {
    let reader: ReadableStreamDefaultReader = stream
        .get_reader()
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to get stream reader".into()))?;

    let mut chunks = Vec::new();

    loop {
        let promise = reader.read();
        let read_result = JsFuture::from(promise).await.map_err(classify_error)?;

        let done = js_sys::Reflect::get(&read_result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if done {
            break;
        }

        let value = js_sys::Reflect::get(&read_result, &JsValue::from_str("value"))
            .map_err(classify_error)?;

        let chunk: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| FetchError::Parse("chunk is not a Uint8Array".into()))?;

        chunks.push(chunk.to_vec());
    }

    reader.release_lock();

    Ok(chunks)
}

// ============================================================
// CORS и режимы запроса
// ============================================================

/// Создаёт GET-запрос в режиме `cors`.
///
/// Это **режим по умолчанию** для cross-origin запросов. Браузер:
/// 1. Проверяет, является ли запрос простым (simple).
/// 2. Если нет — отправляет preflight OPTIONS.
/// 3. Только после успешного preflight делает настоящий запрос.
///
/// Сервер должен ответить заголовками `Access-Control-Allow-Origin`
/// и сопутствующими, иначе браузер заблокирует ответ.
///
/// Мы используем `cors` во всех предыдущих главах, потому что
/// запросы шли на другой origin (JSONPlaceholder, наш auth-server
/// на другом порту). Если бы мы использовали `same-origin`, браузер
/// отклонил бы их сразу, не отправляя на сервер.
pub fn make_cors_request(url: &str) -> Result<web_sys::Request, JsValue> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);

    web_sys::Request::new_with_str_and_init(url, &opts)
}

/// Создаёт GET-запрос в режиме `same-origin`.
///
/// Браузер **отклонит запрос**, если URL ведёт на другой origin.
/// Не отправит ни preflight, ни сам запрос — просто выбросит
/// `TypeError: Failed to fetch`.
///
/// Зачем это нужно:
/// - Безопасность. Если вы точно знаете, что URL должен быть
///   локальным, режим `same-origin` гарантирует, что запрос
///   не уйдёт наружу даже при опечатке в URL.
/// - Экономия. Не отправляется preflight, что экономит один
///   round-trip.
///
/// Для нашего приложения, работающего через Vite-прокси, запросы
/// к относительным путям (`/api/me`) идут на `localhost:5173` —
/// это тот же origin, что и страница. Режим `same-origin` для них
/// работает без CORS.
pub fn make_same_origin_request(url: &str) -> Result<web_sys::Request, JsValue> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::SameOrigin);

    web_sys::Request::new_with_str_and_init(url, &opts)
}

/// Создаёт GET-запрос с credentials: `include`.
///
/// `credentials` управляет тем, отправляет ли браузер cookies
/// и HTTP-аутентификацию вместе с запросом. Три значения:
///
/// - `Omit` — никогда не отправлять cookies.
/// - `SameOrigin` — только если запрос на тот же origin. **Дефолт.**
/// - `Include` — отправлять всегда, даже на cross-origin.
///
/// **Важно.** При `include` сервер **обязан** ответить заголовком
/// `Access-Control-Allow-Credentials: true`. Просто `Allow-Origin: *`
/// недостаточно — браузер отклонит ответ, потому что wildcard
/// несовместим с credentials. Сервер должен явно перечислить
/// origin (`Access-Control-Allow-Origin: https://app.example.com`).
///
/// `CorsLayer::permissive()` в нашем auth-server **не разрешает**
/// credentials — именно поэтому такой запрос демонстративно
/// упадёт на preflight.
pub fn make_request_with_credentials(url: &str) -> Result<web_sys::Request, JsValue> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);
    opts.set_credentials(web_sys::RequestCredentials::Include);

    web_sys::Request::new_with_str_and_init(url, &opts)
}

/// Отправляет запрос и возвращает классифицированный результат.
///
/// Общая обёртка для всех трёх функций выше. Проверяет `ok()`
/// и читает тело ошибки, как `send_request_classified`.
pub async fn send_prepared_request(
    request: &web_sys::Request,
) -> Result<web_sys::Response, FetchError> {
    let window = web_sys::window().ok_or_else(|| FetchError::Network("no window object".into()))?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(request))
        .await
        .map_err(classify_error)?;

    let response: web_sys::Response = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(classify_error)?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok(response)
}

// ============================================================
// Fetch в Worker и Window
// ============================================================

/// Возвращает `true`, если код выполняется в контексте Worker'а.
///
/// В Worker'е нет `window` — есть `WorkerGlobalScope`. Проверяем это
/// через попытку привести глобальный объект к типу `WorkerGlobalScope`.
pub fn is_worker_scope() -> bool {
    js_sys::global()
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .is_ok()
}

/// Возвращает глобальный объект как `WorkerGlobalScope`.
///
/// Если код выполняется не в Worker'е, вернёт `Err(JsValue)`.
pub fn get_worker_scope() -> Result<web_sys::WorkerGlobalScope, JsValue> {
    js_sys::global()
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .map_err(|_| JsValue::from_str("not a Worker scope"))
}

/// Отправляет `Request` и возвращает `Response`.
///
/// **Универсальная функция:** работает и в Window, и в Worker'е.
/// Использует `js_sys::global()` вместо `web_sys::window()` —
/// в обоих контекстах у глобального объекта есть метод `fetch`.
pub async fn fetch_in_global_scope(
    request: &web_sys::Request,
) -> Result<web_sys::Response, FetchError> {
    let global = js_sys::global();

    // Получаем функцию fetch через Reflect::get, потому что у
    // js_sys::global() нет статического типа с методом fetch.
    let fetch_fn = js_sys::Reflect::get(&global, &JsValue::from_str("fetch"))
        .map_err(classify_error)?;

    let fetch_fn: js_sys::Function = fetch_fn
        .dyn_into()
        .map_err(|_| FetchError::Parse("global.fetch is not a function".into()))?;

    // Передаём global как this — fetch в браузере не зависит от this,
    // но так корректнее.
    let promise = fetch_fn
        .call1(&global, request)
        .map_err(classify_error)?;

    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| FetchError::Parse("fetch did not return a Promise".into()))?;

    let resp_value = JsFuture::from(promise).await.map_err(classify_error)?;

    let response: web_sys::Response = resp_value
        .dyn_into()
        .map_err(classify_error)?;

    if !response.ok() {
        let status = response.status();
        let body = read_error_body(&response).await;
        return Err(FetchError::Http(status, body));
    }

    Ok(response)
}

/// Универсальная отправка: создаёт `Request`, отправляет через
/// `fetch_in_global_scope`, читает тело как текст.
pub async fn send_in_global_scope(url: &str) -> Result<String, FetchError> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::Cors);

    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(classify_error)?;

    let response = fetch_in_global_scope(&request).await?;

    let promise = response.text().map_err(classify_error)?;
    let value = JsFuture::from(promise).await.map_err(classify_error)?;

    Ok(value.as_string().unwrap_or_default())
}