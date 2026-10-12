//! Chunked upload: файл режется через `Blob.slice`, чанки уходят
//! через fetch, прогресс — по завершённым чанкам.
//!
//! **Без лимита по размеру.** Сервер пишет чанки на диск, память
//! O(chunk_size), а не O(file_size). Работает для 4-гигабайтных
//! файлов на любом железе.
//!
//! **Жизненный цикл.** После успешного `upload_chunked` сервер
//! хранит собранный файл в temp-директории. Клиент получает
//! `upload_id` и может вызвать `delete_chunked_upload` для
//! явной очистки.
//!
//! **Content-Type обязателен.** axum 0.8 строго проверяет
//! Content-Type: `Bytes` требует `application/octet-stream`,
//! `Json` требует `application/json`. Браузер при отправке
//! `ArrayBuffer` через fetch его не ставит — устанавливаем явно.
//!
//! **Отмена.** Флаг `cancel: Rc<Cell<bool>>` проверяется перед
//! каждым чанком. **Таймаут.** Каждый запрос — с таймаутом 30 с.

use std::cell::Cell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{AbortController, Blob, File, Request, RequestInit, Response};

/// Размер одного чанка: 1 МБ.
pub const DEFAULT_CHUNK_SIZE: u32 = 1024 * 1024;

/// Таймаут на один чанк: 30 секунд.
pub const CHUNK_TIMEOUT_MS: u32 = 30_000;

/// Результат успешной загрузки.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// Идентификатор загрузки на сервере — нужен для удаления.
    pub upload_id: String,
    /// Тело ответа сервера (JSON).
    pub response: String,
}

/// Считает количество чанков для файла размера `size`.
pub fn chunk_count(size: f64, chunk_size: u32) -> u32 {
    if size <= 0.0 || chunk_size == 0 {
        return 0;
    }
    let n = (size / chunk_size as f64).ceil();
    return n as u32;
}

/// Генерирует уникальный upload_id.
fn generate_upload_id() -> String {
    let ts = js_sys::Date::now() as u64;
    let r = (js_sys::Math::random() * 1_000_000_000.0) as u64;
    return format!("upload-{ts}-{r}");
}

/// Загружает файл чанками на сервер.
///
/// Возвращает `UploadResult` с `upload_id` — он понадобится
/// для удаления файла с сервера после проверки.
pub async fn upload_chunked<F>(
    file: &File,
    base_url: &str,
    chunk_size: u32,
    cancel: Rc<Cell<bool>>,
    mut on_progress: F,
) -> Result<UploadResult, JsValue>
where
    F: FnMut(u32, u32) + 'static,
{
    let total_size = file.size();
    let total_chunks = chunk_count(total_size, chunk_size);
    if total_chunks == 0 {
        return Err(JsValue::from_str("пустой файл"));
    }

    let upload_id = generate_upload_id();
    let filename = file.name();

    for i in 0..total_chunks {
        if cancel.get() {
            return Err(JsValue::from_str("aborted"));
        }

        let start = (i as f64) * (chunk_size as f64);
        let end = (start + chunk_size as f64).min(total_size);
        let slice = file.slice_with_f64_and_f64(start, end)?;
        let bytes = blob_to_bytes(&slice).await?;

        let url = format!(
            "{base_url}?upload_id={upload_id}&chunk_index={i}\
             &total_chunks={total_chunks}&filename={filename}\
             &total_size={}",
            total_size as u64,
        );

        // Content-Type обязателен: сервер использует axum-экстрактор
        // Bytes, который ждёт application/octet-stream.
        let _ = send_with_timeout(
            &url,
            &bytes,
            "POST",
            Some("application/octet-stream"),
            CHUNK_TIMEOUT_MS,
        )
        .await
        .map_err(|e| {
            JsValue::from_str(&format!("чанк {i} не отправлен: {e:?}"))
        })?;

        on_progress(i + 1, total_chunks);
    }

    // Финализация: тело — JSON, Content-Type application/json.
    let finalize_url = format!("{base_url}/finalize");
    let body_obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &body_obj,
        &JsValue::from_str("upload_id"),
        &JsValue::from_str(&upload_id),
    )?;
    let body_json = js_sys::JSON::stringify(&body_obj.into())?;
    let body_str = body_json
        .as_string()
        .ok_or_else(|| JsValue::from_str("не stringify"))?;

    let response = send_with_timeout(
        &finalize_url,
        body_str.as_bytes(),
        "POST",
        Some("application/json"),
        CHUNK_TIMEOUT_MS,
    )
    .await?;

    return Ok(UploadResult { upload_id, response });
}

/// Удаляет загрузку с сервера: директорию с собранным файлом.
///
/// Идемпотентно: повторный вызов для уже удалённого `upload_id`
/// вернёт `removed: false` без ошибки.
pub async fn delete_chunked_upload(
    base_url: &str,
    upload_id: &str,
) -> Result<String, JsValue> {
    let url = format!("{base_url}/{upload_id}");
    // DELETE без тела — Content-Type не нужен.
    return send_with_timeout(
        &url,
        &[],
        "DELETE",
        None,
        CHUNK_TIMEOUT_MS,
    )
    .await;
}

/// Возвращает метаданные загрузки с сервера.
pub async fn get_chunked_upload_status(
    base_url: &str,
    upload_id: &str,
) -> Result<String, JsValue> {
    let url = format!("{base_url}/{upload_id}");
    return send_with_timeout(&url, &[], "GET", None, CHUNK_TIMEOUT_MS).await;
}

// ---------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------

/// Отправляет запрос с таймаутом и опциональным Content-Type.
///
/// `content_type` обязателен для POST с телом: axum 0.8's `Bytes`
/// требует `application/octet-stream`, `Json` требует
/// `application/json`. Без явного заголовка браузер его не
/// ставит, и сервер отвечает 415.
async fn send_with_timeout(
    url: &str,
    body: &[u8],
    method: &str,
    content_type: Option<&str>,
    timeout_ms: u32,
) -> Result<String, JsValue> {
    let controller = AbortController::new()?;
    let signal = controller.signal();

    // Отдельная задача отменяет запрос через timeout_ms.
    {
        let controller_c = controller.clone();
        spawn_local(async move {
            TimeoutFuture::new(timeout_ms).await;
            controller_c.abort();
        });
    }

    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_signal(Some(&signal));

    if let Some(ct) = content_type {
        let headers = web_sys::Headers::new()?;
        headers.set("Content-Type", ct)?;
        opts.set_headers(&headers);
    }

    if !body.is_empty() {
        let uint8 = js_sys::Uint8Array::from(body);
        opts.set_body(&uint8.buffer());
    }

    let request = Request::new_with_str_and_init(url, &opts)?;
    return send_and_read_text(&request).await;
}

async fn blob_to_bytes(blob: &Blob) -> Result<Vec<u8>, JsValue> {
    let promise = blob.array_buffer();
    let value = JsFuture::from(promise).await?;
    let array_buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    return Ok(js_sys::Uint8Array::new(&array_buffer).to_vec());
}

async fn send_and_read_text(request: &Request) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let promise = window.fetch_with_request(request);
    let resp_value = JsFuture::from(promise).await?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("ответ не Response"))?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "HTTP {} {}",
            resp.status(),
            resp.status_text(),
        )));
    }

    let text_promise = resp.text()?;
    let text_value = JsFuture::from(text_promise).await?;
    return text_value
        .as_string()
        .ok_or_else(|| JsValue::from_str("ответ не строка"));
}

/// Элемент списка загрузок на сервере.
///
/// Содержит всё, что нужно клиенту: имя файла для отображения
/// и `upload_id` — служебный идентификатор для операций удаления.
/// **`upload_id` не показывается пользователю**, он хранится
/// внутри замыкания кнопки «Удалить».
#[derive(Debug, Clone)]
pub struct UploadInfo {
    pub upload_id: String,
    pub filename: String,
    pub size: f64,
}

/// Запрашивает список всех активных загрузок на сервере.
pub async fn list_chunked_uploads(
    base_url: &str,
) -> Result<Vec<UploadInfo>, JsValue> {
    let url = format!("{base_url}/list");
    let text = send_with_timeout(
        &url, &[], "GET", None, CHUNK_TIMEOUT_MS,
    )
    .await?;

    let parsed: JsValue = js_sys::JSON::parse(&text)?;
    let uploads_js = js_sys::Reflect::get(
        &parsed,
        &JsValue::from_str("uploads"),
    )?;
    let arr = uploads_js
        .dyn_into::<js_sys::Array>()
        .map_err(|_| JsValue::from_str("uploads не массив"))?;

    let mut out: Vec<UploadInfo> = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let item = arr.get(i);
        let upload_id = js_sys::Reflect::get(
            &item,
            &JsValue::from_str("upload_id"),
        )?
        .as_string()
        .ok_or_else(|| JsValue::from_str("нет upload_id"))?;
        let filename = js_sys::Reflect::get(
            &item,
            &JsValue::from_str("filename"),
        )?
        .as_string()
        .ok_or_else(|| JsValue::from_str("нет filename"))?;
        let size = js_sys::Reflect::get(
            &item,
            &JsValue::from_str("size"),
        )?
        .as_f64()
        .ok_or_else(|| JsValue::from_str("нет size"))?;

        out.push(UploadInfo { upload_id, filename, size });
    }

    return Ok(out);
}