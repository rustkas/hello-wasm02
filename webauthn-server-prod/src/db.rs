//! SQLite-хранилище через `sqlx`.
//!
//! Все функции принимают `&SqlitePool` и возвращают `Result<_, sqlx::Error>`.
//! Сериализация `Passkey` и состояний церемоний — через `serde_json`,
//! потому что webauthn-rs реализует `Serialize`/`Deserialize` для этих типов.

use chrono::Utc;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use webauthn_rs::prelude::{Passkey, PasskeyAuthentication, PasskeyRegistration};

/// Открывает пул соединений и создаёт таблицы, если их нет.
pub async fn init_pool(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;

    // Миграции — читаем файл и выполняем через `sqlx::query`.
    // В продакшене — sqlx::migrate! макрос, но для демо
    // inline-схема понятнее.
    let schema = include_str!("../migrations/001_init.sql");
    for stmt in schema.split(";") {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        sqlx::query(stmt).execute(&pool).await?;
    }

    return Ok(pool);
}

// ============================================================
//                    USERS
// ============================================================

pub struct UserRow {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub created_at: String,
    pub last_login: Option<String>,
}

/// Ищет пользователя по имени.
pub async fn find_user_by_name(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT user_id, username, display_name, created_at, last_login \
         FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    return Ok(row.map(|r| UserRow {
        user_id: r.0,
        username: r.1,
        display_name: r.2,
        created_at: r.3,
        last_login: r.4,
    }));
}

/// Ищет пользователя по user_id (для сессионного restore).
pub async fn find_user_by_id(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT user_id, username, display_name, created_at, last_login \
         FROM users WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    return Ok(row.map(|r| UserRow {
        user_id: r.0,
        username: r.1,
        display_name: r.2,
        created_at: r.3,
        last_login: r.4,
    }));
}

/// Создаёт пользователя. Если уже существует — возвращает существующего.
///
/// `INSERT OR IGNORE` + повторный `SELECT` — идемпотентно.
pub async fn upsert_user(
    pool: &SqlitePool,
    user_id: &str,
    username: &str,
    display_name: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO users (user_id, username, display_name, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(username)
    .bind(display_name)
    .bind(&now)
    .execute(pool)
    .await?;
    return Ok(());
}

pub async fn update_last_login(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET last_login = ? WHERE user_id = ?")
        .bind(&now)
        .bind(user_id)
        .execute(pool)
        .await?;
    return Ok(());
}

// ============================================================
//                    PASSKEYS
// ============================================================

/// Сохраняет `Passkey` (сериализованный в JSON) в БД.
pub async fn save_passkey(
    pool: &SqlitePool,
    user_id: &str,
    passkey: &Passkey,
) -> Result<(), sqlx::Error> {
    let json = serde_json::to_string(passkey)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let cred_id = base64_url_encode(passkey.cred_id().as_ref());
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO passkeys (credential_id, user_id, passkey_json, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&cred_id)
    .bind(user_id)
    .bind(&json)
    .bind(&now)
    .execute(pool)
    .await?;
    return Ok(());
}

/// Возвращает все passkeys пользователя.
pub async fn get_passkeys_for_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<Passkey>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT passkey_json FROM passkeys WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for (json,) in rows {
        let pk: Passkey = serde_json::from_str(&json)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        out.push(pk);
    }
    return Ok(out);
}

/// Обновляет `Passkey` после успешной аутентификации.
///
/// `PasskeyAuthenticationResult` содержит новый `sign_count` и
/// `backup_state`. webauthn-rs даёт метод `update_credential`,
/// который применяет их к сохранённому `Passkey`.
pub async fn update_passkey(
    pool: &SqlitePool,
    passkey: &Passkey,
) -> Result<(), sqlx::Error> {
    let json = serde_json::to_string(passkey)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let cred_id = base64_url_encode(passkey.cred_id().as_ref());
    sqlx::query("UPDATE passkeys SET passkey_json = ? WHERE credential_id = ?")
        .bind(&json)
        .bind(&cred_id)
        .execute(pool)
        .await?;
    return Ok(());
}

// ============================================================
//                    CEREMONIES
// ============================================================

pub async fn save_registration_state(
    pool: &SqlitePool,
    session_id: &str,
    state: &PasskeyRegistration,
) -> Result<(), sqlx::Error> {
    let json = serde_json::to_string(state)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO ceremonies (session_id, kind, state_json, created_at) \
         VALUES (?, 'register', ?, ?)",
    )
    .bind(session_id)
    .bind(&json)
    .bind(&now)
    .execute(pool)
    .await?;
    return Ok(());
}

pub async fn save_auth_state(
    pool: &SqlitePool,
    session_id: &str,
    state: &PasskeyAuthentication,
) -> Result<(), sqlx::Error> {
    let json = serde_json::to_string(state)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO ceremonies (session_id, kind, state_json, created_at) \
         VALUES (?, 'authenticate', ?, ?)",
    )
    .bind(session_id)
    .bind(&json)
    .bind(&now)
    .execute(pool)
    .await?;
    return Ok(());
}

pub async fn take_registration_state(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<PasskeyRegistration>, sqlx::Error> {
    return take_state_generic(pool, session_id, "register").await;
}

pub async fn take_auth_state(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<PasskeyAuthentication>, sqlx::Error> {
    return take_state_generic(pool, session_id, "authenticate").await;
}

async fn take_state_generic<T: serde::de::DeserializeOwned>(
    pool: &SqlitePool,
    session_id: &str,
    kind: &str,
) -> Result<Option<T>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT state_json FROM ceremonies WHERE session_id = ? AND kind = ?",
    )
    .bind(session_id)
    .bind(kind)
    .fetch_optional(pool)
    .await?;

    // Удаляем запись сразу — церемония одноразовая.
    sqlx::query("DELETE FROM ceremonies WHERE session_id = ? AND kind = ?")
        .bind(session_id)
        .bind(kind)
        .execute(pool)
        .await?;

    match row {
        Some((json,)) => {
            let state: T = serde_json::from_str(&json)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            return Ok(Some(state));
        }
        None => return Ok(None),
    }
}

// ============================================================
//                    HELPERS
// ============================================================

fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    return URL_SAFE_NO_PAD.encode(bytes);
}