//! Разделяемое состояние приложения.

use sqlx::sqlite::SqlitePool;
use webauthn_rs::prelude::Webauthn;

pub struct AppState {
    pub pool: SqlitePool,
    pub webauthn: Webauthn,
}