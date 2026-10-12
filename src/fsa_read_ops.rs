//! Реализация чтения: peek, text, bytes, slice, stream.
//!
//! Плюс три инкрементальные задачи на потоке:
//! * `hash_stream` — SHA-256 без загрузки в память;
//! * `count_lines_stream` — подсчёт строк;
//! * `find_marker_stream` — поиск подстроки с обработкой границ чанков.

use gloo_timers::future::TimeoutFuture;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, ReadableStreamDefaultReader};

use crate::fsa_choose_options::format_str;
use crate::fsa_read_plan::{
    format_size, validate_plan, ReadError, ReadLimits, ReadPlan,
};

// ---------------------------------------------------------------------------
// Статистика
// ---------------------------------------------------------------------------

/// Статистика потокового чтения.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamStats {
    pub bytes_read: u64,
    pub chunks: u32,
    pub yields: u32,
    pub elapsed_ms: f64,
    pub cancelled: bool,
    /// Размеры первых до 8 чанков. Показывают, что размеры не фиксированы.
    pub first_chunk_sizes: Vec<usize>,
}

impl StreamStats {
    pub fn matches_size(&self, expected: u64) -> bool {
        return !self.cancelled && self.bytes_read == expected;
    }
    pub fn avg_chunk_bytes(&self) -> u64 {
        if self.chunks == 0 {
            return 0;
        }
        return self.bytes_read / self.chunks as u64;
    }
    pub fn throughput_mbps(&self) -> f64 {
        if self.elapsed_ms <= 0.0 {
            return 0.0;
        }
        let mb = self.bytes_read as f64 / (1024.0 * 1024.0);
        let seconds = self.elapsed_ms / 1000.0;
        return mb / seconds;
    }
}

// ---------------------------------------------------------------------------
// Peek
// ---------------------------------------------------------------------------

/// Прочитать первые `n` байт файла, не загружая остальное.
pub async fn peek_bytes(file: &File, n: u64) -> Result<Vec<u8>, ReadError> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let end = (file.size() as u64).min(n) as f64;
    let slice = file.slice_with_f64_and_f64(0.0, end).map_err(|e| {
        ReadError::Io(format_str(format_args!("slice: {e:?}")))
    })?;
    let promise: js_sys::Promise = slice.array_buffer();
    let value = JsFuture::from(promise).await.map_err(|e| {
        ReadError::Io(format_str(format_args!("array_buffer: {e:?}")))
    })?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| ReadError::Io("не ArrayBuffer".to_string()))?;
    return Ok(Uint8Array::new(&buffer).to_vec());
}

// ---------------------------------------------------------------------------
// Hexdump
// ---------------------------------------------------------------------------

fn format_hex_line(bytes: &[u8]) -> String {
    let mut hex: String = String::with_capacity(16 * 3);
    let mut ascii: String = String::with_capacity(16);
    for i in 0..16 {
        if i < bytes.len() {
            let b: u8 = bytes[i];
            hex.push_str(&format_str(format_args!("{:02X} ", b)));
            let ch: char = if (0x20..0x7f).contains(&b) { b as char } else { '.' };
            ascii.push(ch);
        } else {
            hex.push_str("   ");
            ascii.push(' ');
        }
    }
    return format_str(format_args!("{hex}| {ascii}"));
}

/// Полный дамп: строка на 16 байт с оффсетом.
pub fn format_hexdump(bytes: &[u8], base_offset: u64) -> String {
    if bytes.is_empty() {
        return "(нет данных)".to_string();
    }
    let mut out: String = String::with_capacity(bytes.len() * 4);
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let offset: u64 = base_offset + (i * 16) as u64;
        let line: String = format_hex_line(chunk);
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format_str(format_args!("{offset:08X}: {line}")));
    }
    return out;
}

// ---------------------------------------------------------------------------
// Text / Bytes / Slice
// ---------------------------------------------------------------------------

pub async fn read_text_safe(
    file: &File,
    limits: &ReadLimits,
) -> Result<String, ReadError> {
    validate_plan(&ReadPlan::Text, file.size() as u64, &file.type_(), limits)?;
    let promise: js_sys::Promise = file.text();
    let value = JsFuture::from(promise).await.map_err(|e| {
        ReadError::Io(format_str(format_args!("text: {e:?}")))
    })?;
    return value
        .as_string()
        .ok_or_else(|| ReadError::Io("не строка".to_string()));
}

pub async fn read_bytes_safe(
    file: &File,
    limits: &ReadLimits,
) -> Result<Vec<u8>, ReadError> {
    validate_plan(&ReadPlan::Bytes, file.size() as u64, &file.type_(), limits)?;
    let promise: js_sys::Promise = file.array_buffer();
    let value = JsFuture::from(promise).await.map_err(|e| {
        ReadError::Io(format_str(format_args!("array_buffer: {e:?}")))
    })?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| ReadError::Io("не ArrayBuffer".to_string()))?;
    return Ok(Uint8Array::new(&buffer).to_vec());
}

pub async fn read_slice_safe(
    file: &File,
    start: u64,
    end: u64,
    limits: &ReadLimits,
) -> Result<Vec<u8>, ReadError> {
    let plan = ReadPlan::Slice { start, end };
    validate_plan(&plan, file.size() as u64, &file.type_(), limits)?;
    let slice = file
        .slice_with_f64_and_f64(start as f64, end as f64)
        .map_err(|e| ReadError::Io(format_str(format_args!("slice: {e:?}"))))?;
    let promise: js_sys::Promise = slice.array_buffer();
    let value = JsFuture::from(promise).await.map_err(|e| {
        ReadError::Io(format_str(format_args!("array_buffer: {e:?}")))
    })?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| ReadError::Io("не ArrayBuffer".to_string()))?;
    return Ok(Uint8Array::new(&buffer).to_vec());
}

// ---------------------------------------------------------------------------
// Общий обход потока
// ---------------------------------------------------------------------------

/// Внутренний обход файла чанками.
///
/// `on_chunk(bytes, offset)` вызывается на каждом чанке; возврат `false`
/// прерывает чтение (например, при поиске найдено совпадение).
/// `max_bytes` — если задан, чтение остановится, когда прочитано ≥ max.
async fn stream_foreach<F>(
    file: &File,
    max_bytes: Option<u64>,
    mut on_chunk: F,
    mut on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<StreamStats, ReadError>
where
    F: FnMut(&[u8], u64) -> bool,
{
    let stream = file.stream();
    let reader = ReadableStreamDefaultReader::new(&stream)
        .map_err(|e| ReadError::Io(format_str(format_args!("reader: {e:?}"))))?;

    let total: u64 = file.size() as u64;
    let mut bytes_read: u64 = 0;
    let mut chunks: u32 = 0;
    let mut yields: u32 = 0;
    let mut last_yield: f64 = 0.0;
    let t0: f64 = js_sys::Date::now();
    let mut first_chunk_sizes: Vec<usize> = Vec::with_capacity(8);
    let mut early_exit: bool = false;

    loop {
        if is_cancelled() {
            let elapsed = js_sys::Date::now() - t0;
            return Ok(StreamStats {
                bytes_read,
                chunks,
                yields,
                elapsed_ms: elapsed,
                cancelled: true,
                first_chunk_sizes,
            });
        }

        if let Some(max) = max_bytes {
            if bytes_read >= max {
                early_exit = true;
                break;
            }
        }

        let result = JsFuture::from(reader.read()).await.map_err(|e| {
            ReadError::Io(format_str(format_args!("read: {e:?}")))
        })?;

        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|e| ReadError::Io(format_str(format_args!("value: {e:?}"))))?;
        let chunk = Uint8Array::new(&value);
        let bytes: Vec<u8> = chunk.to_vec();
        let len: u64 = bytes.len() as u64;

        if first_chunk_sizes.len() < 8 {
            first_chunk_sizes.push(bytes.len());
        }

        let keep_going = on_chunk(&bytes, bytes_read);
        bytes_read += len;
        chunks += 1;

        let now = js_sys::Date::now();
        if now - last_yield >= 16.0 {
            if total > 0 {
                on_progress(bytes_read as f64 / total as f64);
            }
            TimeoutFuture::new(0).await;
            last_yield = now;
            yields += 1;
        }

        if !keep_going {
            early_exit = true;
            break;
        }
    }

    if !early_exit {
        on_progress(1.0);
    }
    let elapsed = js_sys::Date::now() - t0;
    return Ok(StreamStats {
        bytes_read,
        chunks,
        yields,
        elapsed_ms: elapsed,
        cancelled: false,
        first_chunk_sizes,
    });
}

/// Потоковое чтение с прогрессом и отменой.
pub async fn read_stream_stats(
    file: &File,
    on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<StreamStats, ReadError> {
    return stream_foreach(file, None, |_, _| true, on_progress, is_cancelled).await;
}

// ---------------------------------------------------------------------------
// Инкрементальный SHA-256
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct HashStreamResult {
    pub hex: String,
    pub stats: StreamStats,
}

/// SHA-256 файла, посчитанный на потоке. Память O(1) — не зависит от размера.
pub async fn hash_stream(
    file: &File,
    on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<HashStreamResult, ReadError> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let stats = stream_foreach(
        file,
        None,
        |chunk, _offset| {
            hasher.update(chunk);
            return true;
        },
        on_progress,
        is_cancelled,
    )
    .await?;

    if stats.cancelled {
        return Err(ReadError::Cancelled);
    }

    let hex: String = hex::encode(hasher.finalize());
    return Ok(HashStreamResult { hex, stats });
}

// ---------------------------------------------------------------------------
// Инкрементальный подсчёт строк
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct LinesStreamResult {
    pub count: u64,
    pub stats: StreamStats,
}

/// Подсчёт строк: считаем `\n`; если файл не заканчивается на `\n`,
/// добавляем последнюю строку. Работает на потоке, память O(1).
pub async fn count_lines_stream(
    file: &File,
    on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<LinesStreamResult, ReadError> {
    let mut count: u64 = 0;
    let mut last_byte: Option<u8> = None;

    let stats = stream_foreach(
        file,
        None,
        |chunk, _offset| {
            for &b in chunk {
                if b == b'\n' {
                    count += 1;
                }
            }
            if let Some(&b) = chunk.last() {
                last_byte = Some(b);
            }
            return true;
        },
        on_progress,
        is_cancelled,
    )
    .await?;

    if !stats.cancelled {
        if let Some(b) = last_byte {
            if b != b'\n' {
                count += 1;
            }
        }
    }
    return Ok(LinesStreamResult { count, stats });
}

// ---------------------------------------------------------------------------
// Инкрементальный поиск маркера
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerStreamResult {
    pub found: bool,
    pub offset: Option<u64>,
    pub stats: StreamStats,
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    for i in 0..=last {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    return None;
}

/// Поиск подстроки в потоке. Обрабатывает границы чанков через tail.
/// Останавливается, как только маркер найден.
pub async fn find_marker_stream(
    file: &File,
    marker: &[u8],
    on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<MarkerStreamResult, ReadError> {
    if marker.is_empty() {
        return Err(ReadError::Io("пустой маркер".to_string()));
    }

    let marker_vec: Vec<u8> = marker.to_vec();
    let marker_len: usize = marker_vec.len();
    let keep_tail: usize = marker_len.saturating_sub(1);

    let mut tail: Vec<u8> = Vec::with_capacity(keep_tail);
    let mut found: Option<u64> = None;

    let stats = stream_foreach(
        file,
        None,
        |chunk, offset| {
            let mut combined: Vec<u8> = Vec::with_capacity(tail.len() + chunk.len());
            combined.extend_from_slice(&tail);
            combined.extend_from_slice(chunk);

            if let Some(pos) = find_subsequence(&combined, &marker_vec) {
                let base = offset.saturating_sub(tail.len() as u64);
                found = Some(base + pos as u64);
                return false;
            }

            tail.clear();
            if keep_tail == 0 {
                return true;
            }
            if combined.len() >= keep_tail {
                let start = combined.len() - keep_tail;
                tail.extend_from_slice(&combined[start..]);
            } else {
                tail.extend_from_slice(&combined);
            }
            return true;
        },
        on_progress,
        is_cancelled,
    )
    .await?;

    return Ok(MarkerStreamResult {
        found: found.is_some(),
        offset: found,
        stats,
    });
}

// ---------------------------------------------------------------------------
// Форматтеры статистики
// ---------------------------------------------------------------------------

/// Строка со статистикой потока для UI.
pub fn format_stream_stats(stats: &StreamStats, expected: u64) -> String {
    let status_line: String = if stats.cancelled {
        "⚠ Отменено".to_string()
    } else if stats.bytes_read == expected {
        "✓ Прочитано полностью — совпадает с размером файла".to_string()
    } else {
        format_str(format_args!(
            "⚠ Прочитано {} из {} — расхождение",
            format_size(stats.bytes_read),
            format_size(expected),
        ))
    };

    let mut lines: Vec<String> = Vec::with_capacity(8);
    lines.push(status_line);
    lines.push(format_str(format_args!(
        "Байт:      {}",
        format_size(stats.bytes_read),
    )));
    lines.push(format_str(format_args!("Чанков:    {}", stats.chunks)));
    lines.push(format_str(format_args!(
        "Средний:   {}",
        format_size(stats.avg_chunk_bytes()),
    )));
    lines.push(format_str(format_args!(
        "Yield'ов:  {} (раз в 16 мс)",
        stats.yields,
    )));
    lines.push(format_str(format_args!(
        "Время:     {:.0} ms · {:.1} МБ/с",
        stats.elapsed_ms,
        stats.throughput_mbps(),
    )));
    lines.push(format_str(format_args!(
        "Размеры чанков (первые {}): {}",
        stats.first_chunk_sizes.len(),
        format_chunk_sizes(&stats.first_chunk_sizes),
    )));
    return lines.join("\n");
}

/// Список размеров чанков в человекочитаемом виде.
pub fn format_chunk_sizes(sizes: &[usize]) -> String {
    if sizes.is_empty() {
        return "(нет)".to_string();
    }
    let mut parts: Vec<String> = Vec::with_capacity(sizes.len());
    for s in sizes {
        parts.push(format_str(format_args!("{}", format_size(*s as u64))));
    }
    return parts.join(", ");
}

/// Строка с результатом hash.
pub fn format_hash_result(r: &HashStreamResult, expected: u64) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(3);
    lines.push(format_str(format_args!("SHA-256: {}", r.hex)));
    lines.push(format_str(format_args!(
        "Ожидаемый размер: {}",
        format_size(expected),
    )));
    lines.push(format_str(format_args!(
        "Прочитано:        {}",
        format_size(r.stats.bytes_read),
    )));
    return lines.join("\n");
}

/// Строка с результатом подсчёта строк.
pub fn format_lines_result(r: &LinesStreamResult) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(3);
    lines.push(format_str(format_args!("Строк:    {}", r.count)));
    lines.push(format_str(format_args!(
        "Прочитано: {}",
        format_size(r.stats.bytes_read),
    )));
    return lines.join("\n");
}

/// Строка с результатом поиска маркера.
pub fn format_marker_result(r: &MarkerStreamResult, marker: &[u8]) -> String {
    let marker_display: String = String::from_utf8_lossy(marker).to_string();
    let mut lines: Vec<String> = Vec::with_capacity(4);
    if let Some(off) = r.offset {
        lines.push(format_str(format_args!("Маркер найден.")));
        lines.push(format_str(format_args!(
            "Смещение:  {off} байт (0x{off:X})",
        )));
    } else {
        lines.push("Маркер не найден.".to_string());
    }
    lines.push(format_str(format_args!(
        "Искали:    \"{marker_display}\" ({} байт)",
        marker.len(),
    )));
    lines.push(format_str(format_args!(
        "Прочитано: {}",
        format_size(r.stats.bytes_read),
    )));
    return lines.join("\n");
}