//! Глава 49 — Rust/WASM File Converter.
//!
//! Полный workflow для текстовых форматов: выбор файла (клик или
//! drag-and-drop) → чанковое чтение с прогрессом → определение
//! формата → конвертация в Rust → превью → скачивание.
//!
//! ## Фазы
//!
//! Idle → Ready → Reading → Converting → Done
//!     Cancelled ← (отмена в Reading/Converting)
//!     Error     ← (ошибка в любой фазе)
//!
//! ## Отмена
//!
//! `CancellationToken` — общий `Rc<Cell<bool>>`. Проверяется
//! между чанками в `read_file_chunked` и после yield между фазами.
//!
//! ## Двухфазный workflow
//!
//! Сначала конвертация с превью, потом явное «Скачать» или
//! «Открыть в новой вкладке». Пользователь видит результат
//! до сохранения.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Document, File, HtmlElement, HtmlInputElement};

use crate::file_convert::{
    choose_target, convert, detect_format, preview_output, Format,
};
use crate::file_io::{
    create_download_url, format_size, make_blob, pick_file, read_file_chunked,
    replace_extension, revoke_download_url, trigger_download, CancellationToken,
};
use crate::video_frame::format_str;

// =============================================================================
// Константы
// =============================================================================

/// Порог, после которого показываем предупреждение о размере.
/// 50 МБ текста — это уже сотни тысяч строк; для «file converter»
/// это крайний случай.
const SIZE_WARN_BYTES: u64 = 50 * 1024 * 1024;

// =============================================================================
// CSS
// =============================================================================

const STYLE_CSS: &str = r#"
.fc-desc { max-width:1000px;margin:0 0 20px 0;padding:12px 16px;background:#161b22;border-left:3px solid #58a6ff;border-radius:4px;color:#c9d1d9;font-size:14px;line-height:1.55;}
.fc-desc-line { margin:6px 0; }
.fc-desc-line code { background:#0d1117; padding:1px 5px; border-radius:3px; color:#79c0ff; }

.fc-block { max-width:1000px;margin-bottom:16px;padding:14px 16px;background:#161b22;border:1px solid #21262d;border-radius:6px;}
.fc-block-head { display:flex; align-items:baseline; gap:10px; font-size:11px;color:#8b949e;text-transform:uppercase;letter-spacing:1px;margin-bottom:10px;font-weight:600;flex-wrap:wrap;}
.fc-block-head .grow { flex:1; }

.fc-drop {
  max-width:1000px; margin-bottom:16px; padding:24px 20px;
  background:#0d1117; border:2px dashed #30363d; border-radius:6px;
  text-align:center; color:#8b949e; font-size:13px; transition:all 0.15s;
}
.fc-drop.fc-drop-hover { border-color:#58a6ff; background:#0a1620; color:#79c0ff; }
.fc-drop-hint { margin-top:6px; font-size:11px; color:#6e7681; }

.fc-row { display:flex;flex-wrap:wrap;gap:10px;align-items:center;margin-bottom:10px;}
.fc-btn { padding:8px 14px;border-radius:6px;border:1px solid #30363d;background:#21262d;color:#c9d1d9;cursor:pointer;font-size:12px;font-weight:500;}
.fc-btn:hover:not([disabled]) { background:#30363d; }
.fc-btn:disabled { opacity:0.5;cursor:not-allowed; }
.fc-btn-primary { background:#238636;border-color:#2ea043;color:#fff;}
.fc-btn-primary:hover:not([disabled]) { background:#2ea043; }
.fc-btn-danger { background:#4a1a1a;border-color:#d1242f;color:#ffa198; }
.fc-btn-danger:hover:not([disabled]) { background:#5c2020; }
.fc-btn-link { background:transparent; border:none; color:#79c0ff; text-decoration:underline; padding:8px 4px; cursor:pointer; font-size:12px; }
.fc-btn-link:hover:not([disabled]) { color:#58a6ff; }
.fc-file-input { display:none; }
.fc-file-label { display:inline-block; padding:8px 14px; border-radius:6px; background:#21262d; border:1px solid #30363d; color:#c9d1d9; cursor:pointer; font-size:12px; font-weight:500; }
.fc-file-label:hover { background:#30363d; }

.fc-info { font-family:ui-monospace,monospace; font-size:12px; line-height:1.8; color:#c9d1d9; }
.fc-info .lbl { color:#8b949e; display:inline-block; min-width:150px; }
.fc-info .val { color:#79c0ff; }
.fc-info .val-warn { color:#f0c674; }
.fc-info .arrow { color:#6e7681; margin:0 6px; }

.fc-phase { display:inline-flex; align-items:center; padding:3px 10px; border-radius:10px; font-size:10px; font-weight:600; letter-spacing:0.5px; text-transform:uppercase; }
.fc-phase-idle { background:#21262d; color:#8b949e; border:1px solid #30363d; }
.fc-phase-ready { background:#0a1620; color:#79c0ff; border:1px solid #1e3a5f; }
.fc-phase-busy { background:#2a1d10; color:#f0c674; border:1px solid #d29922; }
.fc-phase-done { background:#1a3828; color:#7ee2a8; border:1px solid #2ea043; }
.fc-phase-error { background:#4a1a1a; color:#ffa198; border:1px solid #f85149; }

.fc-steps { display:flex; align-items:center; gap:8px; margin-top:8px; flex-wrap:wrap; }
.fc-step { display:flex; align-items:center; gap:6px; padding:4px 10px; border-radius:14px; font-size:11px; font-family:ui-monospace,monospace; color:#6e7681; background:#161b22; border:1px solid #21262d; }
.fc-step-active { color:#f0c674; background:#2a1d10; border-color:#d29922; }
.fc-step-done { color:#7ee2a8; background:#1a3828; border-color:#2ea043; }
.fc-step-arrow { color:#30363d; font-size:12px; }

.fc-progress-wrap { margin-top:10px; height:8px; background:#0d1117; border-radius:4px; overflow:hidden; border:1px solid #21262d; }
.fc-progress-bar { height:100%; background:linear-gradient(90deg,#238636,#2ea043); transition:width 0.1s ease-out; }
.fc-progress-label { margin-top:4px; font-family:ui-monospace,monospace; font-size:11px; color:#8b949e; display:flex; justify-content:space-between; }

.fc-warn-card { margin-top:10px; padding:8px 12px; background:#2a1d10; color:#f0c674; border-left:3px solid #d29922; border-radius:4px; font-family:ui-monospace,monospace; font-size:12px; }
.fc-error { margin-top:10px; padding:10px 12px; background:#1a0d0d; color:#ffa198; border-left:3px solid #f85149; border-radius:4px; font-family:ui-monospace,monospace; font-size:12px; overflow-wrap:anywhere; }

.fc-preview { margin-top:10px; padding:10px 12px; background:#0a0d12; border:1px solid #161b22; border-radius:4px; font-family:ui-monospace,monospace; font-size:11.5px; color:#c9d1d9; max-height:220px; overflow-y:auto; white-space:pre-wrap; overflow-wrap:anywhere; }
.fc-preview-actions { margin-top:8px; display:flex; gap:10px; align-items:center; }
.fc-preview-actions .hint { color:#6e7681; font-size:11px; }

.fc-log { max-width:1000px;margin-bottom:16px;padding:12px 16px;background:#0d1117;border:1px solid #21262d;border-radius:6px;font-family:ui-monospace,monospace;font-size:11.5px;line-height:1.6;color:#c9d1d9;max-height:200px;overflow-y:auto;}
.fc-log-entry { display:flex; gap:8px; padding:1px 0; }
.fc-log-entry .time { color:#6e7681; flex-shrink:0; }
.fc-log-entry .msg { color:#c9d1d9; overflow-wrap:anywhere; }
.fc-log-info .msg { color:#79c0ff; }
.fc-log-ok .msg { color:#7ee2a8; }
.fc-log-warn .msg { color:#f0c674; }
.fc-log-err .msg { color:#ffa198; }

.fc-footnote { max-width:1000px;padding:10px 14px;background:#161b22;border-left:3px solid #f0883e;border-radius:4px;color:#8b949e;font-size:12px;line-height:1.55;}
.fc-footnote code { background:#0d1117; padding:1px 5px; border-radius:3px; color:#79c0ff; }
"#;

// =============================================================================
// Модели
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Ready,
    Reading,
    Converting,
    Done,
    Cancelled,
    Error,
}

impl Phase {
    fn label(&self) -> &'static str {
        return match self {
            Self::Idle => "idle",
            Self::Ready => "ready",
            Self::Reading => "reading",
            Self::Converting => "converting",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
        };
    }

    fn css(&self) -> &'static str {
        return match self {
            Self::Idle => "fc-phase-idle",
            Self::Ready => "fc-phase-ready",
            Self::Reading | Self::Converting => "fc-phase-busy",
            Self::Done => "fc-phase-done",
            Self::Cancelled | Self::Error => "fc-phase-error",
        };
    }

    /// Индекс активного шага: 0=read, 1=convert, 2=ready.
    /// `None`, если фаза не относится к workflow.
    fn step_index(&self) -> Option<u8> {
        return match self {
            Self::Reading => Some(0),
            Self::Converting => Some(1),
            Self::Done => Some(2),
            _ => None,
        };
    }
}

struct LogEntry {
    time: String,
    level: &'static str,
    text: String,
}

struct OutputInfo {
    url: String,
    filename: String,
    size: u64,
    preview: String,
}

struct AppState {
    phase: Phase,
    token: Option<CancellationToken>,
    filename: String,
    size: u64,
    detected: Option<Format>,
    target: Option<Format>,
    progress_done: u64,
    progress_total: u64,
    output: Option<OutputInfo>,
    error: Option<String>,
    logs: Vec<LogEntry>,
}

impl AppState {
    fn new() -> Self {
        return Self {
            phase: Phase::Idle,
            token: None,
            filename: String::new(),
            size: 0,
            detected: None,
            target: None,
            progress_done: 0,
            progress_total: 0,
            output: None,
            error: None,
            logs: Vec::new(),
        };
    }

    fn is_busy(&self) -> bool {
        return matches!(self.phase, Phase::Reading | Phase::Converting);
    }

    fn has_file(&self) -> bool {
        return !self.filename.is_empty();
    }
}

// =============================================================================
// Утилиты
// =============================================================================

fn as_node(el: &web_sys::Element) -> &web_sys::Node {
    return el.unchecked_ref::<web_sys::Node>();
}

fn inject_styles(document: &Document) -> Result<(), JsValue> {
    let id = "fc-injected-style";
    if let Some(old) = document.get_element_by_id(id) {
        if let Some(p) = old.parent_node() {
            p.remove_child(as_node(&old))?;
        }
    }
    let style = document.create_element("style")?;
    style.set_id(id);
    style.set_text_content(Some(STYLE_CSS));
    let head = document.head().ok_or_else(|| JsValue::from_str("no <head>"))?;
    head.append_child(as_node(&style))?;
    return Ok(());
}

fn now_timestamp() -> String {
    let d = js_sys::Date::new_0();
    let h = d.get_hours();
    let m = d.get_minutes();
    let s = d.get_seconds();
    let ms = d.get_milliseconds();
    return format_str(format_args!("{h:02}:{m:02}:{s:02}.{ms:03}"));
}

fn escape_html(s: &str) -> String {
    return s
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
}

fn push_log(st: &Rc<RefCell<AppState>>, level: &'static str, text: String) {
    let mut s = st.borrow_mut();
    s.logs.push(LogEntry {
        time: now_timestamp(),
        level,
        text,
    });
    if s.logs.len() > 100 {
        let drop = s.logs.len() - 100;
        s.logs.drain(0..drop);
    }
}

fn render_log_html(logs: &[LogEntry]) -> String {
    let mut html = String::with_capacity(logs.len() * 100);
    for e in logs {
        html.push_str(r#"<div class="fc-log-entry fc-log-"#);
        html.push_str(e.level);
        html.push_str(r#""><span class="time">"#);
        html.push_str(&e.time);
        html.push_str(r#"</span><span class="msg">"#);
        html.push_str(&escape_html(&e.text));
        html.push_str("</span></div>");
    }
    return html;
}

/// Снимает UTF-8 BOM (`EF BB BF`) с начала буфера.
///
/// Многие редакторы (в т.ч. Notepad на Windows) сохраняют файлы
/// с BOM. JSON-парсер спотыкается на первом символе, CSV-парсер
/// видит три лишних байта в первой колонке. Снимаем — и всё парсится.
///
/// UTF-16 (BOM `FF FE` / `FE FF`) не обрабатываем: конвертер
/// работает только с UTF-8.
fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return &bytes[3..];
    }
    return bytes;
}

// =============================================================================
// Рендер
// =============================================================================

fn render_state(doc: &Document, st: &AppState) {
    // Phase badge
    if let Some(el) = doc.get_element_by_id("fc-phase") {
        el.set_class_name(&format_str(format_args!("fc-phase {}", st.phase.css())));
        el.set_text_content(Some(st.phase.label()));
    }

    // Info
    if let Some(el) = doc.get_element_by_id("fc-info") {
        el.set_inner_html(&render_info_html(st));
    }

    // Steps
    if let Some(el) = doc.get_element_by_id("fc-steps") {
        el.set_inner_html(&render_steps_html(st.phase.step_index()));
    }

    // Progress
    if let Some(el) = doc.get_element_by_id("fc-progress") {
        if st.progress_total > 0 && matches!(st.phase, Phase::Reading | Phase::Converting) {
            let pct = ((st.progress_done as f64 / st.progress_total as f64) * 100.0) as u32;
            let bar = format_str(format_args!(
                r#"<div class="fc-progress-wrap"><div class="fc-progress-bar" style="width:{pct}%"></div></div><div class="fc-progress-label"><span>{pct}%</span><span>{d} / {t}</span></div>"#,
                pct = pct,
                d = format_size(st.progress_done),
                t = format_size(st.progress_total),
            ));
            el.set_inner_html(&bar);
        } else {
            el.set_inner_html("");
        }
    }

    // Warning
    if let Some(el) = doc.get_element_by_id("fc-warning") {
        if st.has_file() && st.size > SIZE_WARN_BYTES {
            el.set_class_name("fc-warn-card");
            el.set_text_content(Some(&format_str(format_args!(
                "Файл большой ({}). Текстовые конвертеры — не для гигабайтных файлов; \
                 ожидайте заметное время чтения. Нажмите «Отменить», если случайно.",
                format_size(st.size)
            ))));
        } else {
            el.set_inner_html("");
            el.set_class_name("");
        }
    }

    // Error
    if let Some(el) = doc.get_element_by_id("fc-error") {
        match &st.error {
            Some(e) => el.set_inner_html(&escape_html(e)),
            None => el.set_inner_html(""),
        }
    }

    // Preview + actions
    if let Some(el) = doc.get_element_by_id("fc-preview") {
        match &st.output {
            Some(o) => {
                let mut h = String::with_capacity(o.preview.len() + 256);
                h.push_str(&escape_html(&o.preview));
                h.push_str(r#"<div class="fc-preview-actions"><span class="hint">Проверьте результат и сохраните файл.</span></div>"#);
                el.set_inner_html(&h);
            }
            None => el.set_inner_html(""),
        }
    }

    // Log
    if let Some(el) = doc.get_element_by_id("fc-log") {
        el.set_inner_html(&render_log_html(&st.logs));
        if let Ok(el) = el.dyn_into::<HtmlElement>() {
            let _ = el.set_scroll_top(el.scroll_height() as f64);
        }
    }

    // Buttons
    let busy = st.is_busy();
    let ready = st.phase == Phase::Ready;
    let done = st.phase == Phase::Done;
    let can_reset = matches!(
        st.phase,
        Phase::Done | Phase::Cancelled | Phase::Error
    ) && st.has_file();

    set_btn(doc, "fc-convert", ready);
    set_btn(doc, "fc-cancel", busy);
    set_btn(doc, "fc-download", done);
    set_btn(doc, "fc-open-tab", done);
    set_btn(doc, "fc-reset", can_reset);

    // Drop zone — dim во время работы
    if let Some(el) = doc.get_element_by_id("fc-drop") {
        if busy {
            let _ = el.set_attribute("data-busy", "");
        } else {
            let _ = el.remove_attribute("data-busy");
        }
    }
}

fn render_info_html(st: &AppState) -> String {
    let mut h = String::with_capacity(512);
    if !st.has_file() {
        h.push_str(r#"<span class="lbl">Файл:</span><span class="val">не выбран</span>"#);
        return h;
    }

    h.push_str(r#"<span class="lbl">Файл:</span><span class="val">"#);
    h.push_str(&escape_html(&st.filename));
    h.push_str(r#"</span><br><span class="lbl">Размер:</span><span class="val">"#);
    h.push_str(&format_size(st.size));
    h.push_str("</span>");

    if let (Some(from), Some(to)) = (st.detected, st.target) {
        h.push_str(r#"<br><span class="lbl">Конвертация:</span><span class="val">"#);
        h.push_str(from.label());
        h.push_str(r#"</span><span class="arrow">→</span><span class="val">"#);
        h.push_str(to.label());
        h.push_str("</span>");
    } else if let Some(from) = st.detected {
        h.push_str(r#"<br><span class="lbl">Формат:</span><span class="val">"#);
        h.push_str(from.label());
        h.push_str(r#"</span><span class="arrow">·</span><span class="val-warn">конвертация не поддерживается</span>"#);
    }

    if let Some(o) = &st.output {
        h.push_str(r#"<br><span class="lbl">Результат:</span><span class="val">"#);
        h.push_str(&escape_html(&o.filename));
        h.push_str(" (");
        h.push_str(&format_size(o.size));
        h.push_str(")</span>");
    }

    return h;
}

fn render_steps_html(active: Option<u8>) -> String {
    let steps = ["read", "convert", "ready"];
    let mut h = String::with_capacity(256);
    for (i, name) in steps.iter().enumerate() {
        if i > 0 {
            h.push_str(r#"<span class="fc-step-arrow">→</span>"#);
        }
        let cls = match active {
            Some(a) if (i as u8) < a => "fc-step fc-step-done",
            Some(a) if (i as u8) == a => "fc-step fc-step-active",
            _ => "fc-step",
        };
        h.push_str(r#"<span class=""#);
        h.push_str(cls);
        h.push_str(r#"">"#);
        h.push_str(name);
        h.push_str("</span>");
    }
    return h;
}

fn set_btn(doc: &Document, id: &str, enabled: bool) {
    let Some(el) = doc.get_element_by_id(id) else { return };
    if enabled {
        let _ = el.remove_attribute("disabled");
    } else {
        let _ = el.set_attribute("disabled", "");
    }
}

// =============================================================================
// Действия
// =============================================================================

fn on_file_picked(doc: Document, st: Rc<RefCell<AppState>>, file: File) {
    // Revoke предыдущего output URL и очищаем логи.
    {
        let mut s = st.borrow_mut();
        if let Some(o) = s.output.take() {
            revoke_download_url(&o.url);
        }
        s.logs.clear();
    }

    let size = file.size() as u64;

    if size == 0 {
        let msg = String::from("файл пуст — нечего конвертировать");
        {
            let mut s = st.borrow_mut();
            s.phase = Phase::Error;
            s.filename = file.name();
            s.size = 0;
            s.error = Some(msg.clone());
        }
        push_log(&st, "err", msg);
        render_state(&doc, &st.borrow());
        return;
    }

    {
        let mut s = st.borrow_mut();
        s.phase = Phase::Ready;
        s.filename = file.name();
        s.size = size;
        s.detected = None;
        s.target = None;
        s.progress_done = 0;
        s.progress_total = 0;
        s.error = None;
    }
    push_log(
        &st,
        "ok",
        format_str(format_args!(
            "файл: {} ({})",
            file.name(),
            format_size(size)
        )),
    );
    render_state(&doc, &st.borrow());
}

async fn on_convert(doc: Document, st: Rc<RefCell<AppState>>, file: File) {
    if st.borrow().is_busy() {
        return;
    }

    let token = CancellationToken::new();
    {
        let mut s = st.borrow_mut();
        s.phase = Phase::Reading;
        s.token = Some(token.clone());
        s.error = None;
        if let Some(o) = s.output.take() {
            revoke_download_url(&o.url);
        }
        s.progress_done = 0;
        s.progress_total = file.size() as u64;
    }
    push_log(&st, "info", String::from("чтение файла чанками…"));
    render_state(&doc, &st.borrow());

    // --- Phase 1: chunked read
    let st_read = st.clone();
    let doc_read = doc.clone();
    let raw_bytes = match read_file_chunked(&file, &token, move |done, total| {
        {
            let mut s = st_read.borrow_mut();
            s.progress_done = done;
            s.progress_total = total;
        }
        render_state(&doc_read, &st_read.borrow());
    })
    .await
    {
        Ok(b) => b,
        Err(e) => {
            let cancelled = token.is_cancelled();
            {
                let mut s = st.borrow_mut();
                s.phase = if cancelled { Phase::Cancelled } else { Phase::Error };
                s.error = if cancelled {
                    None
                } else {
                    Some(format_str(format_args!("чтение: {e}")))
                };
                s.token = None;
            }
            push_log(
                &st,
                if cancelled { "warn" } else { "err" },
                if cancelled {
                    String::from("отменено пользователем")
                } else {
                    format_str(format_args!("чтение: {e}"))
                },
            );
            render_state(&doc, &st.borrow());
            return;
        }
    };

    // --- Phase 2: detect & strip BOM
    let detected = detect_format(&file.name(), &raw_bytes);
    let bytes = if matches!(detected, Format::Unknown | Format::Html) {
        // Для бинарных / HTML — не трогаем BOM, но и не конвертируем.
        raw_bytes
    } else {
        strip_utf8_bom(&raw_bytes).to_vec()
    };
    let target = choose_target(detected);

    {
        let mut s = st.borrow_mut();
        s.detected = Some(detected);
        s.target = target;
    }
    push_log(
        &st,
        "info",
        format_str(format_args!("определён формат: {}", detected.label())),
    );
    render_state(&doc, &st.borrow());

    // Проверка отмены между фазами.
    if token.is_cancelled() {
        {
            let mut s = st.borrow_mut();
            s.phase = Phase::Cancelled;
            s.token = None;
        }
        push_log(&st, "warn", String::from("отменено между фазами"));
        render_state(&doc, &st.borrow());
        return;
    }

    let target = match target {
        Some(t) => t,
        None => {
            let msg = format_str(format_args!(
                "формат {} не поддерживает конвертацию",
                detected.label()
            ));
            {
                let mut s = st.borrow_mut();
                s.phase = Phase::Error;
                s.error = Some(msg.clone());
                s.token = None;
            }
            push_log(&st, "err", msg);
            render_state(&doc, &st.borrow());
            return;
        }
    };

    // --- Phase 3: convert
    {
        let mut s = st.borrow_mut();
        s.phase = Phase::Converting;
    }
    push_log(
        &st,
        "info",
        format_str(format_args!(
            "конвертация: {} → {}",
            detected.label(),
            target.label()
        )),
    );
    render_state(&doc, &st.borrow());
    TimeoutFuture::new(0).await;

    if token.is_cancelled() {
        {
            let mut s = st.borrow_mut();
            s.phase = Phase::Cancelled;
            s.token = None;
        }
        push_log(&st, "warn", String::from("отменено перед конвертацией"));
        render_state(&doc, &st.borrow());
        return;
    }

    let output_bytes = match convert(detected, target, &bytes) {
        Ok(b) => b,
        Err(e) => {
            let msg = format_str(format_args!("конвертация: {e}"));
            {
                let mut s = st.borrow_mut();
                s.phase = Phase::Error;
                s.error = Some(msg.clone());
                s.token = None;
            }
            push_log(&st, "err", msg);
            render_state(&doc, &st.borrow());
            return;
        }
    };

    // --- Phase 4: blob & URL
    let blob = match make_blob(&output_bytes, target.mime()) {
        Ok(b) => b,
        Err(e) => {
            let msg = format_str(format_args!("создание Blob: {e}"));
            {
                let mut s = st.borrow_mut();
                s.phase = Phase::Error;
                s.error = Some(msg.clone());
                s.token = None;
            }
            push_log(&st, "err", msg);
            render_state(&doc, &st.borrow());
            return;
        }
    };
    let url = match create_download_url(&blob) {
        Ok(u) => u,
        Err(e) => {
            let msg = format_str(format_args!("createObjectURL: {e}"));
            {
                let mut s = st.borrow_mut();
                s.phase = Phase::Error;
                s.error = Some(msg.clone());
                s.token = None;
            }
            push_log(&st, "err", msg);
            render_state(&doc, &st.borrow());
            return;
        }
    };

    let new_filename = replace_extension(&file.name(), target.as_str());
    let preview = preview_output(&output_bytes, 500);
    let output_size = output_bytes.len() as u64;

    {
        let mut s = st.borrow_mut();
        s.phase = Phase::Done;
        s.token = None;
        s.output = Some(OutputInfo {
            url,
            filename: new_filename.clone(),
            size: output_size,
            preview,
        });
    }
    push_log(
        &st,
        "ok",
        format_str(format_args!(
            "готово: {} ({})",
            new_filename,
            format_size(output_size)
        )),
    );
    render_state(&doc, &st.borrow());
}

fn on_cancel(doc: Document, st: Rc<RefCell<AppState>>) {
    let token = st.borrow().token.clone();
    if let Some(token) = token {
        token.cancel();
        push_log(&st, "warn", String::from("запрошена отмена…"));
        render_state(&doc, &st.borrow());
    }
}

fn on_download(doc: Document, st: Rc<RefCell<AppState>>) {
    let pair = {
        let s = st.borrow();
        match &s.output {
            Some(o) => Some((o.url.clone(), o.filename.clone())),
            None => None,
        }
    };
    let Some((url, filename)) = pair else { return };

    match trigger_download(&doc, &url, &filename) {
        Ok(()) => push_log(&st, "ok", format_str(format_args!("скачано: {filename}"))),
        Err(e) => push_log(&st, "err", format_str(format_args!("download: {e}"))),
    }
    render_state(&doc, &st.borrow());
}

fn on_open_in_tab(doc: Document, st: Rc<RefCell<AppState>>) {
    let url = {
        let s = st.borrow();
        match &s.output {
            Some(o) => o.url.clone(),
            None => return,
        }
    };

    let Some(window) = web_sys::window() else { return };
    match window.open_with_url_and_target(&url, "_blank") {
        Ok(_) => {
            push_log(
                &st,
                "info",
                String::from("открыто в новой вкладке"),
            );
        }
        Err(e) => {
            push_log(&st, "err", format_str(format_args!("open: {e:?}")));
        }
    }
    render_state(&doc, &st.borrow());
}

fn on_reset(doc: Document, st: Rc<RefCell<AppState>>) {
    {
        let mut s = st.borrow_mut();
        if let Some(o) = s.output.take() {
            revoke_download_url(&o.url);
        }
        // Файл (filename/size) сохраняем — можно запустить конвертацию
        // снова без повторного выбора. Сбрасываем только производные поля.
        s.phase = if s.size > 0 { Phase::Ready } else { Phase::Idle };
        s.detected = None;
        s.target = None;
        s.progress_done = 0;
        s.progress_total = 0;
        s.error = None;
        s.token = None;
    }
    push_log(&st, "info", String::from("сброшено — готов к новой конвертации"));
    render_state(&doc, &st.borrow());
}

// =============================================================================
// Сборка UI
// =============================================================================

fn build_intro(doc: &Document) -> Result<web_sys::Node, JsValue> {
    let d = doc.create_element("div")?;
    d.set_class_name("fc-desc");
    d.set_inner_html(
        "<p class=\"fc-desc-line\"><strong>Rust/WASM File Converter.</strong> \
         Полный workflow: выбор файла (клик или drag-and-drop) → \
         чанковое чтение с прогрессом → определение формата → \
         конвертация в Rust → превью → скачивание.</p>\
         <p class=\"fc-desc-line\"><strong>Что поддерживается.</strong> \
         <code>.json → .csv</code>, <code>.csv → .json</code>, \
         <code>.md → .html</code>, <code>.txt → .json</code> (статистика). \
         UTF-8 BOM снимается автоматически.</p>\
         <p class=\"fc-desc-line\"><strong>Отмена.</strong> \
         Файл читается кусками по 64 КБ, между кусками проверяется \
         <code>CancellationToken</code> и делается yield — кнопка \
         «Отменить» работает в реальном времени.</p>",
    );
    return Ok(d.unchecked_into::<web_sys::Node>());
}

fn build_controls(doc: &Document) -> Result<web_sys::Node, JsValue> {
    let d = doc.create_element("div")?;
    d.set_class_name("fc-block");

    let mut html = String::with_capacity(2048);
    html.push_str(r#"<div class="fc-block-head"><span>Файл</span><span class="grow"></span><span id="fc-phase" class="fc-phase fc-phase-idle">idle</span></div>"#);

    html.push_str(r#"<div class="fc-row">"#);
    html.push_str(r#"<input type="file" id="fc-input" class="fc-file-input">"#);
    html.push_str(r#"<label for="fc-input" class="fc-file-label">Выбрать файл…</label>"#);
    html.push_str(r#"<button class="fc-btn fc-btn-primary" id="fc-convert" disabled>Конвертировать</button>"#);
    html.push_str(r#"<button class="fc-btn fc-btn-danger" id="fc-cancel" disabled>Отменить</button>"#);
    html.push_str(r#"<button class="fc-btn" id="fc-download" disabled>Скачать результат</button>"#);
    html.push_str(r#"<button class="fc-btn fc-btn-link" id="fc-open-tab" disabled>Открыть в новой вкладке ↗</button>"#);
    html.push_str(r#"<button class="fc-btn" id="fc-reset" disabled>Сбросить</button>"#);
    html.push_str("</div>");

    html.push_str(r#"<div class="fc-info" id="fc-info"><span class="lbl">Файл:</span><span class="val">не выбран</span></div>"#);
    html.push_str(r#"<div class="fc-steps" id="fc-steps"></div>"#);
    html.push_str(r#"<div id="fc-progress"></div>"#);
    html.push_str(r#"<div id="fc-warning"></div>"#);
    html.push_str(r#"<div id="fc-error"></div>"#);
    html.push_str(r#"<div id="fc-preview" class="fc-preview"></div>"#);

    d.set_inner_html(&html);
    return Ok(d.unchecked_into::<web_sys::Node>());
}

fn build_drop_zone(doc: &Document) -> Result<web_sys::Node, JsValue> {
    let d = doc.create_element("div")?;
    d.set_class_name("fc-drop");
    d.set_id("fc-drop");
    d.set_inner_html(
        "<div>Перетащите файл сюда или нажмите «Выбрать файл…»</div>\
         <div class=\"fc-drop-hint\">.json · .csv · .md · .txt</div>",
    );
    return Ok(d.unchecked_into::<web_sys::Node>());
}

fn build_log(doc: &Document) -> Result<web_sys::Node, JsValue> {
    let d = doc.create_element("div")?;
    d.set_class_name("fc-log");
    d.set_id("fc-log");
    return Ok(d.unchecked_into::<web_sys::Node>());
}

fn build_footnote(doc: &Document) -> Result<web_sys::Node, JsValue> {
    let d = doc.create_element("div")?;
    d.set_class_name("fc-footnote");
    d.set_inner_html(
        "<strong>Правила, применённые в демо.</strong> \
         <em>Реальный прогресс, не indeterminate bar:</em> чанковое чтение даёт \
         измеримый процент (64 KB × N). <em>Отмена идемпотентна:</em> \
         <code>CancellationToken</code> можно выставить много раз, эффект один. \
         <em>Object URL освобождается:</em> при повторной конвертации и Reset мы \
         вызываем <code>revokeObjectURL</code> — иначе утечка памяти. \
         <em>BOM снимается:</em> <code>EF BB BF</code> в начале UTF-8 ломает \
         JSON-парсер; демо удаляет его до конвертации. \
         <em>Никаких <code>unwrap()</code> в обработчиках:</em> только \
         <code>Result</code> и понятные сообщения. <em>Двухфазный workflow:</em> \
         сначала конвертация с превью, потом явная кнопка «Скачать» или \
         «Открыть в новой вкладке».",
    );
    return Ok(d.unchecked_into::<web_sys::Node>());
}

// =============================================================================
// Точка входа
// =============================================================================

pub fn run() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window.document().ok_or_else(|| JsValue::from_str("no document"))?;

    inject_styles(&document)?;

    let root = document
        .get_element_by_id("app")
        .ok_or_else(|| JsValue::from_str("no #app"))?;
    root.set_inner_html("");

    let h2 = document.create_element("h2")?;
    h2.set_text_content(Some("Глава 49 — Rust/WASM File Converter"));
    root.append_child(as_node(&h2))?;

    root.append_child(&build_intro(&document)?)?;

    let state = Rc::new(RefCell::new(AppState::new()));

    root.append_child(&build_drop_zone(&document)?)?;
    root.append_child(&build_controls(&document)?)?;
    root.append_child(&build_log(&document)?)?;
    root.append_child(&build_footnote(&document)?)?;

    // Общая ячейка для текущего File — нужна обработчику Convert.
    let current_file: Rc<RefCell<Option<File>>> = Rc::new(RefCell::new(None));

    // ---- file input ----
    {
        let doc = document.clone();
        let st = state.clone();
        let cf = current_file.clone();
        let input = document
            .get_element_by_id("fc-input")
            .ok_or_else(|| JsValue::from_str("no #fc-input"))?
            .dyn_into::<HtmlInputElement>()?;
        // Клон для замыкания — сам `input` остаётся доступен ниже для
        // set_onchange. `HtmlInputElement::clone` дешёвый (refcount).
        let input_cb = input.clone();
        let closure: Closure<dyn FnMut()> = Closure::new(move || {
            let Some(file) = pick_file(&input_cb) else { return };
            *cf.borrow_mut() = Some(file.clone());
            on_file_picked(doc.clone(), st.clone(), file);
        });
        input.set_onchange(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }

    // ---- drag & drop ----
    {
        let doc = document.clone();
        let st = state.clone();
        let cf = current_file.clone();
        let drop_el = document
            .get_element_by_id("fc-drop")
            .ok_or_else(|| JsValue::from_str("no #fc-drop"))?;
        let drop_el: HtmlElement = drop_el.dyn_into()?;

        // dragover — preventDefault, иначе drop не сработает.
        let drop_for_over = drop_el.clone();
        let on_over: Closure<dyn FnMut(web_sys::DragEvent)> =
            Closure::new(move |e: web_sys::DragEvent| {
                e.prevent_default();
                let _ = drop_for_over.set_class_name("fc-drop fc-drop-hover");
            });
        drop_el.add_event_listener_with_callback("dragover", on_over.as_ref().unchecked_ref())?;
        on_over.forget();

        // dragleave — снять подсветку.
        let drop_for_leave = drop_el.clone();
        let on_leave: Closure<dyn FnMut(web_sys::DragEvent)> =
            Closure::new(move |_e: web_sys::DragEvent| {
                let _ = drop_for_leave.set_class_name("fc-drop");
            });
        drop_el.add_event_listener_with_callback("dragleave", on_leave.as_ref().unchecked_ref())?;
        on_leave.forget();

        // drop — взять первый файл.
        let on_drop: Closure<dyn FnMut(web_sys::DragEvent)> =
            Closure::new(move |e: web_sys::DragEvent| {
                e.prevent_default();
                let Some(target) = e.current_target() else { return };
                if let Ok(el) = target.dyn_into::<HtmlElement>() {
                    let _ = el.set_class_name("fc-drop");
                }
                let Some(dt) = e.data_transfer() else { return };
                let Some(files) = dt.files() else { return };
                if files.length() == 0 {
                    return;
                }
                let Some(file) = files.item(0) else { return };
                *cf.borrow_mut() = Some(file.clone());
                on_file_picked(doc.clone(), st.clone(), file);
            });
        drop_el.add_event_listener_with_callback("drop", on_drop.as_ref().unchecked_ref())?;
        on_drop.forget();
    }

    // ---- convert ----
    {
        let doc = document.clone();
        let st = state.clone();
        let cf = current_file.clone();
        let btn = document
            .get_element_by_id("fc-convert")
            .ok_or_else(|| JsValue::from_str("no #fc-convert"))?;
        let btn: HtmlElement = btn.dyn_into()?;
        let closure: Closure<dyn FnMut()> = Closure::new(move || {
            let file = cf.borrow().clone();
            let Some(file) = file else { return };
            let doc_async = doc.clone();
            let st_async = st.clone();
            spawn_local(async move {
                on_convert(doc_async, st_async, file).await;
            });
        });
        btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // ---- cancel ----
    {
        let doc = document.clone();
        let st = state.clone();
        let btn = document
            .get_element_by_id("fc-cancel")
            .ok_or_else(|| JsValue::from_str("no #fc-cancel"))?;
        let btn: HtmlElement = btn.dyn_into()?;
        let closure: Closure<dyn FnMut()> = Closure::new(move || {
            on_cancel(doc.clone(), st.clone());
        });
        btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // ---- download ----
    {
        let doc = document.clone();
        let st = state.clone();
        let btn = document
            .get_element_by_id("fc-download")
            .ok_or_else(|| JsValue::from_str("no #fc-download"))?;
        let btn: HtmlElement = btn.dyn_into()?;
        let closure: Closure<dyn FnMut()> = Closure::new(move || {
            on_download(doc.clone(), st.clone());
        });
        btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // ---- open in tab ----
    {
        let doc = document.clone();
        let st = state.clone();
        let btn = document
            .get_element_by_id("fc-open-tab")
            .ok_or_else(|| JsValue::from_str("no #fc-open-tab"))?;
        let btn: HtmlElement = btn.dyn_into()?;
        let closure: Closure<dyn FnMut()> = Closure::new(move || {
            on_open_in_tab(doc.clone(), st.clone());
        });
        btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // ---- reset ----
    {
        let doc = document.clone();
        let st = state.clone();
        let btn = document
            .get_element_by_id("fc-reset")
            .ok_or_else(|| JsValue::from_str("no #fc-reset"))?;
        let btn: HtmlElement = btn.dyn_into()?;
        let closure: Closure<dyn FnMut()> = Closure::new(move || {
            on_reset(doc.clone(), st.clone());
        });
        btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    push_log(
        &state,
        "info",
        String::from("готово — выберите файл или перетащите его в область выше"),
    );
    render_state(&document, &state.borrow());

    return Ok(());
}