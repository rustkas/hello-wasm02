//! Управление токенами на клиенте.
//!
//! Access-токен хранится только в памяти — это безопаснее, чем `localStorage`:
//! XSS-атака не сможет прочитать его через `localStorage.getItem`.
//! Refresh-токен в реальном приложении должен жить в HttpOnly-cookie,
//! но для простоты учебного примера держим и его в памяти.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use wasm_bindgen::JsValue;

use crate::fetch;

/// Базовый URL API. Пустой по умолчанию — тогда используются
/// относительные пути, и запросы идут через прокси dev-сервера Vite.
/// В тестах выставляется в `http://127.0.0.1:3000`.
static API_BASE: OnceLock<String> = OnceLock::new();

/// Задаёт базовый URL API. Идемпотентно: повторные вызовы игнорируются.
pub fn set_api_base(url: &str) {
    let _ = API_BASE.set(url.trim_end_matches('/').to_string());
}

fn api_base() -> &'static str {
    API_BASE.get().map(|s| s.as_str()).unwrap_or("")
}

fn api_url(path: &str) -> String {
    return format!("{}{}", api_base(), path)
}

/// Разделяемое хранилище токенов.
#[derive(Default)]
pub struct TokenStore {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

/// Удобный псевдоним для передачи хранилища между замыканиями.
pub type TokenStoreHandle = Rc<RefCell<TokenStore>>;

impl TokenStore {
    /// Создаёт новое пустое хранилище, обёрнутое в `Rc<RefCell<…>>`.
    pub fn new() -> TokenStoreHandle {
        Rc::new(RefCell::new(Self::default()))
    }

    /// Устанавливает оба токена. `refresh` опционален, потому что
    /// при обновлении access-токена новый refresh обычно не выдаётся.
    pub fn set_tokens(&mut self, access: String, refresh: Option<String>) {
        self.access_token = Some(access);
        if let Some(r) = refresh {
            self.refresh_token = Some(r);
        }
    }

    /// Полная очистка — используется при logout и при неудачном refresh.
    pub fn clear(&mut self) {
        self.access_token = None;
        self.refresh_token = None;
    }
}

/// Логин: отправляет credentials, сохраняет оба токена.
pub async fn login(
    store: &TokenStoreHandle,
    username: &str,
    password: &str,
) -> Result<(), JsValue> {
    let url = api_url("/api/auth/login");
    let body = serde_json::json!({
        "username": username,
        "password": password
    });

    let resp = fetch::post_json_raw(&url, &body).await?;

    if resp.status() != 200 {
        return Err(JsValue::from_str(&format!(
            "login failed: status {}",
            resp.status()
        )));
    }

    let json: serde_json::Value = fetch::response_json(&resp).await?;

    let access = json["access_token"]
        .as_str()
        .ok_or_else(|| JsValue::from_str("no access_token"))?
        .to_string();
    let refresh = json["refresh_token"].as_str().map(|s| s.to_string());

    store.borrow_mut().set_tokens(access, refresh);
    Ok(())
}

/// Пытается обновить access-токен через refresh-токен.
///
/// Возвращает:
/// - `Ok(true)` — refresh прошёл, access-токен обновлён;
/// - `Ok(false)` — refresh-токена нет или сервер отказал (хранилище очищено);
/// - `Err(_)` — транспортная ошибка (сеть, CORS и т.п.).
pub async fn try_refresh(store: &TokenStoreHandle) -> Result<bool, JsValue> {
    let refresh = match store.borrow().refresh_token.clone() {
        Some(r) => r,
        None => return Ok(false),
    };

    let url = api_url("/api/auth/refresh");
    let body = serde_json::json!({ "refresh_token": refresh });
    let resp = fetch::post_json_raw(&url, &body).await;

    match resp {
        Ok(r) if r.status() == 200 => {
            let json: serde_json::Value = fetch::response_json(&r).await?;
            let access = json["access_token"]
                .as_str()
                .ok_or_else(|| JsValue::from_str("no access_token"))?
                .to_string();
            store.borrow_mut().access_token = Some(access);
            Ok(true)
        }
        Ok(_) => {
            // Сервер явно отказал — refresh-токен невалиден, чистим всё.
            store.borrow_mut().clear();
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// Logout: просто забываем токены на клиенте.
pub fn logout(store: &TokenStoreHandle) {
    store.borrow_mut().clear();
}