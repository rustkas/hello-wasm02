use axum::{http::StatusCode, Json};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub typ: String,
    pub roles: Vec<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

fn secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or("dev-secret".into())
}

fn make_token(sub: &str, typ: &str, ttl_secs: i64, roles: Vec<String>) -> String {
    let claims = Claims {
        sub: sub.into(),
        exp: (Utc::now() + Duration::seconds(ttl_secs)).timestamp(),
        typ: typ.into(),
        roles,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret().as_bytes()),
    )
    .unwrap()
}

pub async fn login(Json(req): Json<LoginRequest>) -> Result<Json<Value>, StatusCode> {
    if req.password != "secret" {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let roles = if req.username == "admin" {
        vec!["user".into(), "admin".into()]
    } else {
        vec!["user".into()]
    };

    let access_ttl: i64 = std::env::var("ACCESS_TOKEN_TTL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let refresh_ttl: i64 = std::env::var("REFRESH_TOKEN_TTL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(86400);

    let access = make_token(&req.username, "access", access_ttl, roles.clone());
    let refresh = make_token(&req.username, "refresh", refresh_ttl, roles);

    Ok(Json(json!({
        "access_token": access,
        "refresh_token": refresh,
        "token_type": "Bearer",
        "expires_in": access_ttl
    })))
}

pub async fn refresh(Json(req): Json<RefreshRequest>) -> Result<Json<Value>, StatusCode> {
    use jsonwebtoken::{decode, DecodingKey, Validation};

    let token_data = decode::<Claims>(
        &req.refresh_token,
        &DecodingKey::from_secret(secret().as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if token_data.claims.typ != "refresh" {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let access_ttl: i64 = std::env::var("ACCESS_TOKEN_TTL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);

    let new_access = make_token(
        &token_data.claims.sub,
        "access",
        access_ttl,
        token_data.claims.roles,
    );

    Ok(Json(json!({
        "access_token": new_access,
        "token_type": "Bearer",
        "expires_in": access_ttl
    })))
}

/// Извлекает и валидирует access-токен из заголовка Authorization.
pub fn validate_bearer(headers: &axum::http::HeaderMap) -> Result<Claims, StatusCode> {
    use jsonwebtoken::{decode, DecodingKey, Validation};

    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret().as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if data.claims.typ != "access" {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(data.claims)
}