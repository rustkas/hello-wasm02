//! Production-grade WebAuthn Server — companion 2 к главе 41.

mod attestation;
mod config;
mod db;
mod error;
mod handlers;
mod state;

use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::{
    routing::post,
    Router,
};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::{MemoryStore, SessionManagerLayer};
use url::Url;
use webauthn_rs::prelude::*;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Загружаем .env, если файл есть. Игнорируем ошибку —
    // в продакшене .env обычно нет.
    let _ = dotenvy::dotenv();
    // === Config из переменных окружения ===
    let config = Config::from_env();
    if let Err(e) = config.validate() {
        eprintln!("Ошибка конфигурации: {e}");
        std::process::exit(1);
    }

    println!("Конфигурация:");
    println!("  rp_id       = {}", config.rp_id);
    println!("  rp_origin   = {}", config.rp_origin);
    println!("  rp_name     = {}", config.rp_name);
    println!("  db_url      = {}", config.db_url);
    println!("  listen_addr = {}", config.listen_addr);
    println!("  cookie_secure = {}", config.cookie_secure);
    println!();

    // === SQLite ===
    let pool = match db::init_pool(&config.db_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("DB init failed: {e}");
            std::process::exit(1);
        }
    };

    // === Webauthn ===
    let rp_origin = Url::parse(&config.rp_origin).expect("invalid rp_origin URL");
    let mut builder =
        WebauthnBuilder::new(&config.rp_id, &rp_origin).expect("invalid webauthn config");
    builder = builder.rp_name(&config.rp_name);
    let webauthn = builder.build().expect("invalid webauthn");

    let app_state = Arc::new(AppState { pool, webauthn });

    // === Sessions ===
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(config.cookie_secure);

    // === CORS ===
    // Динамический origin из конфига. `HeaderValue::from_str`
    // валидирует строку как HTTP-header-value и вернёт Err,
    // если она содержит недопустимые символы.
    let allowed_origin = HeaderValue::from_str(&config.rp_origin)
        .expect("RP_ORIGIN содержит недопустимые символы для HTTP-заголовка");

    let cors = CorsLayer::new()
        .allow_origin(allowed_origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::COOKIE, header::AUTHORIZATION])
        .allow_credentials(true);

    // === Router ===
    let api = Router::new()
        .route("/api/register/begin", post(handlers::register_begin))
        .route("/api/register/finish", post(handlers::register_finish))
        .route("/api/login/begin", post(handlers::login_begin))
        .route("/api/login/finish", post(handlers::login_finish))
        .with_state(app_state);

    // Отключаем кеширование для всей статики — иначе браузер
    // может отдавать старый index.html после правок, и читатель
    // увидит ошибку в коде, который уже исправлен на диске.
    let no_cache = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );

    let app = Router::new()
        .merge(api)
        .fallback_service(ServeDir::new("static"))
        .layer(cors)
        .layer(no_cache)
        .layer(session_layer);

    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .expect("не удалось занять порт");

    println!("=========================================================");
    println!("  Production WebAuthn Server готов");
    println!("=========================================================");
    println!();
    println!("  Откройте: {}", config.rp_origin);
    println!();

    axum::serve(listener, app).await.expect("server failed");
}
