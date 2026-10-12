//! Практический проект: загрузка файла на Rust-сервер.
//! Загрузка файлов на сервер: FormData + fetch.
//!
//! Модуль автономен: зависит только от `web-sys`, `js-sys`,
//! `wasm-bindgen`, `wasm-bindgen-futures`.
//!
//! **Про Content-Type.** Браузер сам выставляет заголовок
//! `Content-Type: multipart/form-data; boundary=...`, когда
//! body — это `FormData`. Если задать Content-Type вручную,
//! браузер не добавит boundary, и сервер не сможет распарсить
//! multipart. Никогда не устанавливайте Content-Type для FormData.
//!
//! **Про прогресс.** fetch не имеет события `upload.onprogress`.
//! Прогресс отправки через fetch получить невозможно — это
//! свойство API. Для прогресса используйте XMLHttpRequest
//! (см. подглаву 18.14). Здесь мы показываем статус и даём
//! отменить запрос через `AbortController`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortSignal, Blob, File, FormData, Request, RequestInit, Response};

/// Загружает `File` без возможности отмены.
///
/// Удобная обёртка для тестов и сценариев, где отмена не нужна.
pub async fn upload_file(file: &File, url: &str) -> Result<String, JsValue> {
    let form_data = FormData::new()?;
    form_data.append_with_blob("file", file)?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&form_data);

    let request = Request::new_with_str_and_init(url, &opts)?;
    return send_and_read_text(&request).await;
}

/// Загружает `Blob` (не File) с явным именем файла.
///
/// Полезно для отправки результатов конвертации: у Blob нет
/// собственного имени, но сервер обычно хочет его увидеть.
pub async fn upload_blob(
    blob: &Blob,
    filename: &str,
    url: &str,
) -> Result<String, JsValue> {
    let form_data = FormData::new()?;
    form_data.append_with_blob_and_filename("file", blob, filename)?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&form_data);

    let request = Request::new_with_str_and_init(url, &opts)?;
    return send_and_read_text(&request).await;
}

/// Проверяет, была ли ошибка вызвана отменой через AbortController.
///
/// Браузеры возвращают `DOMException` с `name == "AbortError"`
/// при отменённом запросе. В Firefox иногда это `TypeError`,
/// но с сообщением, содержащим «abort». Проверяем оба случая.
pub fn is_abort_error(e: &JsValue) -> bool {
    if let Some(exc) = e.dyn_ref::<web_sys::DomException>() {
        return exc.name() == "AbortError";
    }
    if let Some(s) = e.as_string() {
        let lower = s.to_lowercase();
        return lower.contains("abort");
    }
    return false;
}


/// Загружает `File` на сервер через FormData + fetch.
///
/// Возвращает тело ответа сервера как строку.
///
/// `signal` — `AbortSignal` для отмены. Получите его из
/// `AbortController::new()?.signal()`. Если отмена не нужна,
/// используйте [`upload_file`].
pub async fn upload_file_cancellable(
    file: &File,
    url: &str,
    signal: &AbortSignal,
) -> Result<String, JsValue> {
    let form_data = FormData::new()?;
    // append_with_blob и append_with_blob_and_filename принимают
    // `&Blob` (File реализует AsRef<Blob>) и автоматически
    // устанавливают имя файла из File::name() в первом случае,
    // или берут явное имя во втором.
    form_data.append_with_blob("file", file)?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&form_data);
    opts.set_signal(Some(signal));
    // Content-Type НЕ ставим — браузер сам добавит
    // multipart/form-data с корректным boundary.

    let request = Request::new_with_str_and_init(url, &opts)?;
    return send_and_read_text(&request).await;
}

// ---------------------------------------------------------------
// Внутренняя отправка и чтение ответа
// ---------------------------------------------------------------

/// Общий хвост: fetch → проверка Response.ok → чтение text.
async fn send_and_read_text(request: &Request) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let promise = window.fetch_with_request(request);
    let resp_value = JsFuture::from(promise).await?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("ответ не Response"))?;

    if !resp.ok() {
        let status = resp.status();
        let status_text = resp.status_text();
        return Err(JsValue::from_str(&format!(
            "HTTP {} {}",
            status, status_text,
        )));
    }

    let text_promise = resp.text()?;
    let text_value = JsFuture::from(text_promise).await?;
    return text_value
        .as_string()
        .ok_or_else(|| JsValue::from_str("ответ не строка"));
}