//! Помощники для работы с навигацией и URL.
//!
//! ## Что в этом модуле
//!
//! - `Location` — текущий URL, `assign`, `replace`, `reload`, `set_hash`, `set_search`.
//! - `Url` — парсинг URL, извлечение компонентов.
//! - `UrlSearchParams` — работа с query-параметрами.
//! - `History` — `pushState`, `replaceState`, `back`, `forward`.
//! - Navigation API — экспериментальный API, требует `web_sys_unstable_apis`.
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use web_sys::{History, Location, Url, UrlSearchParams};

// ============================================================
// Location
// ============================================================

/// Возвращает `window.location`.
pub fn location() -> Location {
    web_sys::window()
        .expect("no window")
        .location()
}

/// Возвращает текущий `href` (полный URL).
pub fn current_href() -> String {
    location().href().unwrap_or_default()
}

/// Возвращает текущий `pathname` (путь без query и hash).
pub fn current_pathname() -> String {
    location().pathname().unwrap_or_default()
}

/// Возвращает текущий `search` (query-строка с `?`).
pub fn current_search() -> String {
    location().search().unwrap_or_default()
}

/// Возвращает текущий `hash` (с `#`).
pub fn current_hash() -> String {
    location().hash().unwrap_or_default()
}

/// Переходит по URL (добавляет в историю).
pub fn navigate_to(url: &str) -> Result<(), JsValue> {
    location().assign(url)
}

/// Заменяет текущий URL (без добавления в историю).
pub fn replace_url(url: &str) -> Result<(), JsValue> {
    location().replace(url)
}

/// Перезагружает страницу.
pub fn reload() -> Result<(), JsValue> {
    location().reload()
}

/// Устанавливает hash. Пустая строка убирает hash.
///
/// Вызывает событие `hashchange` в window.
pub fn set_hash(hash: &str) -> Result<(), JsValue> {
    location().set_hash(hash)
}

/// Устанавливает query-строку. Пустая строка убирает query.
pub fn set_search(search: &str) -> Result<(), JsValue> {
    location().set_search(search)
}

// ============================================================
// Url
// ============================================================

/// Парсит URL.
pub fn parse_url(url: &str) -> Result<Url, JsValue> {
    Url::new(url)
}

/// Возвращает `origin` URL (protocol + host + port).
pub fn url_origin(url: &Url) -> String {
    url.origin()
}

/// Возвращает `pathname` URL.
pub fn url_pathname(url: &Url) -> String {
    url.pathname()
}

/// Возвращает `search` URL (query-строка).
pub fn url_search(url: &Url) -> String {
    url.search()
}

/// Возвращает `hash` URL.
pub fn url_hash(url: &Url) -> String {
    url.hash()
}

/// Возвращает `searchParams` URL.
pub fn url_search_params(url: &Url) -> UrlSearchParams {
    url.search_params()
}

// ============================================================
// UrlSearchParams
// ============================================================

/// Создаёт пустой `UrlSearchParams`.
pub fn new_search_params() -> Result<UrlSearchParams, JsValue> {
    UrlSearchParams::new()
}

/// Создаёт `UrlSearchParams` из строки.
pub fn search_params_from_str(s: &str) -> Result<UrlSearchParams, JsValue> {
    UrlSearchParams::new_with_str(s)
}

/// Читает параметр.
pub fn param_get(params: &UrlSearchParams, name: &str) -> Option<String> {
    params.get(name)
}

/// Устанавливает параметр (заменяет все вхождения).
pub fn param_set(params: &UrlSearchParams, name: &str, value: &str) {
    params.set(name, value);
}

/// Добавляет параметр (не удаляет существующие).
pub fn param_append(params: &UrlSearchParams, name: &str, value: &str) {
    params.append(name, value);
}

/// Удаляет параметр.
pub fn param_delete(params: &UrlSearchParams, name: &str) {
    params.delete(name);
}

/// Проверяет наличие параметра.
pub fn param_has(params: &UrlSearchParams, name: &str) -> bool {
    params.has(name)
}

/// Возвращает строковое представление query-параметров.
pub fn params_to_string(params: &UrlSearchParams) -> String {
    params.to_string().into()
}

/// Парсит query-параметры из текущего URL.
pub fn current_search_params() -> Result<UrlSearchParams, JsValue> {
    search_params_from_str(&current_search())
}

// ============================================================
// History
// ============================================================

/// Возвращает `window.history`.
pub fn history() -> History {
    web_sys::window()
        .expect("no window")
        .history()
        .expect("no history")
}

/// Добавляет запись в историю.
pub fn push_state(data: &JsValue, title: &str, url: Option<&str>) -> Result<(), JsValue> {
    match url {
        Some(u) => history().push_state_with_url(data, title, Some(u)),
        None => history().push_state(data, title),
    }
}

/// Заменяет текущую запись в истории.
pub fn replace_state(data: &JsValue, title: &str, url: Option<&str>) -> Result<(), JsValue> {
    match url {
        Some(u) => history().replace_state_with_url(data, title, Some(u)),
        None => history().replace_state(data, title),
    }
}

/// Назад в истории.
pub fn history_back() -> Result<(), JsValue> {
    history().back()
}

/// Вперёд в истории.
pub fn history_forward() -> Result<(), JsValue> {
    history().forward()
}

/// Переход на `delta` шагов в истории.
pub fn history_go(delta: i32) -> Result<(), JsValue> {
    history().go_with_delta(delta)
}

/// Длина истории.
pub fn history_length() -> u32 {
    history().length().unwrap_or(0)
}

/// Текущее состояние истории.
pub fn history_state() -> JsValue {
    history().state().unwrap_or(JsValue::NULL)
}

// ============================================================
// Navigation API (экспериментальный)
// ============================================================

/// Проверяет, доступен ли Navigation API в текущем браузере.
///
/// Возвращает `true`, если `window.navigation` существует.
/// Не зависит от `web_sys_unstable_apis`: работает через `Reflect`.
pub fn is_navigation_api_available() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    js_sys::Reflect::has(&window, &JsValue::from_str("navigation")).unwrap_or(false)
}

/// Возвращает `window.navigation`.
///
/// **Требует `--cfg=web_sys_unstable_apis`.**
#[cfg(web_sys_unstable_apis)]
pub fn navigation() -> web_sys::Navigation {
    use wasm_bindgen::JsCast;

    let nav = js_sys::Reflect::get(
        &web_sys::window().unwrap(),
        &JsValue::from_str("navigation"),
    )
    .expect("no navigation");

    nav.dyn_into::<web_sys::Navigation>()
        .expect("navigation is not Navigation")
}

/// Возвращает текущий URL через Navigation API.
///
/// **Требует `--cfg=web_sys_unstable_apis`.**
#[cfg(web_sys_unstable_apis)]
pub fn navigation_current_url() -> String {
    let nav = navigation();
    nav.current_entry()
        .and_then(|entry| entry.url())
        .unwrap_or_default()
}