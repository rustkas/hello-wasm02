//! 49. File Converter: чистая логика текстовых конвертаций.
//!
//! Никакого DOM, никаких Promise. Только функции над байтами.
//! Тестируется headless.
//!
//! ## Поддерживаемые конвертации
//!
//! - `.json` → `.csv`
//! - `.csv`  → `.json`
//! - `.md`   → `.html`
//! - `.txt`  → `.json` (статистика: строки, слова, символы)
//!
//! ## Обнаружение формата
//!
//! По расширению (приоритет), затем по содержимому. Для JSON —
//! первый непустой символ `{` или `[`. Для CSV — наличие `,`/`;`
//! в первой строке. Для Markdown — заголовки `# `.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::video_frame::format_str;

// =============================================================================
// Format
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Text,
    Json,
    Csv,
    Markdown,
    Html,
    Unknown,
}

impl Format {
    pub fn as_str(&self) -> &'static str {
        return match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Markdown => "md",
            Self::Html => "html",
            Self::Unknown => "bin",
        };
    }

    pub fn label(&self) -> &'static str {
        return match self {
            Self::Text => "Plain text",
            Self::Json => "JSON",
            Self::Csv => "CSV",
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
            Self::Unknown => "Binary",
        };
    }

    pub fn mime(&self) -> &'static str {
        return match self {
            Self::Text => "text/plain;charset=utf-8",
            Self::Json => "application/json",
            Self::Csv => "text/csv;charset=utf-8",
            Self::Markdown => "text/markdown;charset=utf-8",
            Self::Html => "text/html;charset=utf-8",
            Self::Unknown => "application/octet-stream",
        };
    }
}

// =============================================================================
// Обнаружение формата
// =============================================================================

/// Определяет формат по расширению (приоритет), затем по содержимому.
pub fn detect_format(filename: &str, bytes: &[u8]) -> Format {
    let lower = filename.to_lowercase();

    let from_ext = if lower.ends_with(".json") {
        Some(Format::Json)
    } else if lower.ends_with(".csv") {
        Some(Format::Csv)
    } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
        Some(Format::Markdown)
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        Some(Format::Html)
    } else if lower.ends_with(".txt") {
        Some(Format::Text)
    } else {
        None
    };

    if let Some(f) = from_ext {
        return f;
    }

    // Magic bytes — fallback для файлов без расширения.
    let head_len = bytes.len().min(512);
    let head = &bytes[..head_len];
    let first = head.iter().find(|&&b| !b.is_ascii_whitespace()).copied();
    if matches!(first, Some(b'{') | Some(b'[')) {
        return Format::Json;
    }

    if let Ok(s) = std::str::from_utf8(head) {
        if let Some(first_line) = s.lines().next() {
            if first_line.contains(',') || first_line.contains(';') {
                return Format::Csv;
            }
        }
        if s.contains("# ") || s.contains("## ") {
            return Format::Markdown;
        }
    }

    return Format::Unknown;
}

// =============================================================================
// Планирование
// =============================================================================

/// Формат-цель по умолчанию для каждого входного формата.
pub fn choose_target(from: Format) -> Option<Format> {
    return match from {
        Format::Json => Some(Format::Csv),
        Format::Csv => Some(Format::Json),
        Format::Markdown => Some(Format::Html),
        Format::Text => Some(Format::Json),
        Format::Html | Format::Unknown => None,
    };
}

// =============================================================================
// Диспетчер
// =============================================================================

pub fn convert(from: Format, to: Format, input: &[u8]) -> Result<Vec<u8>, String> {
    return match (from, to) {
        (Format::Json, Format::Csv) => json_to_csv(input),
        (Format::Csv, Format::Json) => csv_to_json(input),
        (Format::Markdown, Format::Html) => md_to_html(input),
        (Format::Text, Format::Json) => text_to_json(input),
        _ => Err(format_str(format_args!(
            "неподдерживаемая конвертация: {:?} → {:?}",
            from, to
        ))),
    };
}

// =============================================================================
// JSON → CSV
// =============================================================================

/// Конвертация JSON в CSV. Поддерживает четыре формы корня:
///
/// - **массив объектов** — многорядный CSV с объединением ключей;
/// - **одиночный объект** — 1-строчный CSV с заголовками;
/// - **массив примитивов** — CSV с одной колонкой `value`;
/// - **одиночный примитив** — `value\n<значение>\n`.
///
/// ## Нормализация
///
/// Все четыре формы приводятся к общему виду: последовательность
/// JSON-объектов. Дальше алгоритм один и тот же — объединение
/// ключей и построчное заполнение.
///
/// ## Что НЕ поддерживается
///
/// `null` и пустой массив — нечего конвертировать. Возвращаем `Err`.
pub fn json_to_csv(input: &[u8]) -> Result<Vec<u8>, String> {
    let value: Value =
        serde_json::from_slice(input).map_err(|e| format_str(format_args!("JSON: {e}")))?;

    // Нормализуем вход в последовательность объектов.
    let rows: Vec<Value> = match normalize_to_rows(value) {
        Some(r) => r,
        None => {
            return Err(String::from(
                "нечего конвертировать: пустой массив или JSON null",
            ));
        }
    };

    // Собираем ключи в порядке первого появления.
    let mut keys: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for item in &rows {
        if let Some(obj) = item.as_object() {
            for k in obj.keys() {
                if seen.insert(k.clone()) {
                    keys.push(k.clone());
                }
            }
        }
    }

    if keys.is_empty() {
        return Err(String::from(
            "нечего конвертировать: ни одного поля в JSON",
        ));
    }

    let mut out = String::new();
    out.push_str(
        &keys
            .iter()
            .map(|k| csv_escape(k))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');

    for item in &rows {
        let row: Vec<String> = keys
            .iter()
            .map(|k| item.get(k).map(json_value_to_csv).unwrap_or_default())
            .collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }

    return Ok(out.into_bytes());
}

/// Приводит произвольный JSON к последовательности объектов.
///
/// - `[{...}, {...}]` → как есть (если элемент не объект — оборачиваем);
/// - `{...}` → `[{...}]`;
/// - `[1, 2, 3]` → `[{"value": 1}, {"value": 2}, {"value": 3}]`;
/// - `42` → `[{"value": 42}]`.
///
/// Возвращает `None` для случаев, где нечего конвертировать:
/// пустой массив или `null`.
fn normalize_to_rows(value: Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(arr) => {
            if arr.is_empty() {
                return None;
            }
            let rows: Vec<Value> = arr
                .into_iter()
                .map(|v| match v {
                    Value::Object(_) => v,
                    other => wrap_as_value(other),
                })
                .collect();
            return Some(rows);
        }
        Value::Object(_) => return Some(vec![value]),
        Value::Null => return None,
        other => return Some(vec![wrap_as_value(other)]),
    }
}

fn wrap_as_value(v: Value) -> Value {
    let mut m = Map::new();
    m.insert(String::from("value"), v);
    return Value::Object(m);
}

fn json_value_to_csv(v: &Value) -> String {
    return match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => csv_escape(s),
        _ => csv_escape(&v.to_string()),
    };
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        return format_str(format_args!("\"{escaped}\""));
    }
    return s.to_string();
}

// =============================================================================
// CSV → JSON
// =============================================================================

/// Первая строка — заголовок. Каждая следующая — объект.
pub fn csv_to_json(input: &[u8]) -> Result<Vec<u8>, String> {
    let text =
        std::str::from_utf8(input).map_err(|e| format_str(format_args!("UTF-8: {e}")))?;
    let rows = parse_csv(text)?;
    if rows.is_empty() {
        return Ok(b"[]".to_vec());
    }

    let header = &rows[0];
    let mut arr = Vec::with_capacity(rows.len().saturating_sub(1));
    for row in &rows[1..] {
        let mut obj = Map::new();
        for (i, key) in header.iter().enumerate() {
            let val = row.get(i).cloned().unwrap_or_default();
            obj.insert(key.clone(), Value::String(val));
        }
        arr.push(Value::Object(obj));
    }

    let out = serde_json::to_vec_pretty(&Value::Array(arr))
        .map_err(|e| format_str(format_args!("JSON: {e}")))?;
    return Ok(out);
}

/// Минимальный CSV-парсер с поддержкой кавычек и `""`-escaping.
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    return Ok(rows);
}

// =============================================================================
// Markdown → HTML
// =============================================================================

/// Минимальный Markdown → HTML.
pub fn md_to_html(input: &[u8]) -> Result<Vec<u8>, String> {
    let text =
        std::str::from_utf8(input).map_err(|e| format_str(format_args!("UTF-8: {e}")))?;
    let mut out = String::with_capacity(text.len() * 2);
    let mut in_p = false;

    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            close_paragraph(&mut out, &mut in_p);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            close_paragraph(&mut out, &mut in_p);
            out.push_str("<h3>");
            out.push_str(&inline_md(rest));
            out.push_str("</h3>\n");
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            close_paragraph(&mut out, &mut in_p);
            out.push_str("<h2>");
            out.push_str(&inline_md(rest));
            out.push_str("</h2>\n");
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            close_paragraph(&mut out, &mut in_p);
            out.push_str("<h1>");
            out.push_str(&inline_md(rest));
            out.push_str("</h1>\n");
            continue;
        }
        if !in_p {
            out.push_str("<p>");
            in_p = true;
        } else {
            out.push(' ');
        }
        out.push_str(&inline_md(trimmed));
    }
    close_paragraph(&mut out, &mut in_p);
    return Ok(out.into_bytes());
}

fn close_paragraph(out: &mut String, in_p: &mut bool) {
    if *in_p {
        out.push_str("</p>\n");
        *in_p = false;
    }
}

fn inline_md(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];

        if let Some(end) = rest.strip_prefix("**").and_then(|s| s.find("**")) {
            out.push_str("<strong>");
            out.push_str(&rest[2..2 + end]);
            out.push_str("</strong>");
            i += 2 + end + 2;
            continue;
        }
        if let Some(end) = rest.strip_prefix('`').and_then(|s| s.find('`')) {
            out.push_str("<code>");
            out.push_str(&rest[1..1 + end]);
            out.push_str("</code>");
            i += 1 + end + 1;
            continue;
        }
        if let Some(end) = rest.strip_prefix('*').and_then(|s| s.find('*')) {
            if end > 0 {
                out.push_str("<em>");
                out.push_str(&rest[1..1 + end]);
                out.push_str("</em>");
                i += 1 + end + 1;
                continue;
            }
        }
        if let Some(after_open) = rest.strip_prefix('[') {
            if let Some(close) = after_open.find("](") {
                let link_text = &after_open[..close];
                let after_paren = &after_open[close + 2..];
                if let Some(paren) = after_paren.find(')') {
                    let url = &after_paren[..paren];
                    out.push_str("<a href=\"");
                    out.push_str(url);
                    out.push_str("\">");
                    out.push_str(link_text);
                    out.push_str("</a>");
                    i += 1 + close + 2 + paren + 1;
                    continue;
                }
            }
        }

        let c = rest.chars().next().unwrap();
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            _ => out.push(c),
        }
        i += c.len_utf8();
    }
    return out;
}

// =============================================================================
// TXT → JSON (статистика)
// =============================================================================

#[derive(Clone, Debug)]
pub struct TextStats {
    pub bytes: usize,
    pub chars: usize,
    pub lines: usize,
    pub words: usize,
}

pub fn text_stats(input: &[u8]) -> Result<TextStats, String> {
    let text =
        std::str::from_utf8(input).map_err(|e| format_str(format_args!("UTF-8: {e}")))?;
    return Ok(TextStats {
        bytes: input.len(),
        chars: text.chars().count(),
        lines: text.lines().count(),
        words: text.split_whitespace().count(),
    });
}

pub fn text_to_json(input: &[u8]) -> Result<Vec<u8>, String> {
    let s = text_stats(input)?;
    let v = json!({
        "bytes": s.bytes,
        "chars": s.chars,
        "lines": s.lines,
        "words": s.words,
    });
    let out = serde_json::to_vec_pretty(&v)
        .map_err(|e| format_str(format_args!("JSON: {e}")))?;
    return Ok(out);
}

// =============================================================================
// Превью
// =============================================================================

/// Отдаёт первые `max_chars` символов результата для UI-превью.
/// Для бинарных результатов — hex.
pub fn preview_output(bytes: &[u8], max_chars: usize) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => {
            let total = s.chars().count();
            let taken: String = s.chars().take(max_chars).collect();
            if total > max_chars {
                return format_str(format_args!("{taken}\n… (+{} символов)", total - max_chars));
            }
            return taken;
        }
        Err(_) => {
            let hex = hex_bytes(bytes, 64);
            return format_str(format_args!("<binary>\n{hex}"));
        }
    }
}

fn hex_bytes(bytes: &[u8], limit: usize) -> String {
    let take = bytes.len().min(limit);
    let mut out = String::with_capacity(take * 3);
    for (i, b) in bytes[..take].iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format_str(format_args!("{b:02X}")));
    }
    if bytes.len() > limit {
        out.push_str(" …");
    }
    return out;
}