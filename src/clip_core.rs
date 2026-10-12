//! Операции с содержимым системного буфера обмена.
//!
//! Async Clipboard API даёт два уровня:
//!
//!   • `writeText` / `readText` — только текст. Простой API, работает
//!     без явных разрешений (для записи достаточно user gesture).
//!   • `write` / `read` — `ClipboardItem[]`. Каждый `ClipboardItem`
//!     содержит один или несколько MIME-форматов одновременно (например,
//!     `text/plain` + `text/html` + `image/png`). Это то, что делает
//!     «копировать как Markdown» возможным: приложение вставляет фрагмент
//!     в трёх форматах, а получатель выбирает подходящий.
//!
//! ПРАВИЛО: `navigator.clipboard` требует secure context (HTTPS или
//! localhost). При отсутствии — явная ошибка, не паника.
//!
//! ПРАВИЛО: ошибки пробрасываем как `JsValue` без обёртки в строку.
//! `NotAllowedError` — основной сценарий отказа (нет user gesture,
//! неподдерживаемый MIME, отклонённое разрешение).
//!
//! ПРАВИЛО: `ClipboardItem` — класс, вызывается через
//! `js_sys::Reflect::construct`, а не через `Function::call1`.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

// ============================================================
//                    ОШИБКИ
// ============================================================

/// Классифицированная ошибка Clipboard API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipError {
    /// Нет user gesture, отклонено разрешение или неподдерживаемый MIME.
    NotAllowed,
    /// Нет разрешения `clipboard-read`.
    PermissionDenied,
    /// Secure context отсутствует.
    Security,
    /// Буфер пуст или формат не найден.
    NotFound,
    /// Данные слишком большие или неподдерживаемый формат.
    Data,
    Other(String),
}

impl ClipError {
    pub fn short(&self) -> &'static str {
        match self {
            ClipError::NotAllowed => "отказано: нет жеста или формат отвергнут",
            ClipError::PermissionDenied => "разрешение отклонено",
            ClipError::Security => "нужен HTTPS или localhost",
            ClipError::NotFound => "в буфере нет данных такого формата",
            ClipError::Data => "неподдерживаемый формат данных",
            ClipError::Other(_) => "ошибка буфера обмена",
        }
    }
    pub fn name(&self) -> &str {
        match self {
            ClipError::NotAllowed => "NotAllowedError",
            ClipError::PermissionDenied => "NotAllowedError",
            ClipError::Security => "SecurityError",
            ClipError::NotFound => "NotFoundError",
            ClipError::Data => "DataError",
            ClipError::Other(s) => s.as_str(),
        }
    }
}

pub fn clip_error_name(e: &JsValue) -> String {
    if let Some(dx) = e.dyn_ref::<web_sys::DomException>() {
        return dx.name();
    }
    if let Some(s) = e.as_string() {
        return s;
    }
    js_sys::Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn classify_clip_error(e: &JsValue) -> ClipError {
    match clip_error_name(e).as_str() {
        "NotAllowedError" => ClipError::NotAllowed,
        "SecurityError" => ClipError::Security,
        "NotFoundError" => ClipError::NotFound,
        "DataError" => ClipError::Data,
        other => ClipError::Other(other.to_string()),
    }
}

// ============================================================
//                    ДОСТУП К CLIPBOARD
// ============================================================

fn clipboard() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let nav = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))
        .map_err(|e| JsValue::from_str(&format!("Reflect navigator: {e:?}")))?;
    let clip = js_sys::Reflect::get(&nav, &JsValue::from_str("clipboard"))
        .map_err(|e| JsValue::from_str(&format!("Reflect clipboard: {e:?}")))?;
    if clip.is_undefined() || clip.is_null() {
        return Err(JsValue::from_str(
            "navigator.clipboard недоступен (нужен HTTPS или localhost)",
        ));
    }
    Ok(clip)
}

/// true, если `navigator.clipboard` доступен.
pub fn is_available() -> bool {
    return clipboard().is_ok();
}

/// true, если браузер разрешает записывать этот MIME в ClipboardItem.
///
/// Использует статический `ClipboardItem.supports(mime)` (Chrome 121+).
/// Если метод недоступен — `true` (оптимистично: пусть попытка
/// провалится с осмысленной ошибкой).
pub fn supports_mime(mime: &str) -> bool {
    let global = js_sys::global();
    let ctor = match js_sys::Reflect::get(&global, &JsValue::from_str("ClipboardItem")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return true,
    };
    let supports = match js_sys::Reflect::get(&ctor, &JsValue::from_str("supports")) {
        Ok(v) if !v.is_undefined() => v,
        _ => return true,
    };
    let f: js_sys::Function = match supports.dyn_into() {
        Ok(f) => f,
        Err(_) => return true,
    };
    f.call1(&ctor, &JsValue::from_str(mime))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

// ============================================================
//                    ТЕКСТ
// ============================================================

/// `clipboard.writeText(text)`.
pub async fn write_text(text: &str) -> Result<(), JsValue> {
    let clip = clipboard()?;
    let f = method(&clip, "writeText")?;
    let promise = f
        .call1(&clip, &JsValue::from_str(text))
        .map_err(|e| JsValue::from_str(&format!("writeText: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("writeText не вернул Promise"))?;
    JsFuture::from(promise).await?;
    Ok(())
}

/// `clipboard.readText()`. Пустой буфер → пустая строка.
pub async fn read_text() -> Result<String, JsValue> {
    let clip = clipboard()?;
    let f = method(&clip, "readText")?;
    let promise = f
        .call0(&clip)
        .map_err(|e| JsValue::from_str(&format!("readText: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("readText не вернул Promise"))?;
    let result = JsFuture::from(promise).await?;
    Ok(result.as_string().unwrap_or_default())
}

// ============================================================
//                    CLIPBOARD ITEM
// ============================================================

/// Создаёт `ClipboardItem` из одного MIME-формата и `Blob`.
fn make_item(mime: &str, blob: &web_sys::Blob) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("ClipboardItem"))
        .map_err(|e| JsValue::from_str(&format!("Reflect ClipboardItem: {e:?}")))?;
    if ctor.is_undefined() || ctor.is_null() {
        return Err(JsValue::from_str("ClipboardItem недоступен"));
    }
    let ctor: js_sys::Function = ctor
        .dyn_into()
        .map_err(|_| JsValue::from_str("ClipboardItem не конструктор"))?;

    let data = js_sys::Object::new();
    js_sys::Reflect::set(&data, &JsValue::from_str(mime), blob)
        .map_err(|e| JsValue::from_str(&format!("set mime: {e:?}")))?;

    // ПРАВИЛО: `ClipboardItem` — класс, вызывается только через `new`.
    // `Function::call1` даёт TypeError: "constructor: 'new' is required".
    let args = js_sys::Array::new();
    args.push(&data);
    js_sys::Reflect::construct(&ctor, &args)
        .map_err(|e| JsValue::from_str(&format!("new ClipboardItem: {e:?}")))
}

/// Создаёт `Blob` из строки с указанным MIME.
pub fn text_blob(text: &str, mime: &str) -> Result<web_sys::Blob, JsValue> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
        .map_err(|e| JsValue::from_str(&format!("Blob::new: {e:?}")))
}

/// Создаёт `Blob` из байтов с указанным MIME.
pub fn bytes_blob(bytes: &[u8], mime: &str) -> Result<web_sys::Blob, JsValue> {
    let uint8 = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&uint8.buffer());
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|e| JsValue::from_str(&format!("Blob::new bytes: {e:?}")))
}

/// Записывает один `ClipboardItem` с одним MIME-форматом.
pub async fn write_item(mime: &str, blob: &web_sys::Blob) -> Result<(), JsValue> {
    let item = make_item(mime, blob)?;
    let items = js_sys::Array::new();
    items.push(&item);
    write_items(&items).await
}

/// Записывает несколько MIME-форматов в одном `ClipboardItem`.
///
/// ПРАВИЛО: браузер пропускает только поддерживаемые MIME. Один
/// неподдерживаемый отклоняет **всю** пачку с `NotAllowedError`.
/// Фильтруйте через `supports_mime` до вызова.
pub async fn write_multi_formats(formats: &[(&str, &str)]) -> Result<(), JsValue> {
    let data = js_sys::Object::new();
    for (mime, text) in formats {
        let blob = text_blob(text, mime)?;
        js_sys::Reflect::set(&data, &JsValue::from_str(mime), &blob)
            .map_err(|e| JsValue::from_str(&format!("set {mime}: {e:?}")))?;
    }
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("ClipboardItem"))
        .map_err(|e| JsValue::from_str(&format!("Reflect ClipboardItem: {e:?}")))?;
    let ctor: js_sys::Function = ctor
        .dyn_into()
        .map_err(|_| JsValue::from_str("ClipboardItem не конструктор"))?;

    let args = js_sys::Array::new();
    args.push(&data);
    let item = js_sys::Reflect::construct(&ctor, &args)
        .map_err(|e| JsValue::from_str(&format!("new ClipboardItem: {e:?}")))?;

    let items = js_sys::Array::new();
    items.push(&item);
    write_items(&items).await
}

/// Низкоуровневый `clipboard.write(items: ClipboardItem[])`.
async fn write_items(items: &js_sys::Array) -> Result<(), JsValue> {
    let clip = clipboard()?;
    let f = method(&clip, "write")?;
    let promise = f
        .call1(&clip, items)
        .map_err(|e| JsValue::from_str(&format!("write: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("write не вернул Promise"))?;
    JsFuture::from(promise).await?;
    Ok(())
}

/// Один элемент из буфера: MIME-тип и `Blob` с данными.
#[derive(Debug, Clone)]
pub struct ClipItem {
    pub mime: String,
    pub blob: web_sys::Blob,
}

/// Читает все элементы буфера.
pub async fn read_items() -> Result<Vec<ClipItem>, JsValue> {
    let clip = clipboard()?;
    let f = method(&clip, "read")?;
    let promise = f
        .call0(&clip)
        .map_err(|e| JsValue::from_str(&format!("read: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("read не вернул Promise"))?;
    let result = JsFuture::from(promise).await?;
    let arr: js_sys::Array = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("read вернул не Array"))?;

    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let item = arr.get(i);
        let types_val = js_sys::Reflect::get(&item, &JsValue::from_str("types"))
            .map_err(|e| JsValue::from_str(&format!("item.types: {e:?}")))?;
        let types: js_sys::Array = types_val
            .dyn_into()
            .map_err(|_| JsValue::from_str("types не Array"))?;
        for j in 0..types.length() {
            let mime = types.get(j).as_string().unwrap_or_default();
            if let Ok(blob) = get_item_type(&item, &mime).await {
                out.push(ClipItem { mime, blob });
            }
        }
    }
    Ok(out)
}

async fn get_item_type(item: &JsValue, mime: &str) -> Result<web_sys::Blob, JsValue> {
    let f = method(item, "getType")?;
    let promise = f
        .call1(item, &JsValue::from_str(mime))
        .map_err(|e| JsValue::from_str(&format!("getType: {e:?}")))?;
    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| JsValue::from_str("getType не вернул Promise"))?;
    let blob = JsFuture::from(promise).await?;
    blob.dyn_into::<web_sys::Blob>()
        .map_err(|_| JsValue::from_str("getType вернул не Blob"))
}

/// Читает `Blob` как текст.
pub async fn blob_to_text(blob: &web_sys::Blob) -> Result<String, JsValue> {
    let promise = blob.text();
    let result = JsFuture::from(promise).await?;
    Ok(result.as_string().unwrap_or_default())
}

/// Читает `Blob` как байты.
pub async fn blob_to_bytes(blob: &web_sys::Blob) -> Result<Vec<u8>, JsValue> {
    let promise = blob.array_buffer();
    let result = JsFuture::from(promise).await?;
    let buf: js_sys::ArrayBuffer = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("array_buffer вернул не ArrayBuffer"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

// ============================================================
//                    CF_HTML
// ============================================================

/// Извлекает содержимое из CF_HTML-обёртки, если она есть.
///
/// Chrome оборачивает `text/html` в Clipboard Format (стандарт MS Office):
///   `<html><body><!--StartFragment--><реальный HTML><!--EndFragment--></body></html>`
///
/// Если маркеры есть — берём фрагмент между ними. Иначе — как есть.
pub fn strip_cf_html_wrapper(s: &str) -> &str {
    const START: &str = "<!--StartFragment-->";
    const END: &str = "<!--EndFragment-->";
    if let Some(i) = s.find(START) {
        let after = &s[i + START.len()..];
        if let Some(j) = after.find(END) {
            return after[..j].trim();
        }
    }
    s
}

// ============================================================
//                    ХЕЛПЕРЫ
// ============================================================

fn method(obj: &JsValue, name: &str) -> Result<js_sys::Function, JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("Reflect {name}: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{name} не функция")))
}

// ============================================================
//                    ОШИБКИ JS
// ============================================================

/// Обрезает произвольную строку до первой строки и 120 символов.
///
/// Для JS-ошибок это спасает от попадания stack trace в UI.
/// `TypeError.toString()` включает весь стек — 1000+ символов,
/// которые бесполезны пользователю.
pub fn clamp_first_line(s: &str) -> String {
    let first = s.lines().next().unwrap_or("").trim();
    let mut out: String = first.chars().take(120).collect();
    if first.chars().count() > 120 {
        out.push('…');
    }
    return out;
}

/// Извлекает читаемое сообщение из `JsValue`-ошибки.
///
/// Порядок попыток:
///   1. `DomException` — через `name()` + `message()`.
///   2. Примитив — через `as_string()`.
///   3. JS `Error`-объект — через `Reflect::get("name"/"message")`.
///      Это отсекает stack trace, оставляя только тип и первую строку.
///   4. Fallback — `toString()` через `Reflect`.
///   5. Отладочный `Debug` — в крайнем случае.
///
/// Результат всегда прогнан через `clamp_first_line`.
pub fn js_error_message(e: &JsValue) -> String {
    // 1. DomException — самый частый случай.
    if let Some(dx) = e.dyn_ref::<web_sys::DomException>() {
        let msg = dx.message();
        return format!("{}: {}", dx.name(), clamp_first_line(&msg));
    }

    // 2. Примитив (строка).
    if let Some(s) = e.as_string() {
        return clamp_first_line(&s);
    }

    // 3. JS Error-объект: name + message. Не даём попасть сюда
    //    полному .toString() со стеком.
    if e.is_object() {
        let name = js_sys::Reflect::get(e, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string());
        let message = js_sys::Reflect::get(e, &JsValue::from_str("message"))
            .ok()
            .and_then(|v| v.as_string());

        match (name, message) {
            (Some(n), Some(m)) => {
                return format!("{n}: {}", clamp_first_line(&m));
            }
            (Some(n), None) => {
                return n;
            }
            _ => {}
        }
    }

    // 4. Fallback через .toString() — обрезаем сразу.
    if let Ok(f) = js_sys::Reflect::get(e, &JsValue::from_str("toString"))
        .and_then(|v| v.dyn_into::<js_sys::Function>())
    {
        if let Ok(s) = f.call0(e) {
            if let Some(s) = s.as_string() {
                if !s.is_empty() {
                    return clamp_first_line(&s);
                }
            }
        }
    }

    // 5. Крайний случай.
    return format!("{e:?}")
}