//! Модель чтения файла: что именно мы собираемся прочитать и почему.
//!
//! Модуль не читает файл (это делает `fsa_read_ops`), а только:
//! * выбирает план под размер и MIME;
//! * проверяет план против лимитов;
//! * определяет формат по magic-байтам;
//! * объясняет выбор и ошибки пользователю.

use crate::fsa_choose_options::format_str;

/// Лимиты чтения. Отдельная структура — их можно варьировать (в тестах —
/// уменьшать, чтобы не гонять гигабайты).
#[derive(Debug, Clone, Copy)]
pub struct ReadLimits {
    /// Больше этого — `arrayBuffer()` не применяем: держим буфер в памяти
    /// ограниченного размера. Реальные приложения ставят 50–100 МБ.
    pub max_buffer_bytes: u64,
    /// Больше этого — `text()` не применяем: декодирование в строку
    /// дороже по памяти, чем в байтовый буфер.
    pub max_text_bytes: u64,
    /// Сколько байт показывать в `peek` по умолчанию.
    pub peek_default: u64,
}

impl ReadLimits {
    pub fn demo() -> Self {
        return ReadLimits {
            max_buffer_bytes: 100 * 1024 * 1024, // 100 МБ
            max_text_bytes: 10 * 1024 * 1024,    // 10 МБ
            peek_default: 8,
        };
    }
}

/// Что именно делаем с файлом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPlan {
    Text,
    Bytes,
    Slice { start: u64, end: u64 },
    Stream,
}

impl ReadPlan {
    pub fn name(&self) -> &'static str {
        match self {
            ReadPlan::Text => return "Text",
            ReadPlan::Bytes => return "Bytes",
            ReadPlan::Slice { .. } => return "Slice",
            ReadPlan::Stream => return "Stream",
        }
    }
}

/// Ошибка чтения — осмысленный результат для UI.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadError {
    TooLarge {
        size: u64,
        limit: u64,
        plan_name: &'static str,
    },
    NotApplicable {
        plan_name: &'static str,
        reason: String,
    },
    Cancelled,
    Io(String),
}

impl ReadError {
    pub fn is_cancelled(&self) -> bool {
        return matches!(self, ReadError::Cancelled);
    }
}

// ---------------------------------------------------------------------------
// Определение формата по magic-байтам
// ---------------------------------------------------------------------------

/// Определение формата по первым байтам.
///
/// Возвращает человекочитаемое название или `None`, если формат не
/// распознан. Не претендует на полноту — только частые форматы.
pub fn detect_magic_format(bytes: &[u8]) -> Option<&'static str> {
    // PNG: 89 50 4E 47
    if bytes.len() >= 4 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some("PNG (изображение)");
    }
    // JPEG: FF D8 FF
    if bytes.len() >= 3 && bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("JPEG (изображение)");
    }
    // GIF: GIF8
    if bytes.len() >= 4 && bytes.starts_with(b"GIF8") {
        return Some("GIF (изображение)");
    }
    // PDF: %PDF
    if bytes.len() >= 4 && bytes.starts_with(b"%PDF") {
        return Some("PDF (документ)");
    }
    // ZIP: 50 4B 03 04
    if bytes.len() >= 4 && bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return Some("ZIP (архив)");
    }
    // WEBP / WAV — оба RIFF
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" {
        if &bytes[8..12] == b"WEBP" {
            return Some("WEBP (изображение)");
        }
        if &bytes[8..12] == b"WAVE" {
            return Some("WAV (аудио)");
        }
    }
    // ELF: 7F 45 4C 46
    if bytes.len() >= 4 && bytes.starts_with(&[0x7F, 0x45, 0x4C, 0x46]) {
        return Some("ELF (исполняемый)");
    }
    // OGG: OggS
    if bytes.len() >= 4 && bytes.starts_with(b"OggS") {
        return Some("OGG (аудио/видео)");
    }
    // FLAC: fLaC
    if bytes.len() >= 4 && bytes.starts_with(b"fLaC") {
        return Some("FLAC (аудио)");
    }
    // GZIP: 1F 8B
    if bytes.len() >= 2 && bytes.starts_with(&[0x1F, 0x8B]) {
        return Some("GZIP (архив)");
    }
    // BZIP2: BZh
    if bytes.len() >= 3 && bytes.starts_with(b"BZh") {
        return Some("BZIP2 (архив)");
    }
    // 7z: 37 7A BC AF 27 1C
    if bytes.len() >= 6 && bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some("7-Zip (архив)");
    }
    // MP4: ?? ?? ?? ?? ftyp
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return Some("MP4 (видео)");
    }
    // MP3 с ID3-тегом
    if bytes.len() >= 3 && bytes.starts_with(b"ID3") {
        return Some("MP3 (аудио)");
    }
    // MP3 без ID3 / AAC: FF Ex / FF Fx
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return Some("MP3 / AAC (аудио)");
    }
    return None;
}

/// Строка «hexdump первых N байт» — только одна линия.
pub fn format_magic_line(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "(нет данных)".to_string();
    }
    let mut parts: Vec<String> = Vec::with_capacity(bytes.len());
    for b in bytes.iter().take(8) {
        parts.push(format_str(format_args!("{:02X}", b)));
    }
    return parts.join(" ");
}

// ---------------------------------------------------------------------------
// Выбор и проверка плана
// ---------------------------------------------------------------------------

/// Выбор разумного плана по размеру и MIME.
pub fn suggest_plan(size: u64, mime: &str, limits: &ReadLimits) -> ReadPlan {
    if size == 0 {
        return ReadPlan::Bytes;
    }
    if mime.starts_with("text/") && size <= limits.max_text_bytes {
        return ReadPlan::Text;
    }
    if size <= limits.max_buffer_bytes {
        return ReadPlan::Bytes;
    }
    return ReadPlan::Stream;
}

/// Проверка, применим ли план к файлу такого размера и MIME.
pub fn validate_plan(
    plan: &ReadPlan,
    size: u64,
    mime: &str,
    limits: &ReadLimits,
) -> Result<(), ReadError> {
    match plan {
        ReadPlan::Text => {
            if size > limits.max_text_bytes {
                return Err(ReadError::TooLarge {
                    size,
                    limit: limits.max_text_bytes,
                    plan_name: "Text",
                });
            }
            if !mime.is_empty() && !mime.starts_with("text/") && !mime.contains("json") {
                let reason: String = format_str(format_args!(
                    "MIME '{mime}' не текстовый — text() даст кракозябры. \
                     Используйте Bytes или Slice.",
                ));
                return Err(ReadError::NotApplicable {
                    plan_name: "Text",
                    reason,
                });
            }
        }
        ReadPlan::Bytes => {
            if size > limits.max_buffer_bytes {
                return Err(ReadError::TooLarge {
                    size,
                    limit: limits.max_buffer_bytes,
                    plan_name: "Bytes",
                });
            }
        }
        ReadPlan::Slice { start, end } => {
            if end < start {
                return Err(ReadError::Io(format_str(format_args!(
                    "Slice: end ({end}) < start ({start})",
                ))));
            }
            if *end > size {
                return Err(ReadError::Io(format_str(format_args!(
                    "Slice: end ({end}) > размера файла ({size})",
                ))));
            }
        }
        ReadPlan::Stream => {}
    }
    return Ok(());
}

// ---------------------------------------------------------------------------
// Форматтеры для UI
// ---------------------------------------------------------------------------

/// Человекочитаемый размер файла.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format_str(format_args!("{bytes} Б"));
    }
    if bytes < 1024 * 1024 {
        let kb: f64 = bytes as f64 / 1024.0;
        return format_str(format_args!("{kb:.1} КБ"));
    }
    if bytes < 1024 * 1024 * 1024 {
        let mb: f64 = bytes as f64 / (1024.0 * 1024.0);
        return format_str(format_args!("{mb:.2} МБ"));
    }
    let gb: f64 = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    return format_str(format_args!("{gb:.2} ГБ"));
}

/// Строка «почему этот план» для UI.
pub fn explain_plan(plan: &ReadPlan, size: u64, mime: &str) -> String {
    let mime_display: &str = if mime.is_empty() { "—" } else { mime };
    match plan {
        ReadPlan::Text => {
            let s: String = format_str(format_args!(
                "Text: файл {}, MIME {mime_display}. Тип текстовый и размер в \
                 пределах лимита — декодируем как UTF-8.",
                format_size(size),
            ));
            return s;
        }
        ReadPlan::Bytes => {
            let s: String = format_str(format_args!(
                "Bytes: файл {}. Умещается в буфер памяти — читаем целиком \
                 через arrayBuffer().",
                format_size(size),
            ));
            return s;
        }
        ReadPlan::Slice { start, end } => {
            let s: String = format_str(format_args!(
                "Slice {start}..{end}: читаем только диапазон — не весь файл.",
            ));
            return s;
        }
        ReadPlan::Stream => {
            let s: String = format_str(format_args!(
                "Stream: файл {}. Больше лимита Bytes — читаем чанками, \
                 не грузим в память.",
                format_size(size),
            ));
            return s;
        }
    }
}

/// Объяснение ошибки для UI.
pub fn explain_error(err: &ReadError) -> String {
    match err {
        ReadError::TooLarge {
            size,
            limit,
            plan_name,
        } => {
            let s: String = format_str(format_args!(
                "✗ Файл {} превышает лимит {} для плана {}.\n\
                 Используйте Stream — он читает чанками и не держит всё в памяти.",
                format_size(*size),
                format_size(*limit),
                plan_name,
            ));
            return s;
        }
        ReadError::NotApplicable { plan_name, reason } => {
            let s: String = format_str(format_args!(
                "✗ План {plan_name} неприменим: {reason}",
            ));
            return s;
        }
        ReadError::Cancelled => {
            return "Отменено пользователем.".to_string();
        }
        ReadError::Io(msg) => {
            let s: String = format_str(format_args!("✗ Ошибка чтения: {msg}"));
            return s;
        }
    }
}