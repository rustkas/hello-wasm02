use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{
    Document, HtmlButtonElement, HtmlElement, HtmlImageElement, HtmlInputElement,
    HtmlOptionElement, HtmlProgressElement, HtmlSelectElement, HtmlSpanElement,
};

use super::state::{DemoState, StageStatus, Ui, STAGES};

pub fn build() -> (HtmlElement, Ui, Rc<RefCell<DemoState>>) {
    let document = web_sys::window().unwrap().document().unwrap();
    let section = document.create_element("section").unwrap();
    section.set_id("converter-demo");

    let h2 = document.create_element("h2").unwrap();
    h2.set_text_content(Some("18.18 — Конвертер: PNG/JPEG → Grayscale PNG"));
    section.append_child(&h2).unwrap();

    let controls = document.create_element("div").unwrap();
    controls
        .set_attribute(
            "style",
            "margin-bottom: 12px; display: flex; gap: 8px; flex-wrap: wrap; align-items: center;",
        )
        .unwrap();

    let pick_btn = make_button(&document, "Выбрать изображение", "pick-btn");
    let cancel_btn = make_button(&document, "Отмена", "cancel-btn");
    let save_btn = make_button(&document, "Сохранить результат", "save-btn");
    let reset_btn = make_button(&document, "Сбросить", "reset-btn");
    let hash_download_btn = make_button(&document, "Скачать SHA-256", "hash-dl-btn");
    let delete_btn = make_button(&document, "Удалить с сервера", "delete-btn");
    cancel_btn.set_disabled(true);
    save_btn.set_disabled(true);
    hash_download_btn.set_disabled(true);
    delete_btn.set_disabled(true);
    for b in [&pick_btn, &cancel_btn, &save_btn, &hash_download_btn, &reset_btn] {
        controls.append_child(b).unwrap();
    }

    let engine_select = document
        .create_element("select")
        .unwrap()
        .dyn_into::<HtmlSelectElement>()
        .unwrap();
    engine_select.set_id("engine-select");
    for (val, label) in [
        ("wasm", "WASM inline"),
        ("worker", "WASM worker"),
        ("parallel", "WASM parallel"),
        ("native", "Native OffscreenCanvas"),
    ] {
        let opt = document
            .create_element("option")
            .unwrap()
            .dyn_into::<HtmlOptionElement>()
            .unwrap();
        opt.set_value(val);
        opt.set_text_content(Some(label));
        engine_select.append_child(&opt).unwrap();
    }
    let lbl = document.create_element("label").unwrap();
    lbl.set_text_content(Some("Движок:"));
    lbl.set_attribute("style", "margin-left: 12px;").unwrap();
    controls.append_child(&lbl).unwrap();
    controls.append_child(&engine_select).unwrap();

    let upload_cb = document
        .create_element("input")
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    upload_cb.set_type("checkbox");
    upload_cb.set_id("upload-cb");
    let upload_btn = make_button(&document, "Загрузить", "upload-btn");
    upload_btn.set_disabled(true);
    let upload_lbl = document.create_element("label").unwrap();
    upload_lbl.set_text_content(Some("Загрузить на сервер"));
    upload_lbl.set_attribute("style", "margin-left: 12px;").unwrap();
    upload_lbl.append_child(&upload_cb).unwrap();
    controls.append_child(&upload_lbl).unwrap();
    controls.append_child(&upload_btn).unwrap();
    controls.append_child(&delete_btn).unwrap();
    section.append_child(&controls).unwrap();

    // Pipeline stages
    let stages_row = document.create_element("div").unwrap();
    stages_row
        .set_attribute(
            "style",
            "margin-bottom: 8px; padding: 8px; background: #f9f9f9; border-radius: 4px; font-size: 14px;",
        )
        .unwrap();
    let mut stages: Vec<HtmlSpanElement> = Vec::with_capacity(STAGES.len());
    for (i, name) in STAGES.iter().enumerate() {
        let s = document
            .create_element("span")
            .unwrap()
            .dyn_into::<HtmlSpanElement>()
            .unwrap();
        super::state::set_stage(&s, name, StageStatus::Pending);
        stages_row.append_child(&s).unwrap();
        stages.push(s);
        if i + 1 < STAGES.len() {
            let arrow = document.create_element("span").unwrap();
            arrow.set_text_content(Some("→"));
            arrow
                .set_attribute("style", "color: #ccc; margin-right: 12px;")
                .unwrap();
            stages_row.append_child(&arrow).unwrap();
        }
    }
    section.append_child(&stages_row).unwrap();

    // 4.4 — тонкая полоса фаз
    let phase_bar = document.create_element("div").unwrap();
    phase_bar.set_attribute("class", "phase-bar").unwrap();
    let mut phase_cells: Vec<HtmlElement> = Vec::with_capacity(STAGES.len());
    for _ in STAGES.iter() {
        let c = document
            .create_element("div")
            .unwrap()
            .dyn_into::<HtmlElement>()
            .unwrap();
        c.set_attribute("class", "phase-cell").unwrap();
        phase_bar.append_child(&c).unwrap();
        phase_cells.push(c);
    }
    section.append_child(&phase_bar).unwrap();

    // Превью
    let preview_row = document.create_element("div").unwrap();
    preview_row
        .set_attribute("style", "display: flex; gap: 24px; margin: 12px 0;")
        .unwrap();
    let (orig_wrap, preview_original, _) =
        make_preview(&document, "preview-original", "Оригинал");
    let (res_wrap, preview_result, verify_badge) =
        make_preview(&document, "preview-result", "Grayscale");
    preview_row.append_child(&orig_wrap).unwrap();
    preview_row.append_child(&res_wrap).unwrap();
    section.append_child(&preview_row).unwrap();

    // Progress + labels
    let progress_row = document.create_element("div").unwrap();
    progress_row
        .set_attribute(
            "style",
            "display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 8px;",
        )
        .unwrap();
    let progress_outer = document.create_element("div").unwrap();
    progress_outer.set_id("progress-outer");
    let progress_inner = document.create_element("div").unwrap();
    progress_inner.set_id("progress-inner");
    progress_inner.set_attribute("style", "width: 0%;").unwrap();
    progress_outer.append_child(&progress_inner).unwrap();
    let progress = document
        .create_element("progress")
        .unwrap()
        .dyn_into::<HtmlProgressElement>()
        .unwrap();
    progress.set_max(1.0);
    progress.set_value(0.0);
    progress.set_attribute("style", "display:none;").unwrap();
    let progress_label = document
        .create_element("span")
        .unwrap()
        .dyn_into::<HtmlSpanElement>()
        .unwrap();
    progress_label.set_text_content(Some("0%"));
    let phase_label = document
        .create_element("span")
        .unwrap()
        .dyn_into::<HtmlSpanElement>()
        .unwrap();
    phase_label.set_text_content(Some("—"));
    phase_label
        .set_attribute("style", "color:#555; font-family: monospace; font-size: 12px;")
        .unwrap();
    progress_row.append_child(&progress_outer).unwrap();
    progress_row.append_child(&progress_label).unwrap();
    progress_row.append_child(&phase_label).unwrap();
    section.append_child(&progress_row).unwrap();
    section.append_child(&progress).unwrap();

    let status = document
        .create_element("div")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    status.set_text_content(Some("Готово к выбору файла."));
    status.set_attribute("style", "margin-bottom: 8px;").unwrap();
    section.append_child(&status).unwrap();

    let metrics = document
        .create_element("pre")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    metrics
        .set_attribute(
            "style",
            "background:#f6f6f6; padding: 8px; border-radius: 4px; font-size: 13px; white-space: pre-wrap; margin: 0;",
        )
        .unwrap();
    section.append_child(&metrics).unwrap();

    let ui = Ui {
        pick_btn,
        cancel_btn,
        save_btn,
        reset_btn,
        upload_cb,
        upload_btn,
        delete_btn,
        hash_download_btn,
        engine_select,
        progress,
        progress_label,
        phase_label,
        stages: Rc::new(stages),
        phase_cells: Rc::new(phase_cells),
        status,
        metrics,
        preview_original,
        preview_result,
        verify_badge,
    };
    let state = Rc::new(RefCell::new(DemoState::default()));
    (
        section.dyn_into::<HtmlElement>().unwrap(),
        ui,
        state,
    )
}

fn make_button(d: &Document, text: &str, id: &str) -> HtmlButtonElement {
    let b = d
        .create_element("button")
        .unwrap()
        .dyn_into::<HtmlButtonElement>()
        .unwrap();
    b.set_id(id);
    b.set_text_content(Some(text));
    b
}

fn make_preview(
    d: &Document,
    id: &str,
    caption: &str,
) -> (HtmlElement, HtmlImageElement, HtmlSpanElement) {
    let wrap = d
        .create_element("div")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    wrap.set_attribute("style", "text-align: center;").unwrap();

    let cap = d.create_element("div").unwrap();
    cap.set_text_content(Some(caption));
    cap.set_attribute("style", "font-weight: bold; margin-bottom: 4px;")
        .unwrap();
    wrap.append_child(&cap).unwrap();

    let img = d
        .create_element("img")
        .unwrap()
        .dyn_into::<HtmlImageElement>()
        .unwrap();
    img.set_id(id);
    img.set_attribute(
        "style",
        "max-width: 240px; max-height: 240px; min-width: 120px; min-height: 120px; \
         border: 1px solid #ccc; background: #f6f6f6; object-fit: contain;",
    )
    .unwrap();
    wrap.append_child(&img).unwrap();

    // ✅ создаём <span>, чтобы dyn_into::<HtmlSpanElement>() был валиден.
    // `display: block` возвращает блочное поведение, как было у div.
    let badge = d
        .create_element("span")
        .unwrap()
        .dyn_into::<HtmlSpanElement>()
        .unwrap();
    badge
        .set_attribute(
            "style",
            "display: block; font-size: 12px; margin-top: 4px;",
        )
        .unwrap();
    wrap.append_child(&badge).unwrap();

    (wrap, img, badge)
}