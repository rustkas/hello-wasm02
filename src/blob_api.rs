//! Обёртка над Blob, Object URL и скачиванием файлов.
//!
//! **Что даёт.** Четыре универсальных примитива:
//! * `create_blob(bytes, mime)` — Blob из байтов;
//! * `create_object_url(blob)` — временный URL вида `blob:…`;
//! * `revoke_object_url(url)` — освободить URL (идемпотентно);
//! * `download_url(url, filename)` — скачать по URL через `<a download>`.
//!
//! **Что НЕ содержит.** Никаких форматов (JSON, PNG, HTML),
//! никакой генерации данных, никакого UI. Только байты → Blob → URL.
//!
//! **Почему через Reflect.** Сигнатуры конструкторов `Blob`
//! в `web-sys` менялись между версиями. Reflect-подход работает
//! везде и не привязывается к крейту.
//!
//! **Правило WASM-инференса.** `format!` в tail-позиции — через
//! `format_str(format_args!(…))`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// ===========================================================================
// Строковые хелперы
// ===========================================================================

#[inline]
fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

// ===========================================================================
// Создание Blob
// ===========================================================================

/// Создать Blob из байтов с MIME-типом.
///
/// `mime` должен быть валидным типом: `"text/plain"`,
/// `"application/json"`, `"image/png"`, `"text/html;charset=utf-8"`.
/// Пустая строка тоже допустима — браузер подставит
/// `"application/octet-stream"` при использовании.
pub fn create_blob(bytes: &[u8], mime: &str) -> Result<web_sys::Blob, JsValue> {
    let u8arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&u8arr.buffer());

    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("type"),
        &JsValue::from_str(mime),
    )?;

    let global = js_sys::global();
    let ctor_js = js_sys::Reflect::get(&global, &JsValue::from_str("Blob"))?;
    let ctor: js_sys::Function = ctor_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("Blob не конструктор"))?;

    let args = js_sys::Array::new();
    args.push(&parts);
    args.push(&options);

    let blob_js = js_sys::Reflect::construct(&ctor, &args)?;
    blob_js
        .dyn_into::<web_sys::Blob>()
        .map_err(|_| JsValue::from_str("не Blob"))
}

/// Размер Blob в байтах.
pub fn blob_size(blob: &web_sys::Blob) -> f64 {
    blob.size()
}

/// MIME-тип Blob.
pub fn blob_mime(blob: &web_sys::Blob) -> String {
    blob.type_()
}

// ===========================================================================
// Object URL
// ===========================================================================

/// Создать Object URL из Blob.
///
/// Возвращает строку вида `blob:http://localhost:5173/uuid`.
/// **Каждый вызов `create_object_url` нужно парно завершить
/// `revoke_object_url`.** Иначе Blob держится в памяти до
/// закрытия вкладки.
pub fn create_object_url(blob: &web_sys::Blob) -> Result<String, JsValue> {
    web_sys::Url::create_object_url_with_blob(blob)
}

/// Освободить Object URL.
///
/// **Идемпотентно.** Повторный revoke того же URL — no-op,
/// не паника. Браузер молча игнорирует.
pub fn revoke_object_url(url: &str) {
    web_sys::Url::revoke_object_url(url).ok();
}

// ===========================================================================
// Скачивание
// ===========================================================================

/// Скачать ресурс по URL под указанным именем.
///
/// Создаёт `<a href="..." download="...">`, кладёт его в `<body>`,
/// программно кликает, удаляет. `<a>` **обязан быть в DOM** —
/// некоторые браузеры игнорируют программный click на отвязанном
/// элементе.
///
/// **User gesture не требуется.** Blob URL — безопасный ресурс
/// того же origin, браузер разрешает скачивание программным
/// кликом. В отличие от `showSaveFilePicker`, который требует
/// клика пользователя.
pub fn download_url(url: &str, filename: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let doc = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let body = doc.body().ok_or_else(|| JsValue::from_str("no body"))?;

    let a = doc.create_element("a")?;
    a.set_attribute("href", url)?;
    a.set_attribute("download", filename)?;
    a.set_attribute("style", "display:none")?;
    body.append_child(&a)?;

    let a_el: web_sys::HtmlElement = a
        .dyn_into()
        .map_err(|_| JsValue::from_str("не HtmlElement"))?;
    a_el.click();

    a_el.remove();
    Ok(())
}

/// Утилита: отрезать префикс blob URL для отображения.
///
/// `blob:http://localhost:5173/uuid` → `…/uuid`.
/// Полная строка остаётся внутри — это только для UI.
pub fn shorten_url(url: &str) -> String {
    let max = 48;
    if url.len() <= max {
        return url.to_string();
    }
    let tail: String = url.chars().rev().take(24).collect::<String>();
    let tail: String = tail.chars().rev().collect();
    let s: String = format_str(format_args!("…{tail}"));
    s
}