-- Схема БД для production WebAuthn.

-- Пользователи.
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT PRIMARY KEY,       -- UUID v4
    username TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_login TEXT
);

-- Passkey credentials.
--
-- `passkey_json` — сериализованный `webauthn_rs::prelude::Passkey`
-- в формате JSON. Содержит публичный ключ, credential_id, sign_count,
-- флаги BE/BS. webauthn-rs сам управляет его форматом.
CREATE TABLE IF NOT EXISTS passkeys (
    credential_id TEXT PRIMARY KEY,  -- base64url от rawId
    user_id TEXT NOT NULL,
    passkey_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_passkeys_user ON passkeys(user_id);

-- In-flight церемонии (registration + authentication).
--
-- `state_json` — сериализованное состояние `PasskeyRegistration`
-- или `PasskeyAuthentication`. Живёт короткое время (до 5 минут).
-- Удаляется после успешного finish'а. Если что-то пошло не так —
-- периодический vacuum удалит старые записи.
CREATE TABLE IF NOT EXISTS ceremonies (
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL,             -- 'register' | 'authenticate'
    state_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (session_id, kind)
);