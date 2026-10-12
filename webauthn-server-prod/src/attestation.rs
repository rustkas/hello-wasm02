//! Ручной парсинг `attestationObject` через `ciborium`.
//!
//! Зачем это нужно, если `webauthn-rs` уже всё делает?
//!
//! `webauthn-rs::finish_passkey_registration` возвращает
//! `Passkey` — но это «чёрный ящик»: библиотека проверила attestation,
//! извлекла публичный ключ и упаковала его в свою структуру.
//!
//! Для обучения полезно **увидеть**, что лежит в `attestationObject`
//! и как оттуда достаётся публичный ключ. Этот модуль делает ту же
//! работу вручную, чтобы протокол перестал быть «магией».
//!
//! В продакшене эту работу выполняет webauthn-rs. Модуль — для
//! демонстрации и отладки.

use ciborium::Value;
use serde::Deserialize;

/// Структура `attestationObject` в CBOR.
///
/// W3C WebAuthn Level 2 §6.5.4 определяет три обязательных поля:
///   - `fmt`: string — формат attestation ("none", "packed", "tpm", ...);
///   - `attStmt`: map — attestation statement (зависит от fmt);
///   - `authData`: bytes — authenticatorData (37+ байт, содержит
///     rpIdHash, flags, signCount, attestedCredentialData).
#[derive(Debug, Deserialize)]
pub struct AttestationObject {
    pub fmt: String,
    #[serde(rename = "attStmt")]
    pub att_stmt: Value,
    #[serde(rename = "authData")]
    pub auth_data: Vec<u8>,
}

/// Разбирает `attestationObject` из сырых байт.
pub fn parse_attestation_object(bytes: &[u8]) -> Result<AttestationObject, String> {
    let att: AttestationObject = ciborium::from_reader(bytes)
        .map_err(|e| format!("CBOR parse failed: {e}"))?;
    return Ok(att);
}

/// Извлекает из `authData` блок `attestedCredentialData`.
///
/// Формат (W3C §6.5.2):
///   ```
///   rpIdHash          : 32 байта
///   flags             : 1 байт
///   signCount         : 4 байта (BE)
///   attestedCredentialData : переменный
///     aaguid          : 16 байт
///     credentialIdLength : 2 байта (BE)
///     credentialId    : N байт
///     credentialPublicKey : CBOR-encoded COSE_Key
///   ```
///
/// Возвращает `(aaguid, credential_id, cose_public_key, remaining)`.
/// `remaining` — extensions, если есть (флаг ED).
pub struct AttestedCredentialData<'a> {
    pub aaguid: [u8; 16],
    pub credential_id: &'a [u8],
    pub cose_public_key: &'a [u8],
}

pub fn extract_attested_credential_data(
    auth_data: &[u8],
) -> Result<AttestedCredentialData<'_>, String> {
    if auth_data.len() < 37 {
        return Err(format!("authData too short: {} < 37", auth_data.len()));
    }

    let flags = auth_data[32];
    if (flags & 0x40) == 0 {
        return Err("AT flag not set — no attested credential data".to_string());
    }

    // После 37 байт идёт attestedCredentialData.
    let mut cursor = 37;

    // aaguid: 16 байт.
    if auth_data.len() < cursor + 16 {
        return Err("authData too short for aaguid".to_string());
    }
    let mut aaguid = [0u8; 16];
    aaguid.copy_from_slice(&auth_data[cursor..cursor + 16]);
    cursor += 16;

    // credentialIdLength: 2 байта BE.
    if auth_data.len() < cursor + 2 {
        return Err("authData too short for credentialIdLength".to_string());
    }
    let cred_id_len = u16::from_be_bytes([auth_data[cursor], auth_data[cursor + 1]]) as usize;
    cursor += 2;

    // credentialId: cred_id_len байт.
    if auth_data.len() < cursor + cred_id_len {
        return Err("authData too short for credentialId".to_string());
    }
    let credential_id = &auth_data[cursor..cursor + cred_id_len];
    cursor += cred_id_len;

    // credentialPublicKey: остаток до конца authData (CBOR-encoded COSE_Key).
    // В реальности там может быть ещё extensions, но для демо
    // берём весь остаток — этого хватает, чтобы посмотреть на CBOR.
    let cose_public_key = &auth_data[cursor..];

    return Ok(AttestedCredentialData {
        aaguid,
        credential_id,
        cose_public_key,
    });
}

/// Разбирает `credentialPublicKey` — CBOR-encoded COSE_Key.
///
/// Возвращает `(kty, alg, crv, x_len, y_len)` — человекочитаемое
/// описание ключа без криптографической интерпретации.
pub struct CoseKeySummary {
    pub kty: i64,
    pub alg: i64,
    pub crv: Option<i64>,
    pub coord_x_len: usize,
    pub coord_y_len: usize,
}

pub fn summarize_cose_key(cbor: &[u8]) -> Result<CoseKeySummary, String> {
    let value: Value = ciborium::from_reader(cbor)
        .map_err(|e| format!("COSE CBOR parse failed: {e}"))?;

    let map = value.as_map().ok_or("COSE key is not a map")?;

    let mut kty = 0i64;
    let mut alg = 0i64;
    let mut crv: Option<i64> = None;
    let mut x_len = 0;
    let mut y_len = 0;

    for (k, v) in map {
        // COSE-ключи используют integer-ключи.
        let key_id = k.as_integer().and_then(|i| i.try_into().ok());
        match key_id {
            Some(1) => kty = v.as_integer().and_then(|i| i.try_into().ok()).unwrap_or(0),
            Some(3) => alg = v.as_integer().and_then(|i| i.try_into().ok()).unwrap_or(0),
            Some(-1) => crv = v.as_integer().and_then(|i| i.try_into().ok()),
            Some(-2) => x_len = v.as_bytes().map(|b| b.len()).unwrap_or(0),
            Some(-3) => y_len = v.as_bytes().map(|b| b.len()).unwrap_or(0),
            _ => {}
        }
    }

    return Ok(CoseKeySummary { kty, alg, crv, coord_x_len: x_len, coord_y_len: y_len });
}

/// Описывает `attStmt` — attestation statement.
///
/// Для `fmt = "none"` это пустая map. Для `fmt = "packed"` —
/// `{alg, sig}`. Для `fmt = "tpm"` — набор полей по TPM-спеке.
///
/// Возвращает краткое человекочитаемое описание: список ключей
/// верхнего уровня и их типы.
pub fn describe_att_stmt(att_stmt: &Value) -> String {
    let map = match att_stmt.as_map() {
        Some(m) => m,
        None => return "не map".to_string(),
    };
    if map.is_empty() {
        return "пустая map (для fmt=\"none\" это норма)".to_string();
    }
    let mut parts: Vec<String> = Vec::with_capacity(map.len());
    for (k, v) in map {
        let key = match k.as_text() {
            Some(s) => s.to_string(),
            None => format!("{k:?}"),
        };
        let kind = match v {
            Value::Bytes(b) => format!("bytes({})", b.len()),
            Value::Integer(_) => "int".to_string(),
            Value::Text(s) => format!("text({})", s.len()),
            Value::Bool(_) => "bool".to_string(),
            Value::Array(a) => format!("array({})", a.len()),
            Value::Map(m) => format!("map({})", m.len()),
            Value::Null => "null".to_string(),
            Value::Float(_) => "float".to_string(),
            Value::Tag(_, _) => "tag".to_string(),
            // `Value` помечен `#[non_exhaustive]` — компилятор
            // требует wildcard-arm даже после перечисления всех
            // текущих вариантов.
            _ => "unknown".to_string(),
        };
        parts.push(format!("{key}: {kind}"));
    }
    return parts.join(", ");
}