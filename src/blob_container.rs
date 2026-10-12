//! Blob как контейнер бинарных данных.
//!
//! **Что даёт.** Примитивы работы с Blob **как с данными**:
//! * `from_text` / `from_bytes` — создание;
//! * `read_text` / `read_bytes` / `read_first_chunk` — чтение;
//! * `slice` — нарезка на части;
//! * `is_file` / `file_name` / `file_size` / `file_mime` /
//!   `file_last_modified` — метаданные `File extends Blob`;
//! * `hex_dump`, `crc32` — представление и контрольная сумма.
//!
//! **Что НЕ содержит.** Object URL, скачивание, UI — это тема
//! главы 20. Здесь только работа с Blob-контейнером.
//!
//! **Почему через Reflect.** Сигнатуры методов `Blob` в `web-sys`
//! менялись между версиями. Reflect-подход работает везде и не
//! привязывается к конкретной версии крейта.
//!
//! **Правило WASM-инференса.** `format!`/`vec!` в tail-позиции —
//! через обёртки `format_str`/`vec_of`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

// ===========================================================================
// Строковые и коллекционные хелперы
// ===========================================================================

#[inline]
fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

#[inline]
fn vec_of<T>(v: Vec<T>) -> Vec<T> {
    v
}

// ===========================================================================
// Создание Blob
// ===========================================================================

/// Blob из строки. Строка кодируется в UTF-8.
pub fn from_text(text: &str, mime: &str) -> Result<JsValue, JsValue> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(text));
    construct_blob(&parts, mime)
}

/// Blob из байтов (`Uint8Array` → `ArrayBuffer` → part).
pub fn from_bytes(bytes: &[u8], mime: &str) -> Result<JsValue, JsValue> {
    let u8arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&u8arr.buffer());
    construct_blob(&parts, mime)
}

fn construct_blob(parts: &js_sys::Array, mime: &str) -> Result<JsValue, JsValue> {
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
    args.push(parts);
    args.push(&options);

    js_sys::Reflect::construct(&ctor, &args)
}

/// Размер Blob (или File) в байтах.
pub fn blob_size(blob: &JsValue) -> f64 {
    js_sys::Reflect::get(blob, &JsValue::from_str("size"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// MIME-тип Blob.
pub fn blob_mime(blob: &JsValue) -> String {
    js_sys::Reflect::get(blob, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

// ===========================================================================
// Чтение Blob
// ===========================================================================

/// Прочитать Blob как строку через `.text()`.
///
/// Декодирование — UTF-8. Для не-UTF-8 данных получим
/// replacement-символы U+FFFD. Для бинарных данных используйте
/// `read_bytes`.
pub async fn read_text(blob: &JsValue) -> Result<String, JsValue> {
    let method = js_sys::Reflect::get(blob, &JsValue::from_str("text"))?;
    let f: js_sys::Function = method
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.text не функция"))?;
    let promise: js_sys::Promise = f
        .call0(blob)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.text не Promise"))?;
    let value = JsFuture::from(promise).await?;
    value
        .as_string()
        .ok_or_else(|| JsValue::from_str("blob.text вернул не строку"))
}

/// Прочитать Blob как байты через `.arrayBuffer()`.
pub async fn read_bytes(blob: &JsValue) -> Result<Vec<u8>, JsValue> {
    let method = js_sys::Reflect::get(blob, &JsValue::from_str("arrayBuffer"))?;
    let f: js_sys::Function = method
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.arrayBuffer не функция"))?;
    let promise: js_sys::Promise = f
        .call0(blob)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.arrayBuffer не Promise"))?;
    let ab_value = JsFuture::from(promise).await?;
    let ab: js_sys::ArrayBuffer = ab_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    Ok(u8_from_ab(&ab))
}

/// Прочитать **первый chunk** из `.stream()`.
///
/// `Blob.stream()` отдаёт `ReadableStream<Uint8Array>`. Размер
/// одного chunk — не гарантирован: может быть весь Blob, может
/// быть часть. Мы читаем ровно один chunk и возвращаем его байты
/// (не более `max_bytes`).
pub async fn read_first_chunk(blob: &JsValue, max_bytes: usize) -> Result<Vec<u8>, JsValue> {
    // stream()
    let method = js_sys::Reflect::get(blob, &JsValue::from_str("stream"))?;
    let f: js_sys::Function = method
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.stream не функция"))?;
    let stream = f.call0(blob)?;

    // getReader()
    let get_reader_js = js_sys::Reflect::get(&stream, &JsValue::from_str("getReader"))?;
    let get_reader: js_sys::Function = get_reader_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("stream.getReader не функция"))?;
    let reader = get_reader.call0(&stream)?;

    // reader.read() → Promise<{value, done}>
    let read_js = js_sys::Reflect::get(&reader, &JsValue::from_str("read"))?;
    let read: js_sys::Function = read_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("reader.read не функция"))?;
    let promise: js_sys::Promise = read
        .call0(&reader)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("read не Promise"))?;
    let result = JsFuture::from(promise).await?;

    // Освобождаем reader.
    if let Ok(release_js) = js_sys::Reflect::get(&reader, &JsValue::from_str("releaseLock")) {
        if let Ok(release_fn) = release_js.dyn_into::<js_sys::Function>() {
            let _ = release_fn.call0(&reader);
        }
    }

    let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
        .as_bool()
        .unwrap_or(true);
    if done {
        let v: Vec<u8> = Vec::new();
        return Ok(v);
    }

    let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
    let arr: js_sys::Uint8Array = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("chunk не Uint8Array"))?;
    let len = arr.length() as usize;
    let take = len.min(max_bytes);
    let mut out: Vec<u8> = Vec::with_capacity(take);
    for i in 0..take {
        out.push(arr.get_index(i as u32) as u8);
    }
    Ok(vec_of(out))
}

fn u8_from_ab(ab: &js_sys::ArrayBuffer) -> Vec<u8> {
    let u8arr = js_sys::Uint8Array::new(ab);
    let len = u8arr.length() as usize;
    let mut out: Vec<u8> = vec![0u8; len];
    u8arr.copy_to(&mut out);
    out
}

/// Прочитать **все** chunks через `.stream()` в цикле.
///
/// Возвращает `(chunks, total_bytes)`. Если чанков больше
/// `max_chunks` — останавливается и возвращает то, что успел.
///
/// Полезно, когда `.arrayBuffer()` не подходит — например, для
/// файлов, не влезающих в память целиком. Можно обрабатывать
/// каждый chunk по мере поступления, не держа весь Blob.
pub async fn read_all_chunks(
    blob: &JsValue,
    max_chunks: usize,
) -> Result<(Vec<Vec<u8>>, usize), JsValue> {
    let method = js_sys::Reflect::get(blob, &JsValue::from_str("stream"))?;
    let f: js_sys::Function = method
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.stream не функция"))?;
    let stream = f.call0(blob)?;

    let get_reader_js = js_sys::Reflect::get(&stream, &JsValue::from_str("getReader"))?;
    let get_reader: js_sys::Function = get_reader_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("stream.getReader не функция"))?;
    let reader = get_reader.call0(&stream)?;

    let read_js = js_sys::Reflect::get(&reader, &JsValue::from_str("read"))?;
    let read: js_sys::Function = read_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("reader.read не функция"))?;

    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut total: usize = 0;

    loop {
        if chunks.len() >= max_chunks {
            break;
        }

        let promise: js_sys::Promise = read
            .call0(&reader)?
            .dyn_into()
            .map_err(|_| JsValue::from_str("read не Promise"))?;
        let result = JsFuture::from(promise).await?;

        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
            .as_bool()
            .unwrap_or(true);
        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
        let arr: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| JsValue::from_str("chunk не Uint8Array"))?;
        let len = arr.length() as usize;
        let mut buf: Vec<u8> = vec![0u8; len];
        arr.copy_to(&mut buf);
        total += len;
        chunks.push(buf);
    }

    // Освобождаем reader.
    if let Ok(release_js) = js_sys::Reflect::get(&reader, &JsValue::from_str("releaseLock")) {
        if let Ok(release_fn) = release_js.dyn_into::<js_sys::Function>() {
            let _ = release_fn.call0(&reader);
        }
    }

    Ok((chunks, total))
}

// ===========================================================================
// Slice
// ===========================================================================

/// Нарезать Blob: байты `[start, end)`.
///
/// Возвращает новый Blob — **дескриптор**, не копию данных.
/// MIME-тип наследуется от родителя.
pub fn slice(blob: &JsValue, start: f64, end: f64) -> Result<JsValue, JsValue> {
    let method = js_sys::Reflect::get(blob, &JsValue::from_str("slice"))?;
    let f: js_sys::Function = method
        .dyn_into()
        .map_err(|_| JsValue::from_str("blob.slice не функция"))?;
    f.call2(blob, &JsValue::from_f64(start), &JsValue::from_f64(end))
}

// ===========================================================================
// File inspection
// ===========================================================================

/// Проверка: это File (наследник Blob)?
///
/// Проверяем наличие `name` и `lastModified` — этих свойств нет
/// у чистого Blob, но есть у File.
pub fn is_file(value: &JsValue) -> bool {
    if value.is_null() || value.is_undefined() {
        return false;
    }
    let has_name = js_sys::Reflect::has(value, &JsValue::from_str("name")).unwrap_or(false);
    let has_mod = js_sys::Reflect::has(value, &JsValue::from_str("lastModified"))
        .unwrap_or(false);
    let is_ok: bool = has_name && has_mod;
    is_ok
}

pub fn file_name(file: &JsValue) -> String {
    js_sys::Reflect::get(file, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

pub fn file_size(file: &JsValue) -> f64 {
    js_sys::Reflect::get(file, &JsValue::from_str("size"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

pub fn file_mime(file: &JsValue) -> String {
    js_sys::Reflect::get(file, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

pub fn file_last_modified(file: &JsValue) -> f64 {
    js_sys::Reflect::get(file, &JsValue::from_str("lastModified"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Отформатировать Unix-время (мс) как `YYYY-MM-DD HH:MM`.
pub fn format_timestamp_ms(ms: f64) -> String {
    if ms <= 0.0 {
        let s: String = "—".to_string();
        return s;
    }
    let date = js_sys::Date::new(&JsValue::from_f64(ms));
    let y = date.get_full_year();
    let m = date.get_month() + 1;
    let d = date.get_date();
    let hh = date.get_hours();
    let mm = date.get_minutes();
    let s: String = format_str(format_args!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y, m, d, hh, mm
    ));
    s
}

// ===========================================================================
// Hex-dump и CRC32
// ===========================================================================

/// Классический hex-dump: смещение, 16 байт в hex, ASCII.
///
/// Если `max_bytes < bytes.len()` — вывод обрезается, внизу
/// приписывается «… ещё N байт скрыто».
pub fn hex_dump(bytes: &[u8], max_bytes: usize) -> String {
    let take: usize = bytes.len().min(max_bytes);
    let mut out: String = String::new();
    let mut i: usize = 0;
    while i < take {
        let line_end: usize = (i + 16).min(take);

        // Offset.
        let offset: String = format_str(format_args!("{:08x}  ", i));
        out.push_str(&offset);

        // Hex.
        for j in 0..16 {
            let idx: usize = i + j;
            if idx < line_end {
                let h: String = format_str(format_args!("{:02x} ", bytes[idx]));
                out.push_str(&h);
            } else {
                out.push_str("   ");
            }
            if j == 7 {
                out.push(' ');
            }
        }
        out.push_str(" |");

        // ASCII.
        for j in 0..16 {
            let idx: usize = i + j;
            if idx < line_end {
                let b: u8 = bytes[idx];
                let c: char = if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                };
                out.push(c);
            } else {
                out.push(' ');
            }
        }
        out.push_str("|\n");

        i = line_end;
    }
    if take < bytes.len() {
        let tail: String =
            format_str(format_args!("… ещё {} байт скрыто\n", bytes.len() - take));
        out.push_str(&tail);
    }
    out
}

/// CRC32 по стандарту IEEE (полином 0xEDB88320).
///
/// Тот же CRC, что используется в gzip, zlib, PNG. Возвращает
/// `u32` — для отображения в hex.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask: u32 = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}