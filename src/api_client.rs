//! API-клиент: единая точка для всех HTTP-запросов.
//!
//! Инкапсулирует:
//! - базовый URL;
//! - общие заголовки (Accept, Authorization, Content-Type);
//! - хранилище токенов с прозрачным refresh при 401;
//! - конфигурацию retry и timeout (через AbortController);
//! - кэш GET-ответов с TTL;
//! - обработку 429 с заголовком Retry-After;
//! - опциональное логирование запросов.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::auth::TokenStoreHandle;
use crate::fetch::{self, FetchError, RetryConfig};
use crate::log;

/// Запись в кэше GET-ответов.
#[derive(Clone)]
struct CachedEntry {
    body: String,
    expires_at: f64, // timestamp в миллисекундах (js_sys::Date::now())
}

/// HTTP-клиент с состоянием.
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    default_headers: Vec<(String, String)>,
    token_store: Option<TokenStoreHandle>,
    retry_config: RetryConfig,
    timeout_ms: Option<u32>,
    verbose: bool,
    cache: Rc<RefCell<HashMap<String, CachedEntry>>>,
    cache_ttl_ms: u64,
    rate_limit_until: Rc<Cell<f64>>,
}

impl ApiClient {
    /// Создаёт клиент с базовым URL и настройками по умолчанию.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            default_headers: vec![("Accept".to_string(), "application/json".to_string())],
            token_store: None,
            retry_config: RetryConfig::default(),
            timeout_ms: None,
            verbose: false,
            cache: Rc::new(RefCell::new(HashMap::new())),
            cache_ttl_ms: 0,
            rate_limit_until: Rc::new(Cell::new(0.0)),
        }
    }

    /// Устанавливает токен напрямую (без хранилища).
    ///
    /// Полезно, когда токен известен заранее. При 401 клиент **не**
    /// сможет обновить его — для этого нужен `with_token_store`.
    pub fn with_token(mut self, token: &str) -> Self {
        let store = crate::auth::TokenStore::new();
        store.borrow_mut().set_tokens(token.to_string(), None);
        self.token_store = Some(store);
        self
    }

    /// Передаёт клиенту хранилище токенов.
    ///
    /// Если задано, клиент при получении 401 попробует обновить токен
    /// через `auth::try_refresh` и повторит запрос один раз.
    pub fn with_token_store(mut self, store: TokenStoreHandle) -> Self {
        self.token_store = Some(store);
        self
    }

    /// Убирает токен (logout).
    pub fn clear_token(&mut self) {
        if let Some(store) = &self.token_store {
            store.borrow_mut().clear();
        }
    }

    /// Добавляет общий заголовок ко всем запросам.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.default_headers
            .push((name.to_string(), value.to_string()));
        self
    }

    /// Устанавливает заголовок, заменяя прежние с таким именем.
    pub fn set_header(&mut self, name: &str, value: &str) {
        self.default_headers
            .retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self.default_headers
            .push((name.to_string(), value.to_string()));
    }

    /// Устанавливает конфигурацию retry.
    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Устанавливает таймаут в миллисекундах.
    ///
    /// Таймаут реализован через `AbortController` — запрос **реально
    /// отменяется**, а не просто прекращается ожидание. Ошибка отмены
    /// возвращается как `FetchError::Timeout`.
    pub fn with_timeout(mut self, ms: u32) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Включает логирование запросов через `crate::log!`.
    pub fn with_logging(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Включает кэширование GET-ответов.
    ///
    /// `ttl_ms` — время жизни записи в миллисекундах. `0` отключает
    /// кэш. Кэшируются только успешные GET-ответы. Ключ — полный URL.
    pub fn with_cache(mut self, ttl_ms: u64) -> Self {
        self.cache_ttl_ms = ttl_ms;
        self
    }

    /// Базовый URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // ============================================================
    // Сборка URL и запроса
    // ============================================================

    fn full_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }

        let path_with_slash: String = if path.starts_with('/') {
            path.to_string()
        } else {
            return format!("/{path}");
        };

        return format!("{}{}", self.base_url, path_with_slash);
    }

    /// Читает Retry-After из ответа (в миллисекундах).
    fn parse_retry_after(response: &web_sys::Response) -> Option<u64> {
        let value = response
            .headers()
            .get("Retry-After")
            .ok()
            .flatten()?;
        value.trim().parse::<u64>().ok().map(|s| s * 1000)
    }

    /// Ждёт, если активен rate limit.
    async fn wait_for_rate_limit(&self) {
        let now = js_sys::Date::now();
        let until = self.rate_limit_until.get();
        if now < until {
            let wait_ms = (until - now).max(0.0) as u32;
            if self.verbose {
                log!("ApiClient: rate limit активен, ждём {wait_ms} мс");
            }
            gloo_timers::future::TimeoutFuture::new(wait_ms).await;
        }
    }

    // ============================================================
    // Отправка одной попытки
    // ============================================================

    async fn send_once(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        extra_headers: &[(&str, &str)],
    ) -> Result<web_sys::Response, FetchError> {
        let url = self.full_url(path);

        let opts = web_sys::RequestInit::new();
        opts.set_method(method);
        opts.set_mode(web_sys::RequestMode::Cors);

        if let Some(b) = body {
            opts.set_body(&JsValue::from_str(b));
        }

        // --- Timeout через AbortController ---
        let controller = if self.timeout_ms.is_some() {
            Some(
                web_sys::AbortController::new()
                    .map_err(fetch::classify_error)?,
            )
        } else {
            None
        };

        let timeout_id: Option<i32> = match (self.timeout_ms, &controller) {
            (Some(ms), Some(ctrl)) => {
                let ctrl_clone = ctrl.clone();
                let callback = Closure::once_into_js(move || {
                    ctrl_clone.abort();
                });
                let window = web_sys::window()
                    .ok_or_else(|| FetchError::Network("no window".into()))?;
                let id = window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.unchecked_ref(),
                        ms as i32,
                    )
                    .map_err(fetch::classify_error)?;
                Some(id)
            }
            _ => None,
        };

        if let Some(ctrl) = &controller {
            opts.set_signal(Some(&ctrl.signal()));
        }

        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(fetch::classify_error)?;

        // Общие заголовки.
        for (name, value) in &self.default_headers {
            request
                .headers()
                .set(name, value)
                .map_err(fetch::classify_error)?;
        }

        // Authorization из хранилища.
        if let Some(store) = &self.token_store {
            if let Some(token) = store.borrow().access_token.clone() {
                request
                    .headers()
                    .set("Authorization", &format!("Bearer {token}"))
                    .map_err(fetch::classify_error)?;
            }
        }

        // Заголовки конкретного запроса (перезаписывают общие).
        for (name, value) in extra_headers {
            request
                .headers()
                .set(name, value)
                .map_err(fetch::classify_error)?;
        }

        if self.verbose {
            log!("ApiClient: {method} {url}");
        }

        // --- Отправка ---
        let window = web_sys::window()
            .ok_or_else(|| FetchError::Network("no window".into()))?;

        let result = JsFuture::from(window.fetch_with_request(&request)).await;

        // Снимаем таймер — запрос завершился (успешно или нет).
        if let Some(id) = timeout_id {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
        }

        let value = match result {
            Ok(v) => v,
            Err(e) => {
                // Если контроллер отменён и у нас был таймаут — значит, это timeout.
                if let (Some(ms), Some(ctrl)) = (self.timeout_ms, &controller) {
                    if ctrl.signal().aborted() {
                        if self.verbose {
                            log!("ApiClient: timeout {ms} мс для {url}");
                        }
                        return Err(FetchError::Timeout(ms));
                    }
                }
                return Err(fetch::classify_error(e));
            }
        };

        let response: web_sys::Response = value
            .dyn_into()
            .map_err(fetch::classify_error)?;

        if self.verbose {
            log!("ApiClient: {} {} → {}", method, url, response.status());
        }

        // --- Rate limit ---
        if response.status() == 429 {
            if let Some(ms) = Self::parse_retry_after(&response) {
                let now = js_sys::Date::now();
                self.rate_limit_until.set(now + ms as f64);
                if self.verbose {
                    log!("ApiClient: 429, Retry-After = {ms} мс");
                }
            }
        }

        Ok(response)
    }

    // ============================================================
    // Retry-цикл
    // ============================================================

    async fn try_with_retry(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        extra_headers: &[(&str, &str)],
    ) -> Result<web_sys::Response, FetchError> {
        let is_idempotent = matches!(method, "GET" | "HEAD" | "PUT" | "DELETE");
        let mut last_error: Option<FetchError> = None;

        for attempt in 0..self.retry_config.max_attempts {
            match self.send_once(method, path, body, extra_headers).await {
                Ok(resp) => {
                    // Успех — возвращаем.
                    if resp.ok() {
                        return Ok(resp);
                    }

                    let status = resp.status();

                    // Не повторяем неидемпотентные методы и клиентские ошибки
                    // (кроме 408 и 429).
                    let is_retriable_status =
                        status >= 500 || status == 429 || status == 408;

                    if !is_idempotent || !is_retriable_status {
                        return Ok(resp); // вызывающий сам обработает
                    }

                    let is_last = attempt + 1 >= self.retry_config.max_attempts;
                    if is_last {
                        return Ok(resp);
                    }

                    // Пауза перед повтором. Retry-After, если сервер его дал,
                    // уже обработан в rate_limit_until — ждём через wait_for_rate_limit.
                    let delay = Self::parse_retry_after(&resp)
                        .unwrap_or_else(|| fetch::calculate_backoff(attempt, &self.retry_config));

                    // Поглощаем тело, чтобы не оставлять висеть ответ.
                    let _ = fetch::read_error_body(&resp).await;

                    gloo_timers::future::TimeoutFuture::new(
                        delay.min(u32::MAX as u64) as u32,
                    )
                    .await;
                }
                Err(error) => {
                    let is_last = attempt + 1 >= self.retry_config.max_attempts;
                    let should_retry =
                        is_idempotent && fetch::is_retriable(&error);

                    if !should_retry || is_last {
                        return Err(error);
                    }

                    let delay = fetch::calculate_backoff(attempt, &self.retry_config);
                    gloo_timers::future::TimeoutFuture::new(
                        delay.min(u32::MAX as u64) as u32,
                    )
                    .await;

                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            FetchError::Network("retry loop exhausted".into())
        }))
    }

    // ============================================================
    // Верхний уровень: rate limit + refresh + HTTP-ошибки
    // ============================================================

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        extra_headers: &[(&str, &str)],
    ) -> Result<web_sys::Response, FetchError> {
        self.wait_for_rate_limit().await;

        let resp = self
            .try_with_retry(method, path, body, extra_headers)
            .await?;

        // Прозрачный refresh токена при 401.
        if resp.status() == 401 {
            if let Some(store) = &self.token_store {
                let refresh_token_present = store.borrow().refresh_token.is_some();
                if refresh_token_present {
                    if self.verbose {
                        log!("ApiClient: 401, пробуем refresh");
                    }
                    match crate::auth::try_refresh(store).await {
                        Ok(true) => {
                            // Один повтор с обновлённым токеном.
                            let resp2 = self
                                .try_with_retry(method, path, body, extra_headers)
                                .await?;
                            return self.finalize(resp2).await;
                        }
                        Ok(false) => {
                            if self.verbose {
                                log!("ApiClient: refresh не удался");
                            }
                        }
                        Err(e) => {
                            if self.verbose {
                                log!("ApiClient: ошибка refresh: {:?}", e);
                            }
                        }
                    }
                }
            }
        }

        self.finalize(resp).await
    }

    /// Преобразует `!ok` в `FetchError::Http`, `ok` — возвращает как есть.
    async fn finalize(
        &self,
        resp: web_sys::Response,
    ) -> Result<web_sys::Response, FetchError> {
        if resp.ok() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = fetch::read_error_body(&resp).await;
        Err(FetchError::Http(status, body))
    }

    // ============================================================
    // Разбор тела
    // ============================================================

    async fn parse_json<T: DeserializeOwned>(
        body: &str,
    ) -> Result<T, FetchError> {
        serde_json::from_str(body)
            .map_err(|e| FetchError::Parse(format!("deserialize error: {e}")))
    }

    // ============================================================
    // Публичные методы
    // ============================================================

    /// GET-запрос с десериализацией JSON.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, FetchError> {
        let url = self.full_url(path);

        // Проверяем кэш.
        if self.cache_ttl_ms > 0 {
            let now = js_sys::Date::now();
            let cache = self.cache.borrow();
            if let Some(entry) = cache.get(&url) {
                if now < entry.expires_at {
                    if self.verbose {
                        log!("ApiClient: cache hit для {url}");
                    }
                    return Self::parse_json::<T>(&entry.body).await;
                }
            }
        }

        let resp = self.send("GET", path, None, &[]).await?;
        let body = fetch::read_error_body(&resp).await;

        if self.cache_ttl_ms > 0 {
            let now = js_sys::Date::now();
            let entry = CachedEntry {
                body: body.clone(),
                expires_at: now + self.cache_ttl_ms as f64,
            };
            self.cache.borrow_mut().insert(url, entry);
        }

        Self::parse_json::<T>(&body).await
    }

    /// GET-запрос без разбора тела.
    pub async fn get_raw(&self, path: &str) -> Result<web_sys::Response, FetchError> {
        self.send("GET", path, None, &[]).await
    }

    /// POST-запрос с JSON-телом.
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, FetchError> {
        let body_str = serde_json::to_string(body)
            .map_err(|e| FetchError::Parse(format!("serialize error: {e}")))?;

        let resp = self
            .send(
                "POST",
                path,
                Some(&body_str),
                &[("Content-Type", "application/json")],
            )
            .await?;

        let body_text = fetch::read_error_body(&resp).await;
        Self::parse_json::<T>(&body_text).await
    }

    /// PUT-запрос с JSON-телом.
    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, FetchError> {
        let body_str = serde_json::to_string(body)
            .map_err(|e| FetchError::Parse(format!("serialize error: {e}")))?;

        let resp = self
            .send(
                "PUT",
                path,
                Some(&body_str),
                &[("Content-Type", "application/json")],
            )
            .await?;

        let body_text = fetch::read_error_body(&resp).await;
        Self::parse_json::<T>(&body_text).await
    }

    /// DELETE-запрос.
    pub async fn delete(&self, path: &str) -> Result<web_sys::Response, FetchError> {
        self.send("DELETE", path, None, &[]).await
    }

    /// Сбрасывает кэш.
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }
}