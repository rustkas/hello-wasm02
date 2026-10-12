
//! Object URL — временные ссылки на Blob для превью и скачивания.
//! Модуль автономен: не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;
use web_sys::{Blob, HtmlAnchorElement, Url};

/// Создаёт Object URL для `Blob`.
///
/// Возвращает строку вида `blob:http://localhost:3000/uuid`.
/// URL остаётся валидным, пока не будет освобождён вызовом
/// [`revoke_object_url`].
pub fn create_object_url(blob: &Blob) -> Result<String, JsValue> {
    return Url::create_object_url_with_blob(blob);
}

/// Освобождает Object URL, позволяя браузеру выгрузить связанный Blob.
///
/// После вызова URL становится недействительным: `<img>`, `<video>`
/// и другие потребители не смогут загрузить данные.
pub fn revoke_object_url(url: &str) -> Result<(), JsValue> {
    return Url::revoke_object_url(url);
}

/// Подставляет Object URL в `src` элемента `<img>`.
///
/// Полезно для превью изображений.
pub fn set_image_src(img: &web_sys::HtmlImageElement, url: &str) {
    img.set_src(url);
}

/// Подставляет Object URL в `src` элемента `<video>`.
pub fn set_video_src(video: &web_sys::HtmlVideoElement, url: &str) {
    video.set_src(url);
}

/// Инициирует скачивание `Blob` через `<a download>`.
///
/// Создаёт временный `<a>`, подставляет Object URL и имя файла,
/// программно кликает, а затем освобождает URL.
///
/// Возвращает созданный Object URL — на случай, если вызывающая
/// сторона хочет управлять его жизненным циклом самостоятельно.
pub fn download_blob(blob: &Blob, file_name: &str) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let url = Url::create_object_url_with_blob(blob)?;

    let anchor: HtmlAnchorElement = document
        .create_element("a")?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не anchor"))?;

    anchor.set_href(&url);
    anchor.set_download(file_name);
    anchor.click();

    // Браузер уже начал скачивание — освобождаем URL.
    Url::revoke_object_url(&url)?;

    return Ok(url);
}

/// Инициирует скачивание `Blob` с автоматическим определением
/// имени файла из MIME-типа (fallback: `download.bin`).
pub fn download_blob_with_mime(blob: &Blob, mime_type: &str) -> Result<String, JsValue> {
    let file_name = file_name_from_mime(mime_type);
    return download_blob(blob, &file_name);
}

/// Определяет имя файла по MIME-типу (базовый набор).
fn file_name_from_mime(mime_type: &str) -> String {
    match mime_type {
        "image/png" => "download.png".to_string(),
        "image/jpeg" => "download.jpg".to_string(),
        "image/gif" => "download.gif".to_string(),
        "image/webp" => "download.webp".to_string(),
        "text/plain" => "download.txt".to_string(),
        "application/json" => "download.json".to_string(),
        "application/pdf" => "download.pdf".to_string(),
        _ => "download.bin".to_string(),
    }
}