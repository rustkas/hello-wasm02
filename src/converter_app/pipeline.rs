use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use web_sys::Url;

use super::handlers::current_engine;
use super::metrics;
use super::state::{cancel_check, DemoState, Engine, StageCtx, Ui, MAX_FILE_SIZE};
use crate::{converter, converter_native, converter_ui, converter_worker};

const SERVER_BASE: &str = "http://localhost:3001/api";

fn ctx(ui: &Ui) -> StageCtx {
    StageCtx {
        stages: ui.stages.clone(),
        phase_cells: ui.phase_cells.clone(),
        phase_label: ui.phase_label.clone(),
    }
}

pub fn release_urls(state: &Rc<RefCell<DemoState>>) {
    let mut s = state.borrow_mut();
    if let Some(u) = s.original_url.take() {
        let _ = Url::revoke_object_url(&u);
    }
    if let Some(u) = s.result_url.take() {
        let _ = Url::revoke_object_url(&u);
    }
}

pub async fn run(ui: Ui, state: Rc<RefCell<DemoState>>) {
    {
        let mut s = state.borrow_mut();
        if s.pipeline_running {
            return;
        }
        s.pipeline_running = true;
        s.cancel_requested = false;
    }

    let c = ctx(&ui);
    release_urls(&state);
    let _ = ui.preview_original.remove_attribute("src");
    let _ = ui.preview_result.remove_attribute("src");
    metrics::reset_stages_and_bar(&ui);
    metrics::update_progress(&ui, 0.0);
    ui.progress_label.set_text_content(Some("0%"));
    ui.metrics.set_text_content(Some(""));
    ui.verify_badge.set_text_content(Some(""));
    ui.pick_btn.set_disabled(true);
    ui.cancel_btn.set_disabled(false);
    ui.save_btn.set_disabled(true);
    ui.upload_btn.set_disabled(true);
    ui.delete_btn.set_disabled(true);
    ui.hash_download_btn.set_disabled(true);
    ui.reset_btn.set_disabled(true);

    let engine = current_engine(&ui);

    // === Stage 0: pick ===
    c.enter(0, "Pick");
    ui.status.set_text_content(Some("Choosing file..."));
    let file = match converter_ui::pick_image_file().await {
        Ok(f) => {
            c.done(0);
            f
        }
        Err(e) => {
            let name: String = metrics::error_name(&e);
            c.cancel(0);
            let msg: String = format_pick_error(name.as_str());
            ui.status.set_text_content(Some(&msg));
            finish(&ui, false);
            state.borrow_mut().pipeline_running = false;
            return;
        }
    };

    let file_name = file.name();
    let file_size = file.size() as u64;
    let file_type = file.type_();

    if file_size > MAX_FILE_SIZE {
        c.error(1);
        let msg: String = format!(
            "File too large: {:.1} MB (limit {:.0} MB)",
            file_size as f64 / 1_048_576.0,
            MAX_FILE_SIZE as f64 / 1_048_576.0,
        );
        ui.status.set_text_content(Some(&msg));
        finish(&ui, false);
        state.borrow_mut().pipeline_running = false;
        return;
    }

    let info: String = format!(
        "Picked: {} ({:.1} MB, MIME: {})",
        file_name,
        file_size as f64 / 1_048_576.0,
        if file_type.is_empty() {
            "--"
        } else {
            &file_type
        }
    );
    ui.status.set_text_content(Some(&info));

    // === Stage 1: read ===
    c.enter(1, "Read (Blob.stream)");
    metrics::update_progress(&ui, 0.0);
    let read_t0 = js_sys::Date::now();
    let progress_cb = metrics::make_progress_cb(ui.clone(), file_size);
    let cancelled = cancel_check(state.clone());
    let outcome = match converter::read_blob_stream_validated(&file, progress_cb, cancelled).await {
        Ok(o) => o,
        Err(e) => {
            c.error(1);
            let msg: String = format!("Read error: {}", metrics::error_name(&e));
            ui.status.set_text_content(Some(&msg));
            finish(&ui, false);
            state.borrow_mut().pipeline_running = false;
            return;
        }
    };
    state.borrow_mut().read_ms = js_sys::Date::now() - read_t0;

    let data: Vec<u8> = match outcome {
        converter::ReadOutcome::Ok(d) => {
            c.done(1);
            d
        }
        converter::ReadOutcome::Cancelled => {
            c.cancel(1);
            ui.status.set_text_content(Some("Read cancelled."));
            finish(&ui, false);
            state.borrow_mut().pipeline_running = false;
            return;
        }
        converter::ReadOutcome::InvalidFormat(m) => {
            c.error(1);
            c.cancel(2);
            let msg: String = format!("Early-exit: {}", m);
            ui.status.set_text_content(Some(&msg));
            finish(&ui, false);
            state.borrow_mut().pipeline_running = false;
            return;
        }
    };

    // === Stage 2: validate ===
    c.enter(2, "Validate (sync)");
    let fmt = match converter::validate_magic_bytes(&data) {
        Ok(f) => {
            c.done(2);
            f
        }
        Err(m) => {
            c.error(2);
            ui.status.set_text_content(Some(&m));
            finish(&ui, false);
            state.borrow_mut().pipeline_running = false;
            return;
        }
    };

    let fmt_label: &str = match fmt {
        converter::ImageFormat::Png => "PNG",
        converter::ImageFormat::Jpeg => "JPEG",
    };
    let magic_hex = metrics::hex_preview(&data, 8);

    let mime_note: String = format_mime_note(&file_type, fmt);

    let status_msg: String = format!(
        "Format: {} (magic: {}; {})",
        fmt_label, magic_hex, mime_note
    );
    ui.status.set_text_content(Some(&status_msg));

    {
        let mut s = state.borrow_mut();
        s.original_len = data.len() as u64;
        s.format_label = fmt_label.to_string();
        s.magic_hex = magic_hex;
        s.engine_label = engine.label().to_string();
    }

    if let Ok(blob) = converter::bytes_to_blob(&data) {
        if let Ok(url) = Url::create_object_url_with_blob(&blob) {
            ui.preview_original.set_src(&url);
            state.borrow_mut().original_url = Some(url);
        }
    }

    let cancel_now = state
        .try_borrow()
        .map(|s| s.cancel_requested)
        .unwrap_or(true);
    if cancel_now {
        c.cancel(3);
        ui.status
            .set_text_content(Some("Cancelled before convert."));
        finish(&ui, false);
        state.borrow_mut().pipeline_running = false;
        return;
    }

    // === Stage 3: convert ===
    let phase: String = format!("Convert ({})", engine.label());
    c.enter(3, &phase);
    metrics::update_progress(&ui, 1.0);
    ui.progress_label
        .set_text_content(Some("100% (sync, no progress)"));
    ui.cancel_btn.set_disabled(true);
    TimeoutFuture::new(0).await;

    let conv_t0 = js_sys::Date::now();
    let gray: Vec<u8> = match convert_by_engine(engine, &data, fmt).await {
        Ok(g) => g,
        Err(e) => {
            c.error(3);
            let msg: String = format!("Convert error: {}", e);
            ui.status.set_text_content(Some(&msg));
            finish(&ui, false);
            state.borrow_mut().pipeline_running = false;
            return;
        }
    };
    state.borrow_mut().convert_ms = js_sys::Date::now() - conv_t0;
    ui.cancel_btn.set_disabled(false);
    c.done(3);

    if let Ok(blob) = converter::bytes_to_blob(&gray) {
        if let Ok(url) = Url::create_object_url_with_blob(&blob) {
            ui.preview_result.set_src(&url);
            state.borrow_mut().result_url = Some(url);
        }
    }

    let verified = metrics::verify_grayscale(&gray);
    let (txt, col): (&str, &str) = if verified {
        ("OK R==G==B (16 pts)", "#27ae60")
    } else {
        ("verify failed", "#c0392b")
    };
    ui.verify_badge.set_text_content(Some(txt));
    let style: String = format!("color: {}; font-weight: bold; font-size: 12px;", col);
    let _ = ui.verify_badge.set_attribute("style", &style);
    state.borrow_mut().verified = verified;

    // === Stage 4: hash ===
    c.enter(4, "SHA-256 (chunked)");
    metrics::update_progress(&ui, 0.0);
    let hash_t0 = js_sys::Date::now();
    let progress_cb = metrics::make_progress_cb(ui.clone(), gray.len() as u64);
    let stats = converter::sha256_hex_with_progress(&gray, progress_cb).await;
    let hash_ms = js_sys::Date::now() - hash_t0;

    {
        let mut s = state.borrow_mut();
        s.hash_ms = hash_ms;
        s.hash_hex = stats.hex;
        s.hash_yields = stats.yields;
        s.hash_chunks = stats.chunks;
        s.hash_chunk_size = stats.chunk_size;
        s.result_len = gray.len() as u64;
        s.result_bytes = Some(gray);
        s.result_name = Some(metrics::build_output_name(&file_name));
    }
    c.done(4);

    metrics::refresh(&ui, &state);
    ui.status.set_text_content(Some("Ready. Save or upload."));
    metrics::update_progress(&ui, 1.0);
    ui.progress_label.set_text_content(Some("100%"));
    ui.phase_label.set_text_content(Some("--"));
    state.borrow_mut().pipeline_running = false;
    finish(&ui, true);
    ui.save_btn.set_disabled(false);
    ui.hash_download_btn.set_disabled(false);
    if ui.upload_cb.checked() {
        ui.upload_btn.set_disabled(false);
    }
}

async fn convert_by_engine(
    engine: Engine,
    data: &[u8],
    fmt: converter::ImageFormat,
) -> Result<Vec<u8>, String> {
    match engine {
        Engine::WasmInline => converter::to_grayscale_png(data, fmt),
        Engine::WasmWorker => {
            let (rgba, w, h) = converter::decode_to_rgba(data, fmt)?;
            let out = converter_worker::grayscale_in_worker(rgba)
                .await
                .map_err(|e| format!("{:?}", e))?;
            converter::encode_rgba_png(&out, w, h)
        }
        Engine::WasmParallel => {
            let (rgba, w, h) = converter::decode_to_rgba(data, fmt)?;
            let out = converter_worker::grayscale_parallel(rgba)
                .await
                .map_err(|e| format!("{:?}", e))?;
            converter::encode_rgba_png(&out, w, h)
        }
        Engine::NativeCanvas => converter_native::grayscale_native(data)
            .await
            .map_err(|e| format!("{:?}", e)),
    }
}

pub async fn run_save(ui: Ui, state: Rc<RefCell<DemoState>>) {
    if state.borrow().pipeline_running {
        return;
    }

    // Извлекаем буфер в отдельном блоке — Ref не живёт через .await.
    let extracted: Option<(Vec<u8>, String)> = {
        match state.try_borrow() {
            Ok(s) => match (s.result_bytes.clone(), s.result_name.clone()) {
                (Some(b), Some(n)) => Some((b, n)),
                _ => None,
            },
            Err(_) => {
                ui.status.set_text_content(Some("State busy; retry."));
                return;
            }
        }
    };

    let (bytes, name) = match extracted {
        Some(pair) => pair,
        None => {
            ui.status.set_text_content(Some("No result to save."));
            return;
        }
    };

    state.borrow_mut().pipeline_running = true;
    state.borrow_mut().cancel_requested = false;

    let c = ctx(&ui);
    ui.save_btn.set_disabled(true);
    ui.pick_btn.set_disabled(true);
    ui.reset_btn.set_disabled(true);
    ui.cancel_btn.set_disabled(false);
    metrics::update_progress(&ui, 0.0);

    let has_fsa: bool = web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &JsValue::from_str("showSaveFilePicker")).ok())
        .map(|v| !v.is_undefined())
        .unwrap_or(false);

    let phase: &str = if has_fsa {
        "Save (chunked)"
    } else {
        "Save (fallback <a download>)"
    };
    c.enter(5, phase);
    ui.status.set_text_content(Some("Saving..."));

    let save_t0 = js_sys::Date::now();
    let progress_cb = metrics::make_progress_cb(ui.clone(), bytes.len() as u64);
    let cancelled = cancel_check(state.clone());
    let result = converter_ui::save_file_chunked(&bytes, &name, progress_cb, cancelled).await;
    state.borrow_mut().save_ms = js_sys::Date::now() - save_t0;

    match result {
        Ok(converter_ui::SaveOutcome::Fsa) => {
            let msg: String = format!("Saved (FSA): {}", name);
            ui.status.set_text_content(Some(&msg));
            c.done(5);
        }
        Ok(converter_ui::SaveOutcome::FallbackDownload) => {
            let msg: String = format!("Saved (fallback): {}", name);
            ui.status.set_text_content(Some(&msg));
            c.done(5);
        }
        Ok(converter_ui::SaveOutcome::Cancelled) => {
            ui.status.set_text_content(Some("Save cancelled."));
            c.cancel(5);
        }
        Err(e) => {
            let en: String = metrics::error_name(&e);
            let msg: String = format_save_error(en.as_str());
            ui.status.set_text_content(Some(&msg));
            c.error(5);
        }
    }

    ui.phase_label.set_text_content(Some("--"));
    metrics::refresh(&ui, &state);
    state.borrow_mut().pipeline_running = false;
    finish(&ui, true);
    ui.save_btn.set_disabled(false);
}

pub async fn run_upload(ui: Ui, state: Rc<RefCell<DemoState>>) {
    if state.borrow().pipeline_running {
        return;
    }

    let extracted: Option<(Vec<u8>, String)> = {
        match state.try_borrow() {
            Ok(s) => match (s.result_bytes.clone(), s.result_name.clone()) {
                (Some(b), Some(n)) => Some((b, n)),
                _ => None,
            },
            Err(_) => {
                ui.status.set_text_content(Some("State busy; retry."));
                return;
            }
        }
    };

    let (bytes, name) = match extracted {
        Some(pair) => pair,
        None => {
            ui.status.set_text_content(Some("No result to upload."));
            return;
        }
    };

    let existing_id: Option<String> = match state.try_borrow() {
        Ok(s) => s.upload_id.clone(),
        Err(_) => None,
    };
    let upload_id: String = match existing_id {
        Some(id) => id,
        None => converter::generate_upload_id(),
    };

    state.borrow_mut().pipeline_running = true;
    state.borrow_mut().upload_id = Some(upload_id.clone());

    let c = ctx(&ui);
    ui.upload_btn.set_disabled(true);
    ui.save_btn.set_disabled(true);
    ui.pick_btn.set_disabled(true);
    ui.reset_btn.set_disabled(true);
    ui.cancel_btn.set_disabled(false);
    metrics::update_progress(&ui, 0.0);
    c.enter(6, "Upload (XHR)");

    let short_id: &str = if upload_id.len() >= 8 {
        &upload_id[..8]
    } else {
        upload_id.as_str()
    };
    let info: String = format!("Uploading: {} -> files/{}/{}", name, short_id, name);
    ui.status.set_text_content(Some(&info));

    let t0 = js_sys::Date::now();
    let progress_cb = metrics::make_progress_cb(ui.clone(), bytes.len() as u64);
    let res = converter_ui::upload_to_server(
        &bytes,
        &upload_id,
        &name,
        "image/png",
        SERVER_BASE,
        progress_cb,
    )
    .await;
    state.borrow_mut().upload_ms = js_sys::Date::now() - t0;

    match res {
        Ok(st) if (200..300).contains(&st) => {
            state.borrow_mut().uploaded = true;
            let msg: String = format!("Uploaded (HTTP {}): {}", st, name);
            ui.status.set_text_content(Some(&msg));
            c.done(6);
            ui.delete_btn.set_disabled(false);
        }
        Ok(st) => {
            let msg: String = format!("HTTP {}; check auth-server.", st);
            ui.status.set_text_content(Some(&msg));
            c.error(6);
        }
        Err(e) => {
            let msg: String = e.as_string().unwrap_or_else(|| format!("{:?}", e));
            ui.status.set_text_content(Some(&msg));
            c.error(6);
        }
    }

    ui.phase_label.set_text_content(Some("--"));
    metrics::refresh(&ui, &state);
    state.borrow_mut().pipeline_running = false;
    finish(&ui, true);
    ui.save_btn.set_disabled(false);
}

fn finish(ui: &Ui, success: bool) {
    ui.pick_btn.set_disabled(false);
    ui.cancel_btn.set_disabled(true);
    ui.reset_btn.set_disabled(false);
    if !success {
        ui.save_btn.set_disabled(true);
        ui.upload_btn.set_disabled(true);
        ui.delete_btn.set_disabled(true);
        ui.hash_download_btn.set_disabled(true);
    }
}

fn format_pick_error(name: &str) -> String {
    match name {
        "AbortError" => {
            let s: String = "Cancelled.".to_string();
            s
        }
        "SecurityError" => {
            let s: String = "Browser requires user gesture.".to_string();
            s
        }
        "" => {
            let s: String = "Pick failed.".to_string();
            s
        }
        other => {
            let s: String = format!("Pick failed: {}", other);
            s
        }
    }
}

fn format_save_error(en: &str) -> String {
    if en == "AbortError" {
        let s: String = "Dialog closed.".to_string();
        s
    } else if en.is_empty() {
        let s: String = "Save error.".to_string();
        s
    } else {
        let s: String = format!("Save error: {}", en);
        s
    }
}
fn format_mime_note(file_type: &str, fmt: converter::ImageFormat) -> String {
    if file_type.is_empty() {
        let s: String = "MIME not set".to_string();
        return s;
    }

    let expected: &str = match fmt {
        converter::ImageFormat::Png => "image/png",
        converter::ImageFormat::Jpeg => "image/jpeg",
    };

    if file_type.eq_ignore_ascii_case(expected) {
        let s: String = "MIME matches".to_string();
        return s;
    }

    // хвост тоже обёрнут — RA не будет выводить тип format! по позиции
    let s: String = format!("MIME mismatch: {} vs {}", file_type, expected);
    s
}
