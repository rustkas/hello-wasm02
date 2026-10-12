//! Единый тип ошибок для всех слоёв приложения.
//!
//! Оборачивает ошибки sqlx, webauthn-rs, сессий и CBOR-парсинга
//! в один `AppError`, который реализует `IntoResponse`.
//! Это позволяет обработчикам использовать `?` без ручного маппинга.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    /// Ошибка уровня webauthn-rs (attestation/assertion не прошли проверку).
    Webauthn(String),
    /// Ошибка БД.
    Db(String),
    /// Сессия отсутствует или просрочена.
    NoSession,
    /// Пользователь не найден.
    UserNotFound,
    /// Прочее.
    Other(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return match self {
            AppError::Webauthn(m) => write!(f, "webauthn: {m}"),
            AppError::Db(m) => write!(f, "db: {m}"),
            AppError::NoSession => write!(f, "no active session"),
            AppError::UserNotFound => write!(f, "user not found"),
            AppError::Other(m) => write!(f, "{m}"),
        };
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::Webauthn(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Db(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            AppError::NoSession => (StatusCode::UNAUTHORIZED, "no active session".to_string()),
            AppError::UserNotFound => (StatusCode::NOT_FOUND, "user not found".to_string()),
            AppError::Other(m) => (StatusCode::BAD_REQUEST, m.clone()),
        };
        return (status, Json(json!({ "error": msg }))).into_response();
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        return AppError::Db(e.to_string());
    }
}

impl From<webauthn_rs::prelude::WebauthnError> for AppError {
    fn from(e: webauthn_rs::prelude::WebauthnError) -> Self {
        return AppError::Webauthn(format!("{e:?}"));
    }
}