//! Замер блокировки главного потока длинным **синхронным циклом**
//! вокруг `localStorage.setItem`.
//!
//! ВАЖНО: `setItem` сам по себе не блокирует UI — блокирует непрерывная
//! работа без yield'а. Async-обёртка (yield через `TimeoutFuture`)
//! снимает блокировку, но снижает пропускную способность примерно в 4
//! раза — цена таймера. Оба эффекта измеряются в этой подглаве.
//!
//! Плюс — заполнение хранилища до квоты.
//!
//! Автономный модуль — не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;

use crate::format_str::format_str;
use crate::ls_probe;

pub fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

#[derive(Debug, Clone)]
pub struct TimingSample {
    pub size_chars: usize,
    pub write_ms: f64,
    pub read_ms: f64,
}

pub fn measure_set_get(key: &str, value: &str) -> Result<TimingSample, JsValue> {
    let t0 = now_ms();
    ls_probe::set_string(key, value)?;
    let write_ms = now_ms() - t0;

    let t1 = now_ms();
    let read_result = ls_probe::get_string(key);
    let read_ms = now_ms() - t1;

    let _ = ls_probe::remove(key);

    read_result?;
    Ok(TimingSample {
        size_chars: value.encode_utf16().count(),
        write_ms,
        read_ms,
    })
}

pub fn run_sweep(prefix: &str, sizes_chars: &[usize]) -> Result<Vec<TimingSample>, JsValue> {
    let mut out = Vec::with_capacity(sizes_chars.len());
    for (i, &size) in sizes_chars.iter().enumerate() {
        let value = "x".repeat(size);
        let key = format!("{prefix}-sweep-{i}");
        out.push(measure_set_get(&key, &value)?);
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct QuotaResult {
    pub written_chunks: u32,
    pub total_utf16_bytes: usize,
    pub chunk_utf16_bytes: usize,
    pub elapsed_ms: f64,
    pub error_msg: String,
}

pub fn fill_until_quota(
    prefix: &str,
    chunk_chars: usize,
    soft_limit: u32,
) -> Result<QuotaResult, JsValue> {
    let chunk = "x".repeat(chunk_chars);
    let chunk_utf16 = chunk.encode_utf16().count() * 2;

    let t0 = now_ms();
    let mut written: u32 = 0;
    let mut error_msg = String::new();

    for i in 0..soft_limit {
        let key = format!("{prefix}-{i}");
        match ls_probe::set_string(&key, &chunk) {
            Ok(()) => written += 1,
            Err(e) => {
                error_msg = classify_error(&e);
                break;
            }
        }
    }

    let elapsed_ms = now_ms() - t0;

    for i in 0..written {
        let key = format!("{prefix}-{i}");
        let _ = ls_probe::remove(&key);
    }

    Ok(QuotaResult {
        written_chunks: written,
        total_utf16_bytes: written as usize * chunk_utf16,
        chunk_utf16_bytes: chunk_utf16,
        elapsed_ms,
        error_msg,
    })
}

/// Асинхронная версия: yield между пакетами.
///
/// Контракт `on_progress`:
///   • вызывается после каждого успешно завершённого batch;
///   • вызывается один раз при выходе по ошибке — аварийный снимок.
pub async fn fill_until_quota_chunked<F>(
    prefix: &str,
    chunk_chars: usize,
    soft_limit: u32,
    batch_size: u32,
    mut on_progress: F,
) -> Result<QuotaResult, JsValue>
where
    F: FnMut(u32, usize),
{
    let batch = batch_size.max(1);
    let chunk = "x".repeat(chunk_chars);
    let chunk_utf16 = chunk.encode_utf16().count() * 2;

    let t0 = now_ms();
    let mut written: u32 = 0;
    let mut error_msg = String::new();

    'outer: for batch_start in (0..soft_limit).step_by(batch as usize) {
        let batch_end = (batch_start + batch).min(soft_limit);
        for i in batch_start..batch_end {
            let key = format!("{prefix}-{i}");
            match ls_probe::set_string(&key, &chunk) {
                Ok(()) => written += 1,
                Err(e) => {
                    error_msg = classify_error(&e);
                    on_progress(written, written as usize * chunk_utf16);
                    break 'outer;
                }
            }
        }
        gloo_timers::future::TimeoutFuture::new(0).await;
        on_progress(written, written as usize * chunk_utf16);
    }

    let elapsed_ms = now_ms() - t0;

    for i in 0..written {
        let key = format!("{prefix}-{i}");
        let _ = ls_probe::remove(&key);
    }

    Ok(QuotaResult {
        written_chunks: written,
        total_utf16_bytes: written as usize * chunk_utf16,
        chunk_utf16_bytes: chunk_utf16,
        elapsed_ms,
        error_msg,
    })
}

/// Классифицирует ошибку в каноническую строку.
///
/// Устойчив к обёрткам: если в тексте встречается «QuotaExceededError» —
/// возвращает каноническую строку, даже если оригинал был обёрнут.
/// Это защита на случай, если `set_string` где-то обернул ошибку в строку.
fn classify_error(e: &JsValue) -> String {
    if crate::ls_probe::is_quota_error(e) {
        return "QuotaExceededError".to_string();
    }
    // Fallback: проверяем, содержит ли текст ошибки маркер квоты.
    let text = debug_or_message(e);
    if text.contains("QuotaExceededError") {
        return "QuotaExceededError".to_string();
    }
    if text.contains("SecurityError") {
        return "SecurityError".to_string();
    }
    // Иначе — короткая строка без мусора.
    return short_text(&text);
}

/// Достаёт читаемое представление JsValue.
fn debug_or_message(e: &JsValue) -> String {
    if let Some(ex) = e.dyn_ref::<web_sys::DomException>() {
        return format_str(format_args!("{}: {}", ex.name(), ex.message()));
    }
    if let Some(s) = e.as_string() {
        return s;
    }
    return format!("{e:?}");
}

/// Обрезает текст до 120 символов.
fn short_text(s: &str) -> String {
    let first_line = s.lines().next().unwrap_or("").trim();
    let mut out: String = first_line.chars().take(120).collect();
    if first_line.chars().count() > 120 {
        out.push('…');
    }
    return out;
}

/// Человекочитаемая интерпретация `error_msg`.
pub fn describe_error(msg: &str) -> String {
    if msg.is_empty() {
        return "не было ошибки (достигнут soft_limit)".to_string();
    }
    if msg == "QuotaExceededError" {
        return "QuotaExceededError — реальная квота браузера исчерпана".to_string();
    }
    if msg == "SecurityError" {
        return "SecurityError — браузер заблокировал запись".to_string();
    }
    return format_str(format_args!("Ошибка браузера: {msg}"));
}