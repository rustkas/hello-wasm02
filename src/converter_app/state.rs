use std::cell::RefCell;
use std::rc::Rc;
use web_sys::{
    HtmlButtonElement, HtmlElement, HtmlImageElement, HtmlInputElement,
    HtmlProgressElement, HtmlSelectElement, HtmlSpanElement,
};

pub const STAGES: [&str; 7] = [
    "Выбор", "Чтение", "Валидация", "Конвертация", "Хеш", "Сохранение", "Загрузка",
];
pub const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq)]
pub enum StageStatus {
    Pending, Active, Done, Error, Cancelled,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Engine {
    WasmInline,
    WasmWorker,
    WasmParallel,
    NativeCanvas,
}

impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Engine::WasmInline => "WASM inline",
            Engine::WasmWorker => "WASM worker",
            Engine::WasmParallel => "WASM parallel",
            Engine::NativeCanvas => "Native OffscreenCanvas",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "worker" => Engine::WasmWorker,
            "parallel" => Engine::WasmParallel,
            "native" => Engine::NativeCanvas,
            _ => Engine::WasmInline,
        }
    }
}

#[derive(Default)]
pub struct DemoState {
    pub result_bytes: Option<Vec<u8>>,
    pub result_name: Option<String>,
    pub original_url: Option<String>,
    pub result_url: Option<String>,
    pub cancel_requested: bool,
    pub pipeline_running: bool,
    pub upload_id: Option<String>,
    pub uploaded: bool,
    pub original_len: u64,
    pub result_len: u64,
    pub format_label: String,
    pub magic_hex: String,
    pub hash_hex: String,
    pub verified: bool,
    pub read_ms: f64,
    pub convert_ms: f64,
    pub hash_ms: f64,
    pub save_ms: f64,
    pub upload_ms: f64,
    pub delete_ms: f64,
    pub hash_yields: u32,
    pub hash_chunks: u32,
    pub hash_chunk_size: usize,
    pub engine_label: String,
}

#[derive(Clone)]
pub struct Ui {
    pub pick_btn: HtmlButtonElement,
    pub cancel_btn: HtmlButtonElement,
    pub save_btn: HtmlButtonElement,
    pub reset_btn: HtmlButtonElement,
    pub upload_cb: HtmlInputElement,
    pub upload_btn: HtmlButtonElement,
    pub delete_btn: HtmlButtonElement,
    pub hash_download_btn: HtmlButtonElement,
    pub engine_select: HtmlSelectElement,
    pub progress: HtmlProgressElement,
    pub progress_label: HtmlSpanElement,
    pub phase_label: HtmlSpanElement,
    pub stages: Rc<Vec<HtmlSpanElement>>,
    pub phase_cells: Rc<Vec<HtmlElement>>,
    pub status: HtmlElement,
    pub metrics: HtmlElement,
    pub preview_original: HtmlImageElement,
    pub preview_result: HtmlImageElement,
    pub verify_badge: HtmlSpanElement,
}

/// 5.7 — централизованное управление стадиями.
pub struct StageCtx {
    pub stages: Rc<Vec<HtmlSpanElement>>,
    pub phase_cells: Rc<Vec<HtmlElement>>,
    pub phase_label: HtmlSpanElement,
}

impl StageCtx {
    pub fn enter(&self, i: usize, phase: &str) {
        set_stage(&self.stages[i], STAGES[i], StageStatus::Active);
        self.phase_label.set_text_content(Some(phase));
        if let Some(c) = self.phase_cells.get(i) {
            let _ = c.set_attribute("class", "phase-cell active");
        }
    }
    pub fn done(&self, i: usize) {
        set_stage(&self.stages[i], STAGES[i], StageStatus::Done);
        if let Some(c) = self.phase_cells.get(i) {
            let _ = c.set_attribute("class", "phase-cell done");
        }
    }
    pub fn error(&self, i: usize) {
        set_stage(&self.stages[i], STAGES[i], StageStatus::Error);
        if let Some(c) = self.phase_cells.get(i) {
            let _ = c.set_attribute("class", "phase-cell error");
        }
    }
    pub fn cancel(&self, i: usize) {
        set_stage(&self.stages[i], STAGES[i], StageStatus::Cancelled);
    }
    pub fn set_phase(&self, text: &str) {
        self.phase_label.set_text_content(Some(text));
    }
}

pub fn set_stage(stage: &HtmlSpanElement, name: &str, status: StageStatus) {
    let (color, icon) = match status {
        StageStatus::Pending => ("#aaa", "○"),
        StageStatus::Active => ("#e67e22", "◐"),
        StageStatus::Done => ("#27ae60", "●"),
        StageStatus::Error => ("#c0392b", "✗"),
        StageStatus::Cancelled => ("#7f8c8d", "⊘"),
    };
    stage.set_text_content(Some(&format!("{} {}", icon, name)));
    let _ = stage.set_attribute(
        "style",
        &format!(
            "color: {}; margin-right: 12px; font-family: monospace;",
            color
        ),
    );
}

pub fn cancel_check(state: Rc<RefCell<DemoState>>) -> impl Fn() -> bool {
    move || {
        state
            .try_borrow()
            .map(|s| s.cancel_requested)
            .unwrap_or(true)
    }
}