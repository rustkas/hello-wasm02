mod auth;
mod handlers;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use axum::{
    extract::{DefaultBodyLimit, Extension},
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    dotenvy::dotenv().ok();

    // Разделяемое состояние для chunked upload.
    let chunk_store: handlers::ChunkStore = Arc::new(Mutex::new(HashMap::new()));

    // Запускаем фоновую TTL-очистку: раз в час удаляет загрузки,
    // к которым не обращались более 24 часов.
    tokio::spawn(handlers::cleanup_old_uploads(Duration::from_secs(
        24 * 3600,
    )));

    let app = Router::new()
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/refresh", post(auth::refresh))
        .route("/api/me", get(handlers::me))
        .route("/api/admin", get(handlers::admin_only))
        .route("/api/flaky", get(handlers::flaky))
        .route("/api/flaky/reset", get(handlers::flaky_reset))
        .route("/api/error500", get(handlers::always_500))
        .route("/api/slow", get(handlers::slow_default))
        .route("/api/slow/{secs}", get(handlers::slow))
        .route("/api/delay/{secs}", get(handlers::delay))
        .route("/api/concurrency/test", get(handlers::concurrency_test))
        .route("/api/concurrency/reset", get(handlers::concurrency_reset))
        .route("/api/concurrency/stats", get(handlers::concurrency_stats))
        .route("/api/image", get(handlers::image))
        .route("/api/binary", get(handlers::binary))
        .route("/api/stream/{count}", get(handlers::stream_chunks))
        .route("/api/stream/records/{count}", get(handlers::stream_records))
        // Снимаем дефолтный лимит 2 МБ ТОЛЬКО для upload.
        // Остальные эндпоинты остаются защищены лимитом.
        .route(
            "/api/upload",
            post(handlers::upload).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/api/upload/chunk",
            post(handlers::upload_chunk).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/api/upload/chunk/finalize",
            post(handlers::finalize_upload),
        )
        .route("/api/upload/chunk/list", get(handlers::list_uploads))
        .route(
            "/api/upload/chunk/{upload_id}",
            get(handlers::upload_status).delete(handlers::delete_upload),
        )
        .layer(Extension(chunk_store))
        .layer(CorsLayer::permissive());

    let addr = std::env::var("BIND_ADDRESS").unwrap_or("127.0.0.1:3000".into());
    println!("Auth server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
