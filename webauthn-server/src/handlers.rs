//! HTTP-эндпоинты WebAuthn-церемоний.
//!
//! Все проверки, которые возвращаются в `checks`, соответствуют
//! шагам верификации из W3C WebAuthn Level 2 (§7.1 для регистрации,
//! §7.2 для аутентификации).

use std::sync::Arc;

use axum::{extract::State, Json};
use rand::RngCore;

use crate::state::{now_secs, AppState, ChallengeKind, StoredUser};
use crate::types::*;
use crate::webauthn::{
    from_base64url, parse_auth_data, parse_client_data, rp_id_hash, to_base64url,
};

// ============================================================
//                    REGISTRATION
// ============================================================

pub async fn register_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterBeginReq>,
) -> Json<RegisterBeginResp> {
    let mut challenge = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut challenge);
    state.set_challenge(
        &req.username,
        challenge.to_vec(),
        ChallengeKind::Register,
    );

    let mut user_id_buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut user_id_buf);

    return Json(RegisterBeginResp {
        challenge: to_base64url(&challenge),
        rp: RpEntity {
            id: state.rp_id.clone(),
            name: state.rp_name.clone(),
        },
        user: UserEntity {
            id: to_base64url(&user_id_buf),
            name: req.username.clone(),
            display_name: req.display_name.clone(),
        },
        pub_key_cred_params: vec![
            CredParam { type_: "public-key".into(), alg: -7 },
            CredParam { type_: "public-key".into(), alg: -257 },
        ],
        timeout: 60_000,
        attestation: "none".into(),
        authenticator_selection: AuthenticatorSelection {
            authenticator_attachment: "platform".into(),
            user_verification: "preferred".into(),
            resident_key: "preferred".into(),
        },
    });
}

pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterFinishReq>,
) -> Json<RegisterFinishResp> {
    let pending = match state.take_challenge(&req.username) {
        Some(p) => p,
        None => return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "нет активного challenge"),
        }),
    };
    if pending.kind != ChallengeKind::Register {
        return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "неверный тип challenge"),
        });
    }

    // === Проверяем attestationObject: должен быть непустым и
    //     корректным base64url. В реальном коде здесь был бы CBOR-парсинг. ===
    let attestation_bytes: Vec<u8> = match from_base64url(&req.attestation_object) {
        Ok(b) if !b.is_empty() => b,
        Ok(_) => return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "attestationObject пуст"),
        }),
        Err(e) => return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, &e),
        }),
    };

    let cd_bytes: Vec<u8> = match from_base64url(&req.client_data_json) {
        Ok(b) => b,
        Err(e) => return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, &e),
        }),
    };
    let cd = match parse_client_data(&cd_bytes) {
        Ok(c) => c,
        Err(e) => return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, &e),
        }),
    };

    if cd.kind != "webauthn.create" {
        return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "type != webauthn.create"),
        });
    }
    if cd.challenge != to_base64url(&pending.challenge) {
        return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "challenge не совпадает"),
        });
    }
    if cd.origin != state.origin {
        return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "origin не совпадает"),
        });
    }
    if cd.cross_origin {
        // Спека WebAuthn §7.1: crossOrigin MUST be false.
        return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, "crossOrigin = true запрещён"),
        });
    }

    let cred_id: Vec<u8> = match from_base64url(&req.credential_id) {
        Ok(b) => b,
        Err(e) => return Json(RegisterFinishResp {
            ok: false,
            user: error_profile(&req.username, &e),
        }),
    };

    let now = now_secs();
    let user = StoredUser {
        username: req.username.clone(),
        display_name: req.username.clone(),
        credential_id: cred_id,
        created_at: now,
        last_login: 0,
        login_count: 0,
        sign_count: 0,
        notes: format!(
            "Зарегистрирован. origin: {}; attestationObject: {} B",
            cd.origin,
            attestation_bytes.len(),
        ),
    };
    state.store_user(user.clone());

    return Json(RegisterFinishResp {
        ok: true,
        user: profile_from(&user),
    });
}

// ============================================================
//                    AUTHENTICATION
// ============================================================

pub async fn login_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginBeginReq>,
) -> Json<LoginBeginResp> {
    let user = state.get_user(&req.username);
    let allow = match &user {
        Some(u) => vec![AllowCredential {
            type_: "public-key".into(),
            id: to_base64url(&u.credential_id),
        }],
        None => vec![],
    };

    let mut challenge = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut challenge);
    state.set_challenge(
        &req.username,
        challenge.to_vec(),
        ChallengeKind::Login,
    );

    return Json(LoginBeginResp {
        challenge: to_base64url(&challenge),
        rp_id: state.rp_id.clone(),
        timeout: 60_000,
        user_verification: "preferred".into(),
        allow_credentials: allow,
    });
}

pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginFinishReq>,
) -> Json<LoginFinishResp> {
    let pending = match state.take_challenge(&req.username) {
        Some(p) => p,
        None => return err_login(&req.username, "нет активного challenge"),
    };

    // === clientDataJSON ===
    let cd_bytes: Vec<u8> = match from_base64url(&req.client_data_json) {
        Ok(b) => b,
        Err(e) => return err_login(&req.username, &e),
    };
    let cd = match parse_client_data(&cd_bytes) {
        Ok(c) => c,
        Err(e) => return err_login(&req.username, &e),
    };

    let challenge_matches = cd.challenge == to_base64url(&pending.challenge);
    let origin_matches = cd.origin == state.origin;
    let not_cross_origin = !cd.cross_origin;

    // === authenticatorData ===
    let auth_bytes: Vec<u8> = match from_base64url(&req.authenticator_data) {
        Ok(b) => b,
        Err(e) => return err_login(&req.username, &e),
    };
    let ad = match parse_auth_data(&auth_bytes) {
        Ok(a) => a,
        Err(e) => return err_login(&req.username, &e),
    };

    // Anti-phishing: rpIdHash == SHA-256(rp_id).
    let expected = rp_id_hash(&state.rp_id);
    let rp_hash_ok = ad.rp_id_hash == expected;

    // Спека WebAuthn §7.2, шаг 15: UP-бит обязан быть установлен.
    // Это подтверждение, что человек физически взаимодействовал
    // с аутентификатором (не «удалённый бот с чужим ключом»).
    let up_flag_ok = (ad.flags & 0x01) != 0;

    // === Подпись: проверяем размер (sanity-check). ===
    // В реальном коде — проверка подписи публичным ключом.
    // ES256 → 64 байта, RS256-2048 → 256 байт. Допускаем широкий
    // диапазон, чтобы отсеять только явный мусор.
    let sig_bytes: Vec<u8> = match from_base64url(&req.signature) {
        Ok(b) => b,
        Err(e) => return err_login(&req.username, &e),
    };
    let signature_size_ok = sig_bytes.len() >= 32 && sig_bytes.len() <= 1024;

    // === Пользователь ===
    let mut user = match state.get_user(&req.username) {
        Some(u) => u,
        None => return err_login(&req.username, "пользователь не найден"),
    };

    // Сверяем credential_id с сохранённым — защита от подмены.
    let cred_id_bytes: Vec<u8> = match from_base64url(&req.credential_id) {
        Ok(b) => b,
        Err(e) => return err_login(&req.username, &e),
    };
    let credential_id_matches = cred_id_bytes == user.credential_id;

    // Anti-cloning: signCount должен расти.
    let prev_sign = user.sign_count;
    let sign_count_ok = if prev_sign == 0 {
        true
    } else {
        ad.sign_count > prev_sign
    };

    let all_ok =
        challenge_matches
            && origin_matches
            && not_cross_origin
            && rp_hash_ok
            && up_flag_ok
            && signature_size_ok
            && credential_id_matches
            && sign_count_ok;

    if all_ok {
        user.last_login = now_secs();
        user.login_count += 1;
        user.sign_count = ad.sign_count;
        user.notes = format!(
            "Последний вход: origin {}; signature {} B",
            cd.origin,
            sig_bytes.len(),
        );
        state.store_user(user.clone());
    }

    return Json(LoginFinishResp {
        ok: all_ok,
        user: profile_from(&user),
        checks: Checks {
            challenge_matches,
            origin_matches,
            not_cross_origin,
            rp_hash_ok,
            up_flag_ok,
            sign_count_ok,
            sign_count_prev: prev_sign,
            sign_count_new: ad.sign_count,
            credential_id_matches,
            signature_size_ok,
        },
    });
}

// ============================================================
//                    LIST
// ============================================================

pub async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<UserProfile>> {
    let users = state.users.lock().unwrap();
    let mut list: Vec<UserProfile> = users.values().map(profile_from).collect();
    list.sort_by(|a, b| a.username.cmp(&b.username));
    return Json(list);
}

// ============================================================
//                    HELPERS
// ============================================================

fn profile_from(u: &StoredUser) -> UserProfile {
    return UserProfile {
        username: u.username.clone(),
        display_name: u.display_name.clone(),
        created_at: format_time(u.created_at),
        last_login: if u.last_login == 0 {
            "—".into()
        } else {
            format_time(u.last_login)
        },
        login_count: u.login_count,
        sign_count: u.sign_count,
        credential_id: to_base64url(&u.credential_id),
        notes: u.notes.clone(),
    };
}

fn error_profile(username: &str, msg: &str) -> UserProfile {
    return UserProfile {
        username: username.to_string(),
        display_name: "—".into(),
        created_at: "—".into(),
        last_login: "—".into(),
        login_count: 0,
        sign_count: 0,
        credential_id: "—".into(),
        notes: format!("Ошибка: {msg}"),
    };
}

fn err_login(username: &str, msg: &str) -> Json<LoginFinishResp> {
    return Json(LoginFinishResp {
        ok: false,
        user: error_profile(username, msg),
        checks: Checks {
            challenge_matches: false,
            origin_matches: false,
            not_cross_origin: false,
            rp_hash_ok: false,
            up_flag_ok: false,
            sign_count_ok: false,
            sign_count_prev: 0,
            sign_count_new: 0,
            credential_id_matches: false,
            signature_size_ok: false,
        },
    });
}

fn format_time(secs: u64) -> String {
    return format!("{secs}s (Unix epoch)");
}