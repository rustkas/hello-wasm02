//! WebAuthn Server — companion к главе 41.
//!
//! Минимальный сервер, реализующий серверную часть WebAuthn:
//!   - генерирует challenge (32 случайных байта);
//!   - принимает attestation при регистрации;
//!   - принимает assertion при входе;
//!   - проверяет rpIdHash, challenge, origin, signCount;
//!   - хранит пользователей в памяти и возвращает профиль.
//!
//! ПРАВИЛО: этот проект standalone — отдельный Cargo.toml, свой main().
//! Он не входит в workspace hello-wasm02.


mod handlers;
mod state;
mod types;
mod webauthn;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::state::AppState;

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState::new());

    // API-роуты.
    let api = Router::new()
        .route("/api/register/begin", post(handlers::register_begin))
        .route("/api/register/finish", post(handlers::register_finish))
        .route("/api/login/begin", post(handlers::login_begin))
        .route("/api/login/finish", post(handlers::login_finish))
        .route("/api/users", get(handlers::list_users))
        .with_state(state);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // static/ отдаётся как fallback — API имеет приоритет.
    let app = Router::new()
        .merge(api)
        .fallback_service(ServeDir::new("static"))
        .layer(cors);

    let addr = "127.0.0.1:3030";
    let listener = TcpListener::bind(addr).await
        .expect("не удалось занять порт 3030");

    println!("=======================================================");
    println!("  WebAuthn Server готов");
    println!("=======================================================");
    println!();
    println!("  Откройте: http://localhost:3030");
    println!();
    println!("  ВАЖНО: именно localhost, а не 127.0.0.1.");
    println!("  WebAuthn запрещает IP-адреса в rp.id — браузер отвергнет");
    println!("  регистрацию при открытии по 127.0.0.1:3030.");
    println!();
    println!("  Эндпоинты:");
    println!("    POST /api/register/begin");
    println!("    POST /api/register/finish");
    println!("    POST /api/login/begin");
    println!("    POST /api/login/finish");
    println!("    GET  /api/users");
    println!();

    axum::serve(listener, app).await.expect("server failed");
}