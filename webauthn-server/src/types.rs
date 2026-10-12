//! DTO для API: запросы и ответы в JSON.
//!
//! Поля, которые в JSON называются `type` (зарезервированное слово
//! Rust), помечены `#[serde(rename = "type")]`.

use serde::{Deserialize, Serialize};

// ============================================================
//                    REGISTRATION
// ============================================================

#[derive(Deserialize)]
pub struct RegisterBeginReq {
    pub username: String,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct RegisterBeginResp {
    pub challenge: String,
    pub rp: RpEntity,
    pub user: UserEntity,
    pub pub_key_cred_params: Vec<CredParam>,
    pub timeout: u32,
    pub attestation: String,
    pub authenticator_selection: AuthenticatorSelection,
}

#[derive(Serialize)]
pub struct RpEntity {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct UserEntity {
    pub id: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct CredParam {
    #[serde(rename = "type")]
    pub type_: String,
    pub alg: i32,
}

#[derive(Serialize)]
pub struct AuthenticatorSelection {
    pub authenticator_attachment: String,
    pub user_verification: String,
    pub resident_key: String,
}

#[derive(Deserialize)]
pub struct RegisterFinishReq {
    pub username: String,
    pub credential_id: String,       // base64url
    pub client_data_json: String,    // base64url
    pub attestation_object: String,  // base64url
}

#[derive(Serialize)]
pub struct RegisterFinishResp {
    pub ok: bool,
    pub user: UserProfile,
}

// ============================================================
//                    AUTHENTICATION
// ============================================================

#[derive(Deserialize)]
pub struct LoginBeginReq {
    pub username: String,
}

#[derive(Serialize)]
pub struct LoginBeginResp {
    pub challenge: String,
    pub rp_id: String,
    pub timeout: u32,
    pub user_verification: String,
    pub allow_credentials: Vec<AllowCredential>,
}

#[derive(Serialize)]
pub struct AllowCredential {
    #[serde(rename = "type")]
    pub type_: String,
    pub id: String,       // base64url
}

#[derive(Deserialize)]
pub struct LoginFinishReq {
    pub username: String,
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct LoginFinishResp {
    pub ok: bool,
    pub user: UserProfile,
    pub checks: Checks,
}

/// Результаты проверок на сервере — для отображения в UI.
#[derive(Serialize)]
pub struct Checks {
    // clientDataJSON — проверки
    pub challenge_matches: bool,
    pub origin_matches: bool,
    pub not_cross_origin: bool,
    // authenticatorData — проверки
    pub rp_hash_ok: bool,
    pub up_flag_ok: bool,
    pub sign_count_ok: bool,
    pub sign_count_prev: u32,
    pub sign_count_new: u32,
    // Прочее
    pub credential_id_matches: bool,
    pub signature_size_ok: bool,
}
// ============================================================
//                    PROFILE
// ============================================================

/// Профиль пользователя — то, что сервер возвращает клиенту.
///
/// Именно эти данные показываются пользователю после входа:
/// сервер «вспомнил» его и вернул сохранённое.
#[derive(Serialize, Clone)]
pub struct UserProfile {
    pub username: String,
    pub display_name: String,
    pub created_at: String,
    pub last_login: String,
    pub login_count: u32,
    pub sign_count: u32,
    pub credential_id: String,
    pub notes: String,
}