//! Эндпоинты WebAuthn-церемоний.
//!
//! Все криптографические проверки выполняет webauthn-rs.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::attestation;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

// ============================================================
//                    REQUEST / RESPONSE
// ============================================================

#[derive(Deserialize)]
pub struct RegisterBeginReq {
    pub username: String,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct RegisterBeginResp {
    /// `CreationChallengeResponse` — сериализуется в JSON напрямую,
    /// клиент передаёт его в `navigator.credentials.create({ publicKey: ... })`.
    ///
    /// В webauthn-rs 0.5 этот тип заменил `PublicKeyCredentialCreationOptions`.
    pub options: CreationChallengeResponse,
    pub user_id: String,
}

#[derive(Deserialize)]
pub struct RegisterFinishReq {
    pub credential: RegisterPublicKeyCredential,
}

#[derive(Serialize)]
pub struct RegisterFinishResp {
    pub ok: bool,
    pub user_id: String,
    pub username: String,
    pub attestation: AttestationInfo,
    pub cose_key: CoseKeyInfo,
}

#[derive(Serialize)]
pub struct AttestationInfo {
    pub fmt: String,
    pub auth_data_len: usize,
    pub aaguid: String,
    pub credential_id_len: usize,
    pub cose_key_len: usize,
    /// Краткое описание attestation statement: для fmt="none"
    /// это пустая map, для fmt="packed" — {alg, sig}.
    pub att_stmt: String,
}

#[derive(Serialize)]
pub struct CoseKeyInfo {
    pub kty: i64,
    pub alg: i64,
    pub crv: Option<i64>,
    pub x_len: usize,
    pub y_len: usize,
}

#[derive(Deserialize)]
pub struct LoginBeginReq {
    pub username: String,
}

#[derive(Serialize)]
pub struct LoginBeginResp {
    /// `RequestChallengeResponse` — сериализуется в JSON,
    /// клиент передаёт в `navigator.credentials.get({ publicKey: ... })`.
    ///
    /// Заменил `PublicKeyCredentialRequestOptions` в webauthn-rs 0.5.
    pub options: RequestChallengeResponse,
    pub user_id: String,
}

#[derive(Deserialize)]
pub struct LoginFinishReq {
    pub credential: PublicKeyCredential,
}

#[derive(Serialize)]
pub struct LoginFinishResp {
    pub ok: bool,
    pub user: UserProfile,
}

#[derive(Serialize)]
pub struct UserProfile {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub created_at: String,
    pub last_login: Option<String>,
    pub passkey_count: usize,
    pub credential_id: String,
    pub sign_count: u32,
}

// ============================================================
//                    SESSION HELPERS
// ============================================================

async fn session_id(session: &Session) -> Result<String, AppError> {
    let id: Option<String> = session.get("sid").await
        .map_err(|e| AppError::Other(format!("session get: {e}")))?;
    return id.ok_or(AppError::NoSession);
}

async fn set_session_id(session: &Session) -> Result<String, AppError> {
    let id = Uuid::new_v4().to_string();
    session.insert("sid", &id).await
        .map_err(|e| AppError::Other(format!("session insert: {e}")))?;
    return Ok(id);
}

// ============================================================
//                    REGISTRATION
// ============================================================

pub async fn register_begin(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<RegisterBeginReq>,
) -> Result<Json<RegisterBeginResp>, AppError> {
    let existing = db::find_user_by_name(&state.pool, &req.username).await?;
    let (user_id, display_name) = match existing {
        Some(u) => (u.user_id, u.display_name),
        None => {
            let new_id = Uuid::new_v4();
            db::upsert_user(
                &state.pool,
                &new_id.to_string(),
                &req.username,
                &req.display_name,
            ).await?;
            (new_id.to_string(), req.display_name.clone())
        }
    };

    let existing_keys = db::get_passkeys_for_user(&state.pool, &user_id).await?;
    let exclude_credentials: Option<Vec<CredentialID>> = if existing_keys.is_empty() {
        None
    } else {
        Some(existing_keys.iter().map(|p| p.cred_id().clone()).collect())
    };

    let user_uuid = Uuid::parse_str(&user_id)
        .map_err(|e| AppError::Other(format!("invalid uuid: {e}")))?;

    // webauthn-rs: возвращает (CreationChallengeResponse, PasskeyRegistration).
    let (options, reg_state) = state.webauthn.start_passkey_registration(
        user_uuid,
        &req.username,
        &display_name,
        exclude_credentials,
    )?;

    let sid = match session_id(&session).await {
        Ok(s) => s,
        Err(_) => set_session_id(&session).await?,
    };
    session.insert("reg_user_id", &user_id).await
        .map_err(|e| AppError::Other(format!("session insert: {e}")))?;
    db::save_registration_state(&state.pool, &sid, &reg_state).await?;

    return Ok(Json(RegisterBeginResp { options, user_id }));
}

pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<RegisterFinishReq>,
) -> Result<Json<RegisterFinishResp>, AppError> {
    let sid = session_id(&session).await?;
    let user_id: String = session.get("reg_user_id").await
        .map_err(|e| AppError::Other(format!("session get: {e}")))?
        .ok_or(AppError::NoSession)?;

    let reg_state = db::take_registration_state(&state.pool, &sid).await?
        .ok_or_else(|| AppError::Other("registration state not found".to_string()))?;

    // === Демонстрация: ручной парсинг attestationObject через ciborium. ===
    let att_bytes = req.credential.response.attestation_object.as_ref();
    let att_info = match attestation::parse_attestation_object(att_bytes) {
        Ok(att) => {
            match attestation::extract_attested_credential_data(&att.auth_data) {
                Ok(acd) => {
                    let cose_info = attestation::summarize_cose_key(acd.cose_public_key);
                    let attestation_info = AttestationInfo {
    fmt: att.fmt.clone(),
    auth_data_len: att.auth_data.len(),
    aaguid: hex_encode(&acd.aaguid),
    credential_id_len: acd.credential_id.len(),
    cose_key_len: acd.cose_public_key.len(),
    att_stmt: attestation::describe_att_stmt(&att.att_stmt),
};
                    let cose_key_info = cose_info.map(|c| CoseKeyInfo {
                        kty: c.kty,
                        alg: c.alg,
                        crv: c.crv,
                        x_len: c.coord_x_len,
                        y_len: c.coord_y_len,
                    }).unwrap_or(CoseKeyInfo {
                        kty: 0, alg: 0, crv: None, x_len: 0, y_len: 0,
                    });
                    (attestation_info, cose_key_info)
                }
                Err(e) => return Err(AppError::Other(format!("extract ACD: {e}"))),
            }
        }
        Err(e) => return Err(AppError::Other(format!("parse attestationObject: {e}"))),
    };

    // === Настоящая проверка через webauthn-rs. ===
    let passkey = state.webauthn.finish_passkey_registration(
        &req.credential,
        &reg_state,
    )?;

    db::save_passkey(&state.pool, &user_id, &passkey).await?;

    let user = db::find_user_by_id(&state.pool, &user_id).await?
        .ok_or(AppError::UserNotFound)?;

    return Ok(Json(RegisterFinishResp {
        ok: true,
        user_id,
        username: user.username,
        attestation: att_info.0,
        cose_key: att_info.1,
    }));
}

// ============================================================
//                    AUTHENTICATION
// ============================================================

pub async fn login_begin(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<LoginBeginReq>,
) -> Result<Json<LoginBeginResp>, AppError> {
    let user = db::find_user_by_name(&state.pool, &req.username).await?
        .ok_or(AppError::UserNotFound)?;

    let passkeys = db::get_passkeys_for_user(&state.pool, &user.user_id).await?;
    if passkeys.is_empty() {
        return Err(AppError::Other("user has no passkeys".to_string()));
    }

    let (options, auth_state) = state.webauthn.start_passkey_authentication(&passkeys)?;

    let sid = match session_id(&session).await {
        Ok(s) => s,
        Err(_) => set_session_id(&session).await?,
    };
    session.insert("auth_user_id", &user.user_id).await
        .map_err(|e| AppError::Other(format!("session insert: {e}")))?;
    db::save_auth_state(&state.pool, &sid, &auth_state).await?;

    return Ok(Json(LoginBeginResp {
        options,
        user_id: user.user_id,
    }));
}

pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<LoginFinishReq>,
) -> Result<Json<LoginFinishResp>, AppError> {
    let sid = session_id(&session).await?;
    let user_id: String = session.get("auth_user_id").await
        .map_err(|e| AppError::Other(format!("session get: {e}")))?
        .ok_or(AppError::NoSession)?;

    let auth_state = db::take_auth_state(&state.pool, &sid).await?
        .ok_or_else(|| AppError::Other("auth state not found".to_string()))?;

    // === Настоящая криптографическая проверка подписи. ===
    let auth_result = state.webauthn.finish_passkey_authentication(
        &req.credential,
        &auth_state,
    )?;

    // sign_count берём из результата — метода `Passkey::counter()`
    // в webauthn-rs 0.5 нет. `AuthenticationResult::counter()` возвращает
    // актуальный счётчик.
    let new_sign_count = auth_result.counter();

    // Обновляем Passkey: sign_count и backup_state изменились.
    let mut passkeys = db::get_passkeys_for_user(&state.pool, &user_id).await?;
    for pk in passkeys.iter_mut() {
        pk.update_credential(&auth_result);
        db::update_passkey(&state.pool, pk).await?;
    }

    db::update_last_login(&state.pool, &user_id).await?;

    let user = db::find_user_by_id(&state.pool, &user_id).await?
        .ok_or(AppError::UserNotFound)?;
    let passkey_count = passkeys.len();
    let cred_id = match passkeys.first() {
        Some(p) => base64_url_encode(p.cred_id().as_ref()),
        None => "—".to_string(),
    };

    return Ok(Json(LoginFinishResp {
        ok: true,
        user: UserProfile {
            user_id: user.user_id,
            username: user.username,
            display_name: user.display_name,
            created_at: user.created_at,
            last_login: user.last_login,
            passkey_count,
            credential_id: cred_id,
            sign_count: new_sign_count,
        },
    }));
}

// ============================================================
//                    HELPERS
// ============================================================

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    return out;
}

fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    return URL_SAFE_NO_PAD.encode(bytes);
}