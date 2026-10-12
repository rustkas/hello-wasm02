use axum::{http::HeaderMap, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::auth::validate_bearer;

use std::sync::atomic::{AtomicUsize, Ordering};
use axum::extract::Path;

use axum::response::IntoResponse;
use axum::http::header;

use axum::body::Body;
use axum::response::Response;
use tokio_stream::wrappers::ReceiverStream;

use axum::extract::Multipart;
use sha2::{Digest, Sha256};

static CONCURRENT: AtomicUsize = AtomicUsize::new(0);
static MAX_CONCURRENT: AtomicUsize = AtomicUsize::new(0);

static FLAKY_COUNTER: AtomicUsize = AtomicUsize::new(0);

// ============================================================
// Защищённые эндпоинты
// ============================================================

pub async fn me(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let claims = validate_bearer(&headers)?;
    Ok(Json(json!({
        "username": claims.sub,
        "roles": claims.roles
    })))
}

pub async fn admin_only(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let claims = validate_bearer(&headers)?;

    if !claims.roles.iter().any(|r| r == "admin") {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(json!({
        "message": "Добро пожаловать, администратор",
        "username": claims.sub
    })))
}

// ============================================================
// Демонстрация retry
// ============================================================

/// Эндпоинт для демонстрации retry.
///
/// Первые два запроса возвращают 500, третий — 200.
/// Счётчик сбрасывается вручную через `/api/flaky/reset`.
pub async fn flaky() -> Result<Json<Value>, StatusCode> {
    let n = FLAKY_COUNTER.fetch_add(1, Ordering::SeqCst);

    if n < 2 {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(json!({
        "message": "Успех после двух неудач",
        "attempt": n + 1
    })))
}

/// Сбрасывает счётчик flaky-эндпоинта.
pub async fn flaky_reset() -> Json<Value> {
    FLAKY_COUNTER.store(0, Ordering::SeqCst);
    Json(json!({ "reset": true }))
}

// ============================================================
// Демонстрация ошибок
// ============================================================

/// Эндпоинт, который всегда возвращает 500.
///
/// Используется для демонстрации и тестов: сервер отвечает
/// мгновенно, без сетевых сюрпризов и без зависимости от внешних
/// сервисов вроде httpstat.us.
pub async fn always_500() -> Result<Json<Value>, StatusCode> {
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

// ============================================================
// Демонстрация таймаутов и отмены
// ============================================================

/// Эндпоинт с задержкой по умолчанию (5 секунд).
///
/// Используется по маршруту `/api/slow`.
pub async fn slow_default() -> Json<Value> {
    slow_with_delay(5).await
}

/// Эндпоинт с задержкой, указанной в URL.
///
/// `/api/slow/10` — задержка 10 секунд.
///
/// **Почему не `Path<Option<u64>>`.** Axum извлекает path-параметры
/// через serde, и `Option<u64>` ожидает либо явный `null`, либо число.
/// Строка `"10"` — ни то, ни другое, поэтому serde падает с
/// «Unsupported type». Разделение на два обработчика — идиоматичнее:
/// маршрут без параметра идёт в `slow_default`, с параметром — в `slow`.
pub async fn slow(Path(secs): Path<u64>) -> Json<Value> {
    slow_with_delay(secs).await
}

/// Общая логика: ждём `secs` секунд и возвращаем JSON.
async fn slow_with_delay(secs: u64) -> Json<Value> {
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;

    Json(json!({
        "message": format!("Ответ получен через {secs} секунд"),
        "delay_secs": secs
    }))
}

/// Эндпоинт с задержкой, указанной в URL.
///
/// `/api/delay/2` — ответ через 2 секунды.
/// Используется для демонстрации параллельных запросов и замеров времени.
pub async fn delay(Path(secs): Path<u64>) -> Json<Value> {
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;

    Json(json!({
        "message": format!("Ответ через {secs} секунд"),
        "delay_secs": secs
    }))
}

/// Эндпоинт с задержкой 300 мс, считающий одновременные запросы.
pub async fn concurrency_test() -> Json<Value> {
    let current = CONCURRENT.fetch_add(1, Ordering::SeqCst) + 1;
    MAX_CONCURRENT.fetch_max(current, Ordering::SeqCst);

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    CONCURRENT.fetch_sub(1, Ordering::SeqCst);
    Json(json!({ "ok": true }))
}

/// Сбрасывает счётчик максимальной одновременности.
pub async fn concurrency_reset() -> Json<Value> {
    MAX_CONCURRENT.store(0, Ordering::SeqCst);
    Json(json!({ "reset": true }))
}

/// Возвращает максимальное количество одновременных запросов.
pub async fn concurrency_stats() -> Json<Value> {
    Json(json!({
        "max_concurrent": MAX_CONCURRENT.load(Ordering::SeqCst)
    }))
}


/// Возвращает простое изображение PNG (1x1 пиксель).
///
/// Используется для демонстрации `response.blob()`.
pub async fn image() -> impl IntoResponse {
    // Минимальный PNG 1x1 прозрачный.
    // Байты: сигнатура PNG, IHDR, IDAT, IEND.
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width=1, height=1
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, // IDAT chunk
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, // IEND chunk
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];

    (
        [(header::CONTENT_TYPE, "image/png")],
        png,
    )
}

/// Возвращает бинарные данные фиксированной длины.
///
/// Используется для демонстрации `response.arrayBuffer()` и `get_bytes`.
pub async fn binary() -> impl IntoResponse {
    let bytes: Vec<u8> = (0u8..=255).collect(); // 256 байт
    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        bytes,
    )
}



/// Потоковый эндпоинт: отдаёт `count` чанков с задержкой.
///
/// Формат запроса: `/api/stream/:count`.
/// Каждый чанк — 1024 байта случайных данных. Между чанками — пауза
/// 100 мс, чтобы клиент мог наблюдать прогресс, а не получить всё разом.
///
/// Используется для демонстрации `ReadableStream` на клиенте.
pub async fn stream_chunks(
    axum::extract::Path(count): axum::extract::Path<u32>,
) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);

    tokio::spawn(async move {
        for i in 0..count {
            // Генерируем чанк: номер итерации в первых байтах, остальное — заполнитель.
            let mut chunk = vec![0u8; 1024];
            chunk[0] = (i & 0xFF) as u8;
            chunk[1] = ((i >> 8) & 0xFF) as u8;

            if tx.send(Ok(chunk)).await.is_err() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });

    let stream = ReceiverStream::new(rx);

    Response::builder()
        .header("Content-Type", "application/octet-stream")
        .body(Body::from_stream(stream))
        .unwrap()
}

// ============================================================
// 17.7: Стриминговый NDJSON-эндпоинт
// ============================================================


/// Отдаёт NDJSON-поток с `count` записями.
///
/// Формат: одна JSON-строка на строку, разделитель — `\n`.
/// Между строками — пауза 50 мс, чтобы клиент видел прогресс
/// в реальном времени, а не «всё сразу».
///
/// Content-Type: application/x-ndjson.
pub async fn stream_records(
    axum::extract::Path(count): axum::extract::Path<u64>,
) -> Response {
    // Ограничим count сверху, чтобы случайно не запустить
    // генерацию на миллион записей.
    let count = count.min(1000);

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);

    tokio::spawn(async move {
        for i in 0..count {
            let level = match i % 4 {
                0 => "info",
                1 => "warn",
                2 => "error",
                _ => "debug",
            };

            let record = serde_json::json!({
                "id": i,
                "level": level,
                "message": format!("Событие номер {i}"),
                "timestamp": chrono::Utc::now().timestamp_millis(),
            });

            // Одна запись — одна строка. Завершающий \n обязателен.
            let line = format!("{record}\n");

            if tx.send(Ok(line.into_bytes())).await.is_err() {
                // Клиент отключился — прекращаем генерацию.
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });

    let stream = ReceiverStream::new(rx);

    Response::builder()
        .header("Content-Type", "application/x-ndjson")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Принимает файл через multipart/form-data и сохраняет на диск.
///
/// **Сохраняет файл в общий каталог `uploads_root()/{upload_id}/{filename}`.**
/// Это тот же каталог, что использует chunked upload — все загрузки
/// (fetch, XHR, chunked) появляются в общем списке `/api/upload/chunk/list`
/// и могут быть удалены через `/api/upload/chunk/{upload_id}`.
///
/// Тело читается потоково: чанк за чанком пишется в файл, память
/// O(chunk_size). Работает для файлов любого размера.
pub async fn upload(mut multipart: Multipart) -> Result<Json<Value>, StatusCode> {
    let mut file_name = String::from("unnamed");
    let mut content_type = String::from("application/octet-stream");
    let mut size: usize = 0;
    let mut hasher = Sha256::new();
    let mut found_file = false;

    // Генерируем upload_id заранее — клиент получит его в ответе.
    let upload_id = generate_upload_id();
    let dir = uploads_root().join(&upload_id);

    loop {
        let field_opt = multipart
            .next_field()
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let field = match field_opt {
            Some(f) => f,
            None => break,
        };

        let name = field.name().map(String::from).unwrap_or_default();
        if name != "file" {
            continue;
        }
        found_file = true;

        if let Some(fname) = field.file_name() {
            file_name = fname.to_string();
        }
        if let Some(ct) = field.content_type() {
            content_type = ct.to_string();
        }

        // Создаём директорию и файл — один раз на загрузку.
        // Локальная переменная вместо Option: обёртка не нужна,
        // файл создаётся и используется здесь же.
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let output_path = dir.join(&file_name);
        let mut out_file = tokio::fs::File::create(&output_path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Читаем поле по чанкам и пишем в файл.
        let mut field = field;
        loop {
            let chunk_opt = field
                .chunk()
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let chunk = match chunk_opt {
                Some(c) => c,
                None => break,
            };
            size += chunk.len();
            hasher.update(&chunk);
            tokio::io::AsyncWriteExt::write_all(&mut out_file, &chunk)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        tokio::io::AsyncWriteExt::flush(&mut out_file)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    if !found_file {
        // Убираем пустую директорию.
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(StatusCode::BAD_REQUEST);
    }

    let sha256_hex = hex::encode(hasher.finalize());

    Ok(Json(json!({
        "status": "ok",
        "upload_id": upload_id,
        "filename": file_name,
        "size": size,
        "content_type": content_type,
        "sha256": sha256_hex,
    })))
}

// ============================================================
// 18.14: Chunked upload на диске
// ============================================================

use std::collections::{HashMap, HashSet};
use std::path::{PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::Query;
use serde::Deserialize;
use tokio::sync::Mutex;

/// Хранилище метаданных активных загрузок.
///
/// **В памяти — только метаданные:** имя файла, размеры,
/// путь к директории, множество полученных индексов. Сами
/// байты — на диске. Память O(number_of_uploads), не O(size).
pub type ChunkStore = Arc<Mutex<HashMap<String, UploadState>>>;

/// Состояние одной активной загрузки.
pub struct UploadState {
    pub filename: String,
    pub total_size: usize,
    pub total_chunks: usize,
    /// Директория с .part-файлами на диске.
    pub dir: PathBuf,
    /// Полученные индексы чанков (для проверки полноты).
    pub received: HashSet<usize>,
}

/// Корневая директория для всех загрузок.
/// `std::env::temp_dir()` → `/tmp` в Linux, `%TEMP%` в Windows.
pub fn uploads_root() -> PathBuf {
    return std::env::temp_dir().join("hello-wasm-uploads");
}

/// Проверяет, что `upload_id` безопасен как имя директории.
///
/// Отсекает `../`, абсолютные пути, пустые строки, юникод-сюрпризы.
/// Без этой проверки клиент мог бы прислать `upload_id=../../etc`
/// и получить запись вне разрешённой директории.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_'
        })
}

#[derive(Deserialize)]
pub struct ChunkQuery {
    pub upload_id: String,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub filename: String,
    pub total_size: usize,
}

/// Принимает один чанк файла и записывает его на диск.
///
/// **Формат запроса:**
/// `POST /api/upload/chunk?upload_id=X&chunk_index=N&total_chunks=M&filename=F&total_size=S`
/// Тело — сырые байты чанка.
///
/// **Ответ:** `{"received": N}` — сколько чанков получено всего.
///
/// **Память:** O(chunk_size). Даже 4-гигабайтный файл уходит
/// при 256 КБ RAM на чанк.
pub async fn upload_chunk(
    axum::extract::Extension(store): axum::extract::Extension<ChunkStore>,
    Query(q): Query<ChunkQuery>,
    body: Bytes,
) -> Result<Json<Value>, StatusCode> {
    if !is_safe_id(&q.upload_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if q.chunk_index >= q.total_chunks {
        return Err(StatusCode::BAD_REQUEST);
    }

    let dir = uploads_root().join(&q.upload_id);

    // Первое появление upload_id — создаём состояние.
    // Для существующих — проверяем, что метаданные совпадают.
    {
        let mut guard = store.lock().await;
        let state = guard.entry(q.upload_id.clone()).or_insert_with(|| {
            UploadState {
                filename: q.filename.clone(),
                total_size: q.total_size,
                total_chunks: q.total_chunks,
                dir: dir.clone(),
                received: HashSet::new(),
            }
        });
        if state.total_chunks != q.total_chunks
            || state.total_size != q.total_size
            || state.filename != q.filename
        {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    // Мьютекс отпущен. Дисковые операции не блокируют другие загрузки.

    // Пишем чанк в файл.
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let path = dir.join(format!("{:08}.part", q.chunk_index));
    tokio::fs::write(&path, &body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Отмечаем чанк полученным.
    let received = {
        let mut guard = store.lock().await;
        let state = guard
            .get_mut(&q.upload_id)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        state.received.insert(q.chunk_index);
        state.received.len()
    };

    Ok(Json(json!({ "received": received })))
}

#[derive(Deserialize)]
pub struct FinalizeRequest {
    pub upload_id: String,
}

/// Финализирует загрузку: склеивает чанки потоково,
/// считает SHA-256, удаляет .part-файлы.
///
/// **Память:** буфер 8 КБ на всё время работы. Файл читается
/// по частям и сразу пишется в выходной — не загружается
/// целиком.
pub async fn finalize_upload(
    axum::extract::Extension(store): axum::extract::Extension<ChunkStore>,
    Json(req): Json<FinalizeRequest>,
) -> Result<Json<Value>, StatusCode> {
    if !is_safe_id(&req.upload_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Забираем состояние из хранилища — запись больше не нужна.
    let state = {
        let mut guard = store.lock().await;
        guard.remove(&req.upload_id).ok_or(StatusCode::NOT_FOUND)?
    };

    if state.received.len() != state.total_chunks {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Открываем выходной файл.
    let output = state.dir.join(&state.filename);
    let mut out = tokio::fs::File::create(&output)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut hasher = Sha256::new();
    let mut total_written: usize = 0;

    // Читаем чанки по порядку и потоково копируем.
    // Память O(8 КБ), не O(file_size).
    for i in 0..state.total_chunks {
        let chunk_path = state.dir.join(format!("{:08}.part", i));
        let mut f = tokio::fs::File::open(&chunk_path)
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        let mut buf = [0u8; 8192];
        loop {
            let n = tokio::io::AsyncReadExt::read(&mut f, &mut buf)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            tokio::io::AsyncWriteExt::write_all(&mut out, &buf[..n])
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            total_written += n;
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut out)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    drop(out);

    // Убираем .part-файлы — остаётся только собранный файл.
    for i in 0..state.total_chunks {
        let path = state.dir.join(format!("{:08}.part", i));
        tokio::fs::remove_file(&path).await.ok();
    }

    if total_written != state.total_size {
        return Err(StatusCode::BAD_REQUEST);
    }

    let sha256_hex = hex::encode(hasher.finalize());

    Ok(Json(json!({
        "status": "ok",
        "filename": state.filename,
        "size": total_written,
        "chunks": state.total_chunks,
        "sha256": sha256_hex,
    })))
}

/// Возвращает метаданные загрузки: список файлов, суммарный размер.
pub async fn upload_status(
    Path(upload_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    if !is_safe_id(&upload_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let dir = uploads_root().join(&upload_id);
    if !dir.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut files: Vec<String> = Vec::new();
    let mut total_size: u64 = 0;
    while let Some(e) = entries
        .next_entry()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        files.push(e.file_name().to_string_lossy().to_string());
        if let Ok(meta) = e.metadata().await {
            total_size += meta.len();
        }
    }

    Ok(Json(json!({
        "upload_id": upload_id,
        "files": files,
        "total_size": total_size,
    })))
}

/// Удаляет загрузку: директорию со всем содержимым.
///
/// Работает и до, и после финализации. Идемпотентен:
/// повторный вызов вернёт `removed: false`, не паникуя.
pub async fn delete_upload(
    axum::extract::Extension(store): axum::extract::Extension<ChunkStore>,
    Path(upload_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    if !is_safe_id(&upload_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Убираем состояние, если оно ещё там.
    {
        let mut guard = store.lock().await;
        guard.remove(&upload_id);
    }

    // Удаляем директорию со всем содержимым.
    let dir = uploads_root().join(&upload_id);
    let existed = dir.exists();
    if existed {
        tokio::fs::remove_dir_all(&dir)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(json!({
        "status": "ok",
        "upload_id": upload_id,
        "removed": existed,
    })))
}

// ============================================================
// TTL-очистка старых загрузок
// ============================================================

/// Запускается при старте сервера. Раз в час удаляет директории
/// загрузок, к которым не обращались более `ttl`.
pub async fn cleanup_old_uploads(ttl: Duration) {
    let root = uploads_root();
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        let _ = cleanup_once(&root, ttl).await;
    }
}

/// Один проход очистки. Возвращает `Ok(())` даже если что-то
/// не удалось — фоновая задача не должна падать.
async fn cleanup_once(
    root: &std::path::Path,
    ttl: Duration,
) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let now = std::time::SystemTime::now();
    let mut entries = tokio::fs::read_dir(root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Ok(metadata) = entry.metadata().await {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age > ttl {
                        tokio::fs::remove_dir_all(&path).await.ok();
                    }
                }
            }
        }
    }
    Ok(())
}

/// Возвращает список всех финализированных загрузок.
///
/// Для каждой директории в `uploads_root()` ищет итоговый файл
/// (не `.part`). Возвращает `[{upload_id, filename, size}, ...]`,
/// отсортированный по имени файла.
pub async fn list_uploads() -> Result<Json<Value>, StatusCode> {
    let root = uploads_root();
    if !root.exists() {
        return Ok(Json(json!({ "uploads": [] })));
    }

    let mut entries = tokio::fs::read_dir(&root)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut uploads: Vec<Value> = Vec::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        let upload_id = entry.file_name().to_string_lossy().to_string();
        let dir = entry.path();

        let mut inner = match tokio::fs::read_dir(&dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Ищем итоговый файл — не `.part`.
        while let Some(item) = inner.next_entry().await.ok().flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            if name.ends_with(".part") {
                continue;
            }
            if let Ok(meta) = item.metadata().await {
                uploads.push(json!({
                    "upload_id": upload_id,
                    "filename": name,
                    "size": meta.len(),
                }));
            }
            break;
        }
    }

    uploads.sort_by(|a, b| {
        let na = a.get("filename").and_then(|v| v.as_str()).unwrap_or("");
        let nb = b.get("filename").and_then(|v| v.as_str()).unwrap_or("");
        na.cmp(nb)
    });

    return Ok(Json(json!({ "uploads": uploads })));
}
/// Генерирует уникальный upload_id для multipart-загрузок
/// (fetch, XHR). Chunked upload генерирует ID на клиенте —
/// там используется этот же формат.
fn generate_upload_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let r: u64 = rand_u64();
    return format!("upload-{ts}-{r}");
}

/// Простой источник случайности без внешних зависимостей.
///
/// Использует хеш от адреса стека и текущего времени — этого
/// достаточно для уникальности upload_id в рамках одной сессии.
/// Для продакшена берите `rand::random::<u64>()`.
fn rand_u64() -> u64 {
    use std::hash::{Hash, Hasher};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    now.hash(&mut hasher);
    (&now as *const _ as usize).hash(&mut hasher);
    return hasher.finish();
}