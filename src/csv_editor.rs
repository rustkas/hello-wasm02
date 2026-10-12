//! Локальный CSV-редактор с автосохранением в OPFS.
//!
//! **Что демонстрирует.** Полный workflow локального
//! файлового инструмента: открыть → распарсить → редактировать
//! → автосохранить (с дебаунсом) → проверить хеш → экспортировать.
//!
//! **Ключевые идеи.**
//! 1. **CSV parsing** — RFC 4180 в минимуме: кавычки, экранирование,
//!    переводы строк внутри ячеек. Без внешних крейтов.
//! 2. **SHA-256 хеш содержимого** — «отпечаток» состояния.
//!    По нему видно, есть ли несохранённые изменения.
//! 3. **Autosave с дебаунсом** — сохранение не на каждое нажатие,
//!    а через N миллисекунд бездействия.
//! 4. **Автономность** — модуль не зависит от других глав. Всё
//!    взаимодействие с ФС — через `js_sys::Reflect`, чтобы
//!    работать независимо от версии `web-sys`.
//!
//! **Правило WASM-инференса.** `format!` в tail-позиции `if/else`/
//! `match`/блока иногда выводится как `()` → E0308. Все такие
//! места обёрнуты в `format_str(format_args!(…))`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

// ===========================================================================
// Строковые хелперы
// ===========================================================================

#[inline]
pub fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

// ===========================================================================
// CSV parser / serializer (чистый Rust, host-тестируемый)
// ===========================================================================

/// Разобрать CSV-строку в матрицу ячеек.
///
/// Поддерживает:
/// * поля, разделённые запятыми;
/// * строки, разделённые `\n` или `\r\n`;
/// * кавычки `"..."` вокруг поля;
/// * экранирование `""` внутри поля-в-кавычках;
/// * переносы строк внутри кавычек (не разрывают поле).
pub fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    row.push(field.clone());
                    field.clear();
                }
                '\r' => {
                    // Игнорируем — обработаем `\n`.
                }
                '\n' => {
                    row.push(field.clone());
                    field.clear();
                    rows.push(row.clone());
                    row.clear();
                }
                _ => field.push(c),
            }
        }
    }
    // «Хвост» — последняя строка без завершающего \n.
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Сериализовать матрицу в CSV-строку.
///
/// Ячейка оборачивается в кавычки, если содержит `,`, `"`, `\n`
/// или `\r`. Внутренние кавычки удваиваются.
pub fn serialize_csv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let mut first = true;
        for cell in row {
            if !first {
                out.push(',');
            }
            first = false;
            let needs_quotes = cell.contains(',')
                || cell.contains('"')
                || cell.contains('\n')
                || cell.contains('\r');
            if needs_quotes {
                out.push('"');
                for c in cell.chars() {
                    if c == '"' {
                        out.push('"');
                    }
                    out.push(c);
                }
                out.push('"');
            } else {
                out.push_str(cell);
            }
        }
        out.push('\n');
    }
    out
}

// ===========================================================================
// SHA-256 (хеш содержимого)
// ===========================================================================

/// SHA-256 в hex. Использует крейт `sha2` + `hex`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    hex::encode(out)
}

/// Короткая версия хеша — первые 12 символов. Для UI.
pub fn short_hash(full: &str) -> String {
    if full.len() <= 12 {
        return full.to_string();
    }
    let s: String = full.chars().take(12).collect();
    s
}

// ===========================================================================
// OPFS: чтение и запись через Reflect
// ===========================================================================

/// Получить корень OPFS.
pub async fn opfs_root() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let nav_js = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))?;

    let storage = js_sys::Reflect::get(&nav_js, &JsValue::from_str("storage"))?;
    if storage.is_undefined() {
        return Err(JsValue::from_str("navigator.storage недоступен"));
    }

    let get_dir = js_sys::Reflect::get(&storage, &JsValue::from_str("getDirectory"))?;
    if !get_dir.is_function() {
        return Err(JsValue::from_str("getDirectory не функция"));
    }
    let f: js_sys::Function = get_dir
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Function"))?;
    let p: js_sys::Promise = f
        .call0(&storage)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    JsFuture::from(p).await
}

/// Получить handle файла в директории.
/// `create = true` — идемпотентно создаст при отсутствии.
pub async fn file_handle_in(
    dir: &JsValue,
    name: &str,
    create: bool,
) -> Result<JsValue, JsValue> {
    let method = js_sys::Reflect::get(dir, &JsValue::from_str("getFileHandle"))?;
    if !method.is_function() {
        return Err(JsValue::from_str("getFileHandle не функция"));
    }
    let f: js_sys::Function = method
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Function"))?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("create"),
        &JsValue::from_bool(create),
    )?;

    let p_js = f.call2(dir, &JsValue::from_str(name), &options)?;
    let p: js_sys::Promise = p_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    JsFuture::from(p).await
}

/// Прочитать содержимое файла через `File.arrayBuffer()`.
pub async fn read_file_bytes(handle: &JsValue) -> Result<Vec<u8>, JsValue> {
    let get_file = js_sys::Reflect::get(handle, &JsValue::from_str("getFile"))?;
    let f: js_sys::Function = get_file
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFile не функция"))?;
    let p: js_sys::Promise = f
        .call0(handle)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let file = JsFuture::from(p).await?;

    let ab_method = js_sys::Reflect::get(&file, &JsValue::from_str("arrayBuffer"))?;
    let ab_fn: js_sys::Function = ab_method
        .dyn_into()
        .map_err(|_| JsValue::from_str("arrayBuffer не функция"))?;
    let ab_p: js_sys::Promise = ab_fn
        .call0(&file)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let ab_value = JsFuture::from(ab_p).await?;
    let ab: js_sys::ArrayBuffer = ab_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let u8arr = js_sys::Uint8Array::new(&ab);
    let len = u8arr.length() as usize;
    let mut out: Vec<u8> = vec![0u8; len];
    u8arr.copy_to(&mut out);
    Ok(out)
}

/// Записать байты в файл через `createWritable`.
///
/// `createWritable()` возвращает поток; пишем в него и закрываем.
/// `close()` — это фактический commit: до него данные не на диске.
pub async fn write_file_bytes(
    handle: &JsValue,
    data: &[u8],
) -> Result<(), JsValue> {
    let cw = js_sys::Reflect::get(handle, &JsValue::from_str("createWritable"))?;
    let cw_fn: js_sys::Function = cw
        .dyn_into()
        .map_err(|_| JsValue::from_str("createWritable не функция"))?;
    let p: js_sys::Promise = cw_fn
        .call0(handle)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let writable = JsFuture::from(p).await?;

    // writable.write(u8arr)
    let write = js_sys::Reflect::get(&writable, &JsValue::from_str("write"))?;
    let write_fn: js_sys::Function = write
        .dyn_into()
        .map_err(|_| JsValue::from_str("write не функция"))?;
    let u8arr = js_sys::Uint8Array::from(data);
    let w_p: js_sys::Promise = write_fn
        .call1(&writable, &u8arr)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("write не Promise"))?;
    JsFuture::from(w_p).await?;

    // writable.close()
    let close = js_sys::Reflect::get(&writable, &JsValue::from_str("close"))?;
    let close_fn: js_sys::Function = close
        .dyn_into()
        .map_err(|_| JsValue::from_str("close не функция"))?;
    let c_p: js_sys::Promise = close_fn
        .call0(&writable)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("close не Promise"))?;
    JsFuture::from(c_p).await?;
    Ok(())
}

// ===========================================================================
// Модель документа
// ===========================================================================

/// Состояние CSV-документа в памяти.
#[derive(Clone)]
pub struct CsvDoc {
    pub rows: Vec<Vec<String>>,
    /// Хеш содержимого, который **последний раз успешно записан**.
    pub saved_hash: String,
}

impl CsvDoc {
    /// Хеш текущего содержимого — по нему сравниваем с saved_hash.
    pub fn current_hash(&self) -> String {
        let text = serialize_csv(&self.rows);
        sha256_hex(text.as_bytes())
    }

    /// Есть ли несохранённые изменения.
    pub fn is_dirty(&self) -> bool {
        let cur = self.current_hash();
        cur != self.saved_hash
    }

    /// Пометить, что текущее состояние успешно записано.
    pub fn mark_saved(&mut self) {
        self.saved_hash = self.current_hash();
    }

    /// Размер в байтах (CSV-текст).
    pub fn byte_size(&self) -> usize {
        let text = serialize_csv(&self.rows);
        text.len()
    }
}

/// Сгенерировать seed-документ с примером данных.
pub fn seed_doc() -> CsvDoc {
    let rows: Vec<Vec<String>> = vec![
        vec!["name".into(), "email".into(), "age".into()],
        vec!["Alice".into(), "alice@example.com".into(), "30".into()],
        vec!["Bob".into(), "bob@example.com".into(), "25".into()],
        vec!["Carol".into(), "carol@example.com".into(), "35".into()],
        vec!["Dave".into(), "dave@example.com".into(), "28".into()],
    ];
    let text = serialize_csv(&rows);
    let h = sha256_hex(text.as_bytes());
    CsvDoc {
        rows,
        saved_hash: h,
    }
}