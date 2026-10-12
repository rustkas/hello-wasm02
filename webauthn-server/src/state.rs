//! In-memory хранилище пользователей и pending challenges.
//!
//! В продакшене:
//!   - users — таблица в PostgreSQL/SQLite (public_key, credential_id,
//!     sign_count, created_at, ...);
//!   - challenges — Redis с TTL или колонка в сессии пользователя.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Тип challenge — при регистрации и при входе разные проверки.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChallengeKind {
    Register,
    Login,
}

pub struct PendingChallenge {
    pub challenge: Vec<u8>,
    pub kind: ChallengeKind,
}

/// Запись о пользователе на сервере.
///
/// `credential_id` и `sign_count` — минимум, который нужен для
/// последующей проверки assertion. В реальном приложении здесь
/// ещё публичный ключ (CBOR-структура), AAGUID аутентификатора, флаги.
#[derive(Clone)]
pub struct StoredUser {
    pub username: String,
    pub display_name: String,
    pub credential_id: Vec<u8>,
    pub created_at: u64,
    pub last_login: u64,
    pub login_count: u32,
    pub sign_count: u32,
    pub notes: String,
}

pub struct AppState {
    pub users: Mutex<HashMap<String, StoredUser>>,
    pub challenges: Mutex<HashMap<String, PendingChallenge>>,
    pub rp_id: String,
    pub rp_name: String,
    pub origin: String,
}

impl AppState {
    pub fn new() -> Self {
        return Self {
            users: Mutex::new(HashMap::new()),
            challenges: Mutex::new(HashMap::new()),
            // Жёстко зашито для демо. В продакшене — из конфига.
            rp_id: "localhost".to_string(),
            rp_name: "Глава 41 — WebAuthn сервер".to_string(),
            origin: "http://localhost:3030".to_string(),
        };
    }

    pub fn store_user(&self, user: StoredUser) {
        self.users
            .lock()
            .unwrap()
            .insert(user.username.clone(), user);
    }

    pub fn get_user(&self, username: &str) -> Option<StoredUser> {
        return self.users.lock().unwrap().get(username).cloned();
    }

    pub fn set_challenge(&self, username: &str, challenge: Vec<u8>, kind: ChallengeKind) {
        self.challenges.lock().unwrap().insert(
            username.to_string(),
            PendingChallenge { challenge, kind },
        );
    }

    /// Забирает challenge и удаляет его из хранилища.
    ///
    /// Удаление — обязательное: challenge одноразовый.
    /// Иначе возможна атака повторного воспроизведения.
    pub fn take_challenge(&self, username: &str) -> Option<PendingChallenge> {
        return self.challenges.lock().unwrap().remove(username);
    }
}

pub fn now_secs() -> u64 {
    return SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
}