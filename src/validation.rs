//! Валидация файлов: MIME, magic-байты, размер.
//!
//! Модуль автономен: не зависит от других модулей проекта.
//!
//! Определяет настоящий формат файла по первым байтам (сигнатуре),
//! а не по расширению или заявленному MIME. Это защищает от
//! подделок: `malware.txt`, внутри которого PNG или ZIP.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FileReader};

/// Формат файла, определённый по сигнатуре.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
    Ico,
    Tiff,
    Pdf,
    Zip,
    Gzip,
    Rar,
    SevenZ,
    Tar,
    Mp3,
    Wav,
    Ogg,
    Flac,
    Mp4,
    Webm,
    Wasm,
    Text,
    Unknown,
}

impl Format {
    /// Человекочитаемое имя формата.
    pub fn name(&self) -> &'static str {
        return match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Gif => "GIF",
            Format::Webp => "WebP",
            Format::Bmp => "BMP",
            Format::Ico => "ICO",
            Format::Tiff => "TIFF",
            Format::Pdf => "PDF",
            Format::Zip => "ZIP",
            Format::Gzip => "GZIP",
            Format::Rar => "RAR",
            Format::SevenZ => "7z",
            Format::Tar => "TAR",
            Format::Mp3 => "MP3",
            Format::Wav => "WAV",
            Format::Ogg => "OGG",
            Format::Flac => "FLAC",
            Format::Mp4 => "MP4",
            Format::Webm => "WebM",
            Format::Wasm => "WebAssembly",
            Format::Text => "Текст (ASCII)",
            Format::Unknown => "Неизвестный",
        };
    }

    /// MIME-тип, соответствующий формату.
    pub fn mime_type(&self) -> &'static str {
        return match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Gif => "image/gif",
            Format::Webp => "image/webp",
            Format::Bmp => "image/bmp",
            Format::Ico => "image/x-icon",
            Format::Tiff => "image/tiff",
            Format::Pdf => "application/pdf",
            Format::Zip => "application/zip",
            Format::Gzip => "application/gzip",
            Format::Rar => "application/vnd.rar",
            Format::SevenZ => "application/x-7z-compressed",
            Format::Tar => "application/x-tar",
            Format::Mp3 => "audio/mpeg",
            Format::Wav => "audio/wav",
            Format::Ogg => "audio/ogg",
            Format::Flac => "audio/flac",
            Format::Mp4 => "video/mp4",
            Format::Webm => "video/webm",
            Format::Wasm => "application/wasm",
            Format::Text => "text/plain",
            Format::Unknown => "application/octet-stream",
        };
    }

    /// Является ли формат изображением.
    ///
    /// `matches!` — макрос; в tail-позиции функции возвращает `()`.
    /// Присваиваем в предобъявленную переменную и возвращаем через
    /// явный `return` — безопасный паттерн для окружения проекта.
    pub fn is_image(&self) -> bool {
        let result: bool = matches!(
            self,
            Format::Png
                | Format::Jpeg
                | Format::Gif
                | Format::Webp
                | Format::Bmp
                | Format::Ico
                | Format::Tiff
        );
        return result;
    }
}

/// Определяет формат по первым байтам (magic bytes).
///
/// Если данных меньше, чем нужно для проверки конкретной сигнатуры,
/// эта сигнатура просто не срабатывает — проверяются остальные.
/// Для пустого входа возвращает `Format::Unknown`.
pub fn detect_format(bytes: &[u8]) -> Format {
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Format::Png;
    }
    // JPEG: FF D8 FF
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Format::Jpeg;
    }
    // GIF: "GIF8"
    if bytes.starts_with(b"GIF8") {
        return Format::Gif;
    }
    // WebP: RIFF .... WEBP
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Format::Webp;
    }
    // WAV: RIFF .... WAVE
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Format::Wav;
    }
    // BMP: "BM"
    if bytes.starts_with(b"BM") {
        return Format::Bmp;
    }
    // ICO: 00 00 01 00
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Format::Ico;
    }
    // TIFF: "II*\0" (little-endian) или "MM\0*" (big-endian)
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return Format::Tiff;
    }
    // PDF: "%PDF"
    if bytes.starts_with(b"%PDF") {
        return Format::Pdf;
    }
    // ZIP: PK\x03\x04
    if bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return Format::Zip;
    }
    // GZIP: 1F 8B
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return Format::Gzip;
    }
    // RAR: "Rar!\x1A\x07"
    if bytes.starts_with(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07]) {
        return Format::Rar;
    }
    // 7z: 37 7A BC AF 27 1C
    if bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Format::SevenZ;
    }
    // TAR: "ustar" по смещению 257
    if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        return Format::Tar;
    }
    // WebAssembly: 00 61 73 6D
    if bytes.starts_with(&[0x00, 0x61, 0x73, 0x6D]) {
        return Format::Wasm;
    }
    // OGG: "OggS"
    if bytes.starts_with(b"OggS") {
        return Format::Ogg;
    }
    // FLAC: "fLaC"
    if bytes.starts_with(b"fLaC") {
        return Format::Flac;
    }
    // MP3: "ID3" или frame sync (старшие 11 бит первого байта == 1)
    if bytes.starts_with(b"ID3") {
        return Format::Mp3;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return Format::Mp3;
    }
    // MP4 / MOV: bytes[4..8] == "ftyp"
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return Format::Mp4;
    }
    // WebM / MKV: EBML 1A 45 DF A3
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Format::Webm;
    }
    // Текстовый эвристический анализ: только печатаемые ASCII.
    if !bytes.is_empty() && is_mostly_text(bytes) {
        return Format::Text;
    }
    return Format::Unknown;
}

/// Проверяет, что байты выглядят как ASCII-текст.
///
/// Считаем текстом, если не менее 90 % байт — печатаемые символы
/// или пробельные (`\n`, `\r`, `\t`, пробел). Пустой срез не
/// считается текстом — возвращаем `false`.
fn is_mostly_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut printable = 0usize;
    for &b in bytes {
        if b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
            printable += 1;
        }
    }
    return printable * 100 / bytes.len() >= 90;
}

/// Результат проверки размера файла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeCheck {
    /// Фактический размер файла в байтах.
    pub actual: f64,
    /// Лимит в байтах; `0.0` означает «без лимита».
    pub limit: f64,
    /// Проходит ли файл по лимиту.
    pub ok: bool,
}

/// Проверяет размер файла против лимита.
///
/// `limit_bytes <= 0.0` означает «без лимита» — всегда `ok = true`.
pub fn check_size(size: f64, limit_bytes: f64) -> SizeCheck {
    let ok: bool;
    if limit_bytes <= 0.0 {
        ok = true;
    } else {
        ok = size <= limit_bytes;
    }
    return SizeCheck {
        actual: size,
        limit: limit_bytes,
        ok,
    };
}

/// Читает первые `n` байт из Blob.
///
/// Использует `Blob::slice` — это позволяет прочитать заголовок
/// большого файла без загрузки всего содержимого в память.
/// Если файл короче `n`, возвращается всё содержимое.
pub async fn read_header(blob: &Blob, n: u32) -> Result<Vec<u8>, JsValue> {
    let slice = blob.slice_with_f64_and_f64(0.0, n as f64)?;
    let reader = FileReader::new()?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let reader_clone = reader.clone();
        let onload = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let onerror = Closure::once_into_js(move |err: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &err);
        });
        reader_clone.set_onload(Some(onload.unchecked_ref()));
        reader_clone.set_onerror(Some(onerror.unchecked_ref()));
    });

    reader.read_as_array_buffer(&slice)?;
    JsFuture::from(promise).await?;

    let result = reader.result()?;
    let array_buffer: js_sys::ArrayBuffer = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let uint8 = js_sys::Uint8Array::new(&array_buffer);
    return Ok(uint8.to_vec());
}
