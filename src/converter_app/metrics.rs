use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use super::state::{set_stage, DemoState, StageStatus, Ui, STAGES};

pub fn update_progress(ui: &Ui, frac: f64) {
    let pct = (frac * 100.0).clamp(0.0, 100.0);
    ui.progress.set_value(frac);
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("progress-inner") {
            let style: String = format!("width: {:.1}%;", pct);
            let _ = el.set_attribute("style", &style);
        }
    }
}

/// Отдельная функция с явным `-> String`.
/// Ранний `return` для простого случая — rust-analyzer не путается.
fn format_progress_label(frac: f64, done: u64, total_bytes: u64, elapsed: f64) -> String {
    if elapsed <= 0.05 || frac <= 0.01 {
        return format!("{:>3.0}%", frac * 100.0);
    }

    let speed: f64 = done as f64 / elapsed / 1_048_576.0;
    let eta: f64 = if frac > 0.001 {
        elapsed / frac * (1.0 - frac)
    } else {
        0.0
    };

    let pct: f64 = frac * 100.0;
    let done_mb: f64 = done as f64 / 1_048_576.0;
    let total_mb: f64 = total_bytes as f64 / 1_048_576.0;

    return format!(
        "{:>3.0}% · {:.1}/{:.1} MB · {:.1} MB/s · ETA {:.1} s",
        pct, done_mb, total_mb, speed, eta
    );
}

pub fn make_progress_cb(ui: Ui, total_bytes: u64) -> impl FnMut(f64) {
    let t0: f64 = js_sys::Date::now();
    move |frac: f64| {
        update_progress(&ui, frac);
        let elapsed: f64 = (js_sys::Date::now() - t0) / 1000.0;
        let done: u64 = (frac * total_bytes as f64) as u64;
        let label: String = format_progress_label(frac, done, total_bytes, elapsed);
        ui.progress_label.set_text_content(Some(&label));
    }
}

pub fn reset_stages_and_bar(ui: &Ui) {
    for (i, s) in ui.stages.iter().enumerate() {
        set_stage(s, STAGES[i], StageStatus::Pending);
    }
    for c in ui.phase_cells.iter() {
        let _ = c.set_attribute("class", "phase-cell");
    }
    ui.phase_label.set_text_content(Some("--"));
}

pub fn error_name(e: &JsValue) -> String {
    // Разворачиваем цепочку в явные шаги — rust-analyzer выводит типы
    // пошагово, без попытки связать всё сразу.
    let raw: Result<JsValue, JsValue> = js_sys::Reflect::get(e, &JsValue::from_str("name"));
    let opt: Option<JsValue> = raw.ok();
    let as_string: Option<String> = opt.and_then(|v| v.as_string());
    match as_string {
        Some(s) => s,
        None => String::new(),
    }
}

pub fn hex_preview(data: &[u8], n: usize) -> String {
    let take: usize = data.len().min(n);
    let mut out: String = String::with_capacity(take * 3);
    for (i, b) in data[..take].iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:02X}", b));
    }
    out
}

pub fn build_output_name(original: &str) -> String {
    let lower: String = original.to_ascii_lowercase();
    // Сначала длина суффикса, потом отдельно — стем.
    let suffix_len: usize = if lower.ends_with(".jpeg") {
        5
    } else if lower.ends_with(".jpg") {
        4
    } else if lower.ends_with(".png") {
        4
    } else {
        0
    };
    let stem: &str = if suffix_len == 0 {
        original
    } else {
        &original[..original.len() - suffix_len]
    };
    let mut out: String = String::with_capacity(stem.len() + 14);
    out.push_str(stem);
    out.push_str(".grayscale.png");
    out
}

pub fn verify_grayscale(png_bytes: &[u8]) -> bool {
    use image::GenericImageView;
    let img = match image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png) {
        Ok(i) => i,
        Err(_) => return false,
    };
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return true;
    }
    for iy in 0..4u32 {
        for ix in 0..4u32 {
            let x = (ix * w / 4).min(w - 1);
            let y = (iy * h / 4).min(h - 1);
            let p = img.get_pixel(x, y).0;
            if p[0] != p[1] || p[1] != p[2] {
                return false;
            }
        }
    }
    true
}

pub fn refresh(ui: &Ui, state: &Rc<RefCell<DemoState>>) {
    let s = state.borrow();

    let orig: u64 = s.original_len;
    let res: u64 = s.result_len;

    let ratio: f64 = if orig > 0 {
        (res as f64 / orig as f64 - 1.0) * 100.0
    } else {
        0.0
    };

    let hash_prefix: String = if s.hash_hex.len() >= 16 {
        s.hash_hex[..16].to_string()
    } else {
        s.hash_hex.clone()
    };

    let sum_other: f64 = s.read_ms + s.hash_ms + s.save_ms;
    let bottleneck: String = if s.convert_ms > 2.0 * sum_other && s.convert_ms > 1.0 {
        "  <- bottleneck".to_string()
    } else {
        String::new()
    };

    let read_mbps: f64 = if s.read_ms > 0.0 {
        orig as f64 / 1_048_576.0 / (s.read_ms / 1000.0)
    } else {
        0.0
    };
    let hash_mbps: f64 = if s.hash_ms > 0.0 {
        res as f64 / 1_048_576.0 / (s.hash_ms / 1000.0)
    } else {
        0.0
    };
    let save_mbps: f64 = if s.save_ms > 0.0 {
        res as f64 / 1_048_576.0 / (s.save_ms / 1000.0)
    } else {
        0.0
    };

    let verify_text: String = if s.result_len == 0 {
        "--".to_string()
    } else if s.verified {
        "grayscale OK (R==G==B on 16 points)".to_string()
    } else {
        "verify failed".to_string()
    };

    let upload_line: String = format_upload_line(&s);

    // Собираем построчно в Vec<String> и склеиваем — убирает
    // многострочный format! с `\`-переносами и `\x20`.
    let mut lines: Vec<String> = Vec::with_capacity(9);
    lines.push(format!(
        "Format:      {} (magic: {})",
        s.format_label, s.magic_hex
    ));
    lines.push(format!("Engine:      {}", s.engine_label));
    lines.push(format!(
        "Size:        {} -> {} bytes ({:+.1} %)",
        orig, res, ratio
    ));
    lines.push(format!(
        "Timings:     read {:.1} ms ({:.1} MB/s) | convert {:.1} ms{}",
        s.read_ms, read_mbps, s.convert_ms, bottleneck
    ));
    lines.push(format!(
        "             hash {:.1} ms ({:.1} MB/s) | save {:.1} ms ({:.1} MB/s)",
        s.hash_ms, hash_mbps, s.save_ms, save_mbps
    ));
    if !upload_line.is_empty() {
        lines.push(upload_line);
    }
    lines.push(format!("SHA-256:     {}...", hash_prefix));
    lines.push(format!(
        "Throttling:  chunk={} KiB | iters={} | yields={}",
        s.hash_chunk_size / 1024,
        s.hash_chunks,
        s.hash_yields
    ));
    lines.push(format!("Self-check:  {}", verify_text));

    let text: String = lines.join("\n");
    ui.metrics.set_text_content(Some(&text));
}

fn format_upload_line(s: &DemoState) -> String {
    match s.upload_id.as_ref() {
        Some(id) => {
            let short: &str = if id.len() >= 8 { &id[..8] } else { id.as_str() };
            let line: String = format!(
                "Upload:      id={}... | {} ms | delete {} ms",
                short, s.upload_ms, s.delete_ms
            );
            line
        }
        None => {
            let empty: String = String::new();
            empty
        }
    }
}