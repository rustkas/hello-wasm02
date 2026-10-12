//! Конфигурация приложения.
//!
//! Читается из переменных окружения с разумными дефолтами
//! для локальной разработки. В продакшене все значения задаются
//! явно и не полагаются на дефолты.

use std::env;

/// Полная конфигурация сервера.
///
/// В продакшене заполняется из секретов (Vault, K8s secrets,
/// systemd Environment=) или из `.env` файла через dotenvy.
#[derive(Debug, Clone)]
pub struct Config {
    /// `rp_id` — домен без порта. WebAuthn запрещает IP-адреса.
    /// Пример: `localhost` (dev), `example.com` (prod).
    pub rp_id: String,

    /// `rp_origin` — полный origin, включая схему и порт.
    /// Пример: `http://localhost:3030` (dev), `https://example.com` (prod).
    pub rp_origin: String,

    /// `rp_name` — человекочитаемое название сервиса.
    /// Показывается в диалоге аутентификатора.
    pub rp_name: String,

    /// URL для подключения к SQLite. По умолчанию — файл рядом с бинарником.
    pub db_url: String,

    /// Адрес, на котором слушает сервер.
    pub listen_addr: String,

    /// Ставить ли флаг `Secure` на session-cookie.
    /// `false` для localhost по HTTP, `true` в продакшене.
    pub cookie_secure: bool,
}

impl Config {
    /// Загружает конфигурацию из переменных окружения.
    ///
    /// Дефолты рассчитаны на локальную разработку:
    /// `localhost`, HTTP, SQLite-файл `webauthn.db`, порт 3030.
    pub fn from_env() -> Self {
        return Self {
            rp_id: env::var("RP_ID")
                .unwrap_or_else(|_| "localhost".to_string()),
            rp_origin: env::var("RP_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:3030".to_string()),
            rp_name: env::var("RP_NAME")
                .unwrap_or_else(|_| "Chapter 41.02 — Production WebAuthn".to_string()),
            db_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:webauthn.db?mode=rwc".to_string()),
            listen_addr: env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:3030".to_string()),
            cookie_secure: env::var("COOKIE_SECURE")
                .map(|v| v == "1" || v == "true")
                .unwrap_or(false),
        };
    }

    /// Проверяет, что конфигурация согласована.
    ///
    /// Например, `rp_origin` должен содержать `rp_id` как hostname —
    /// иначе WebAuthn отвергнет церемонию с `SecurityError`.
    pub fn validate(&self) -> Result<(), String> {
        if self.rp_id.is_empty() {
            return Err("RP_ID пуст".to_string());
        }
        if self.rp_origin.is_empty() {
            return Err("RP_ORIGIN пуст".to_string());
        }
        // Грубая проверка: rp_id должен быть подстрокой rp_origin.
        if !self.rp_origin.contains(&self.rp_id) {
            return Err(format!(
                "RP_ORIGIN ({}) не содержит RP_ID ({})",
                self.rp_origin, self.rp_id,
            ));
        }
        return Ok(());
    }
}