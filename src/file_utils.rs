//! Утилиты для работы с ArrayBuffer, Uint8Array и MIME-типами.
//! Модуль автономен: не зависит от других модулей проекта.

use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::{Blob, File};

/// Преобразует `ArrayBuffer` в `Vec<u8>`.
pub fn array_buffer_to_vec(buffer: &ArrayBuffer) -> Vec<u8> {
    let uint8 = Uint8Array::new(buffer);
    uint8.to_vec()
}

/// Преобразует срез `u8` в `Blob` с указанным MIME-типом.
pub fn vec_to_blob(data: &[u8], mime_type: &str) -> Result<Blob, JsValue> {
    let uint8 = Uint8Array::from(data);
    let parts = js_sys::Array::new();
    parts.push(&uint8.buffer());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime_type);
    Blob::new_with_u8_array_sequence_and_options(&parts, &options)
}

/// Извлекает расширение файла из имени (без точки).
///
/// Возвращает `None`, если имя не содержит точки.
/// Имя с завершающей точкой (`"file."`) даёт `Some("")`.
pub fn get_extension(file_name: &str) -> Option<&str> {
    return file_name.rsplit_once('.').map(|(_, ext)| ext);
}

/// Определяет MIME-тип по расширению.
///
/// Регистр расширения не важен. Для неизвестных расширений
/// возвращает `application/octet-stream`.
///
/// Покрывает распространённые форматы: изображения, текст,
/// код, разметку, документы, архивы, аудио, видео.
pub fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        // Изображения
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tiff" | "tif" => "image/tiff",
        // Текст и разметка
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "xml" => "application/xml",
        // Код и данные
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "wasm" => "application/wasm",
        // Документы
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "rtf" => "application/rtf",
        // Архивы
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        // Аудио
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        // Видео
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        // Fallback
        _ => "application/octet-stream",
    }
}

/// Возвращает «эффективный» MIME-тип файла.
///
/// Сначала спрашивает браузер (`file.type_()`). Если браузер вернул
/// непустую строку — используем её. Иначе определяем MIME по
/// расширению через [`mime_from_extension`].
///
/// Это нужно, потому что не все ОС регистрируют MIME для
/// «нетипичных» расширений: `.md`, `.toml`, `.yaml`, `.wasm`
/// часто дают пустой `file.type_()`.
pub fn effective_mime_type(file: &File) -> String {
    let declared = file.type_();
    if !declared.is_empty() {
        return declared;
    }
    let name = file.name();
    let ext = get_extension(&name).unwrap_or("");
    return mime_from_extension(ext).to_string();
}