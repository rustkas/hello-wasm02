//! 49. File Converter: браузерные File API.
//!
//! Обёртки над `File`, `Blob`, `URL.createObjectURL`, `<input type=file>`.
//! Асинхронное чтение чанками с отменой и прогрессом.
//!
//! ## Что НЕ делает этот модуль
//!
//! Он не пересекается с `converter_ui.rs` (image-пайплайн): тот
//! работает через FSA (`showSaveFilePicker`), этот — через
//! `<a download>`. Разные сценарии.

use std::cell::Cell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use js_sys::Uint8Array;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, BlobPropertyBag, Document, File, HtmlAnchorElement, Url};

use crate::video_frame::format_str;

// =============================================================================
// CancellationToken
// =============================================================================

/// Маркер отмены. Клонируется как `Rc` — общий флаг между UI и
/// async-задачей.
#[derive(Clone)]
pub struct CancellationToken {
    flag: Rc<Cell<bool>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        return Self {
            flag: Rc::new(Cell::new(false)),
        };
    }

    pub fn cancel(&self) {
        self.flag.set(true);
    }

    pub fn is_cancelled(&self) -> bool {
        return self.flag.get();
    }
}

// =============================================================================
// Чтение файла
// =============================================================================

pub const CHUNK_SIZE: u64 = 64 * 1024;

/// Читает `File` по кускам, вызывая `on_progress(consumed, total)`
/// после каждого. Проверяет `token` между чанками.
///
/// ## Почему `slice` + `arrayBuffer`
///
/// `FileReader.readAsArrayBuffer` читает всё за раз — не даёт
/// промежуточного прогресса и точки отмены. `file.slice(a, b)`
/// возвращает `Blob` с методом `arrayBuffer()`. Разбивая файл
/// на куски по 64 KB, получаем прогресс и возможность прервать.
///
/// ## Сигнатуры web-sys 0.3.105
///
/// - `File::slice_with_f64_and_f64(start, end)` → `Result<Blob, JsValue>`
/// - `Blob::array_buffer()` → `Promise` (не `Result<Promise, _>`)
///
/// Поэтому у `slice` есть `.map_err`, а у `array_buffer` — нет.
///
/// ## Yield
///
/// После каждого чанка — `TimeoutFuture::new(0).await`. Без этого
/// UI не перерисуется до конца операции.
pub async fn read_file_chunked<F>(
    file: &File,
    token: &CancellationToken,
    mut on_progress: F,
) -> Result<Vec<u8>, String>
where
    F: FnMut(u64, u64),
{
    let total = file.size() as u64;
    let mut offset: u64 = 0;
    let mut out: Vec<u8> = Vec::with_capacity(total as usize);

    while offset < total {
        if token.is_cancelled() {
            return Err(String::from("Отменено пользователем"));
        }

        let end = (offset + CHUNK_SIZE).min(total);
        let slice = file
            .slice_with_f64_and_f64(offset as f64, end as f64)
            .map_err(|e| format_str(format_args!("slice: {e:?}")))?;

        // `array_buffer()` возвращает `Promise` напрямую.
        let ab_promise: js_sys::Promise = slice.array_buffer();
        let ab_value: wasm_bindgen::JsValue = JsFuture::from(ab_promise)
            .await
            .map_err(|e| format_str(format_args!("read: {e:?}")))?;

        let ab: js_sys::ArrayBuffer = ab_value
            .dyn_into()
            .map_err(|_| String::from("не ArrayBuffer"))?;
        let u8arr = Uint8Array::new(&ab);
        let mut chunk = vec![0u8; u8arr.length() as usize];
        u8arr.copy_to(&mut chunk);
        out.extend_from_slice(&chunk);

        offset = end;
        on_progress(offset, total);

        TimeoutFuture::new(0).await;
    }

    return Ok(out);
}

// =============================================================================
// Blob / download
// =============================================================================

pub fn make_blob(bytes: &[u8], mime: &str) -> Result<Blob, String> {
    let parts = js_sys::Array::new();
    parts.push(&Uint8Array::from(bytes));
    let opts = BlobPropertyBag::new();
    opts.set_type(mime);
    return Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|e| format_str(format_args!("Blob: {e:?}")));
}

pub fn create_download_url(blob: &Blob) -> Result<String, String> {
    return Url::create_object_url_with_blob(blob)
        .map_err(|e| format_str(format_args!("createObjectURL: {e:?}")));
}

/// Освобождает object URL. Идемпотентно.
pub fn revoke_download_url(url: &str) {
    let _ = Url::revoke_object_url(url);
}

/// Программно инициирует скачивание через `<a download href>`.
///
/// ## Почему не `location.href`
///
/// `location.href = url` откроет бинарный файл в браузере, а не
/// скачает. Атрибут `download` на `<a>` заставляет браузер
/// сохранить файл с заданным именем.
pub fn trigger_download(
    document: &Document,
    url: &str,
    filename: &str,
) -> Result<(), String> {
    let a = document
        .create_element("a")
        .map_err(|e| format_str(format_args!("createElement: {e:?}")))?;
    let a: HtmlAnchorElement = a
        .dyn_into()
        .map_err(|_| String::from("не <a>"))?;
    a.set_href(url);
    a.set_download(filename);
    a.click();
    return Ok(());
}

// =============================================================================
// File picker
// =============================================================================

pub fn pick_file(input: &web_sys::HtmlInputElement) -> Option<File> {
    let files = input.files()?;
    if files.length() == 0 {
        return None;
    }
    return files.item(0);
}

// =============================================================================
// Утилиты
// =============================================================================

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format_str(format_args!("{bytes} Б"));
    }
    if bytes < 1024 * 1024 {
        return format_str(format_args!("{:.1} КБ", bytes as f64 / 1024.0));
    }
    return format_str(format_args!("{:.2} МБ", bytes as f64 / (1024.0 * 1024.0)));
}

pub fn replace_extension(filename: &str, new_ext: &str) -> String {
    let stem = match filename.rfind('.') {
        Some(i) => &filename[..i],
        None => filename,
    };
    return format_str(format_args!("{stem}.{new_ext}"));
}