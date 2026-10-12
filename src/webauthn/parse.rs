//! Разбор бинарных структур WebAuthn.
//!
//! WebAuthn возвращает две структуры, которые нужно разобрать,
//! чтобы понять, что реально произошло:
//!
//! - `authenticatorData` — минимум 37 байт:
//!   `rpIdHash(32) || flags(1) || signCount(4 BE) || [extensions]`.
//! - `clientDataJSON` — JSON с `type`, `challenge`, `origin`,
//!   `crossOrigin`. Именно здесь браузер подтверждает origin —
//!   JavaScript не может подделать это поле.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

use crate::format_str::format_str;
use serde::Deserialize;

// ============================================================
//                    AUTHENTICATOR DATA
// ============================================================

/// Байт флагов из `authenticatorData[32]`.
///
/// Расшифровка — по [W3C WebAuthn Level 2, §6.1](https://www.w3.org/TR/webauthn-2/#flags).
#[derive(Debug, Clone, Copy)]
pub struct AuthFlags(pub u8);

impl AuthFlags {
    /// User Present: пользователь коснулся аутентификатора.
    pub fn user_present(self) -> bool { return (self.0 & 0x01) != 0; }
    /// User Verified: подтверждена личность (биометрия или PIN).
    pub fn user_verified(self) -> bool { return (self.0 & 0x04) != 0; }
    /// Backup Eligible: credential может синхронизироваться (облачный passkey).
    pub fn backup_eligible(self) -> bool { return (self.0 & 0x08) != 0; }
    /// Backup State: credential сейчас синхронизирован.
    pub fn backup_state(self) -> bool { return (self.0 & 0x10) != 0; }
    /// Attested Credential Data: включены данные о новом ключе
    /// (только при регистрации).
    pub fn attested_credential(self) -> bool { return (self.0 & 0x40) != 0; }
    /// Extension Data: включены расширения.
    pub fn extension_data(self) -> bool { return (self.0 & 0x80) != 0; }
}

/// Разобранный `authenticatorData`.
#[derive(Debug, Clone, Copy)]
pub struct AuthData {
    pub rp_id_hash: [u8; 32],
    pub flags: AuthFlags,
    pub sign_count: u32,
}

/// Разбирает `authenticatorData`.
///
/// Минимальный валидный размер — 37 байт. При регистрации
/// структура длиннее (attestedCredentialData + extensions), но
/// для проверки флагов и счётчика нам достаточно первых 37.
pub fn parse_authenticator_data(bytes: &[u8]) -> Result<AuthData, String> {
    if bytes.len() < 37 {
        return Err(format_str(format_args!(
            "authenticatorData too short: {} < 37",
            bytes.len(),
        )));
    }
    let mut rp_id_hash = [0u8; 32];
    rp_id_hash.copy_from_slice(&bytes[0..32]);
    let flags = AuthFlags(bytes[32]);
    let sign_count = u32::from_be_bytes([
        bytes[33], bytes[34], bytes[35], bytes[36],
    ]);
    return Ok(AuthData { rp_id_hash, flags, sign_count });
}

// ============================================================
//                    CLIENT DATA JSON
// ============================================================

/// Разобранный `clientDataJSON`.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientData {
    /// `"webauthn.create"` или `"webauthn.get"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// base64url от challenge (без padding).
    pub challenge: String,
    /// Точный origin, на котором произошла операция.
    pub origin: String,
    #[serde(default, rename = "crossOrigin")]
    pub cross_origin: bool,
}

/// Разбирает `clientDataJSON`.
pub fn parse_client_data(bytes: &[u8]) -> Result<ClientData, String> {
    let s = std::str::from_utf8(bytes)
        .map_err(|e| format_str(format_args!("clientDataJSON utf8: {e}")))?;
    return serde_json::from_str(s)
        .map_err(|e| format_str(format_args!("clientDataJSON json: {e}")));
}