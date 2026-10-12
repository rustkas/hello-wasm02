use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::Url;

use crate::{converter_ui};
use super::state::{DemoState, Ui};
use super::{metrics, pipeline};

pub fn wire(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    wire_pick(ui, state.clone());
    wire_cancel(ui, state.clone());
    wire_save(ui, state.clone());
    wire_reset(ui, state.clone());
    wire_upload(ui, state.clone());
    wire_delete(ui, state.clone());
    wire_hash_download(ui, state);
}

fn wire_pick(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        let ui = ui_owned.clone();
        let state = state.clone();
        spawn_local(async move { pipeline::run(ui, state).await });
    });
    ui.pick_btn.set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

fn wire_cancel(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        if let Ok(mut s) = state.try_borrow_mut() {
            s.cancel_requested = true;
        }
        ui_owned.status.set_text_content(Some("Запрошена отмена…"));
        ui_owned.cancel_btn.set_disabled(true);
    });
    ui.cancel_btn.set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

fn wire_reset(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        if state.borrow().pipeline_running {
            ui_owned
                .status
                .set_text_content(Some("Нельзя сбросить: pipeline выполняется."));
            return;
        }
        pipeline::release_urls(&state);
        converter_ui::forget_save_handle();
        *state.borrow_mut() = DemoState::default();
        let _ = ui_owned.preview_original.remove_attribute("src");
        let _ = ui_owned.preview_result.remove_attribute("src");
        metrics::reset_stages_and_bar(&ui_owned);
        metrics::update_progress(&ui_owned, 0.0);
        ui_owned.progress_label.set_text_content(Some("0%"));
        ui_owned.status.set_text_content(Some("Сброшено."));
        ui_owned.metrics.set_text_content(Some(""));
        ui_owned.verify_badge.set_text_content(Some(""));
        ui_owned.upload_cb.set_checked(false);
        ui_owned.upload_btn.set_disabled(true);
        ui_owned.delete_btn.set_disabled(true);
        ui_owned.hash_download_btn.set_disabled(true);
        ui_owned.pick_btn.set_disabled(false);
        ui_owned.cancel_btn.set_disabled(true);
        ui_owned.save_btn.set_disabled(true);
    });
    ui.reset_btn.set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

fn wire_save(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        let ui = ui_owned.clone();
        let state = state.clone();
        spawn_local(async move { pipeline::run_save(ui, state).await });
    });
    ui.save_btn.set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

fn wire_upload(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_for_cb = ui.clone();
    let state_for_cb = state.clone();
    let on_cb = Closure::<dyn FnMut()>::new(move || {
        let enabled = ui_for_cb.upload_cb.checked();
        let has = state_for_cb
            .try_borrow()
            .map(|s| s.result_bytes.is_some())
            .unwrap_or(false);
        ui_for_cb.upload_btn.set_disabled(!(enabled && has));
    });
    ui.upload_cb.set_onchange(Some(on_cb.as_ref().unchecked_ref()));
    on_cb.forget();

    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        let ui = ui_owned.clone();
        let state = state.clone();
        spawn_local(async move { pipeline::run_upload(ui, state).await });
    });
    ui.upload_btn.set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

fn wire_delete(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        let ui = ui_owned.clone();
        let state = state.clone();
        spawn_local(async move {
            let upload_id = match state.try_borrow().ok().and_then(|s| s.upload_id.clone()) {
                Some(v) => v,
                None => {
                    ui.status
                        .set_text_content(Some("Нет upload_id — сначала загрузите."));
                    return;
                }
            };
            ui.delete_btn.set_disabled(true);
            ui.status.set_text_content(Some("Удаление с сервера…"));
            let t0 = js_sys::Date::now();
            let url = "http://localhost:3001/api";
            match converter_ui::delete_upload(url, &upload_id).await {
                Ok(true) => ui
                    .status
                    .set_text_content(Some("Файл удалён (removed: true).")),
                Ok(false) => ui
                    .status
                    .set_text_content(Some("Файла уже нет (removed: false) — идемпотентно.")),
                Err(e) => {
                    let msg = e.as_string().unwrap_or_else(|| format!("{:?}", e));
                    ui.status
                        .set_text_content(Some(&format!("Ошибка удаления: {}", msg)));
                }
            }
            state.borrow_mut().delete_ms = js_sys::Date::now() - t0;
            ui.delete_btn.set_disabled(false);
        });
    });
    ui.delete_btn.set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

fn wire_hash_download(ui: &Ui, state: Rc<RefCell<DemoState>>) {
    let ui_owned = ui.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        // ✅ ФИКС: без .clone().into_iter() на String
        let pair = state.try_borrow().ok().and_then(|s| {
            if s.hash_hex.is_empty() {
                None
            } else {
                s.result_name.clone().map(|n| (s.hash_hex.clone(), n))
            }
        });
        let (hash, name) = match pair {
            Some(v) => v,
            None => return,
        };
        let content = format!("{}  {}\n", hash, name);
        let blob = match web_sys::Blob::new_with_str_sequence(&js_sys::Array::of1(
            &JsValue::from_str(&content),
        )) {
            Ok(b) => b,
            Err(_) => return,
        };
        if let Ok(url) = Url::create_object_url_with_blob(&blob) {
            if let Some(win) = web_sys::window() {
                if let Some(doc) = win.document() {
                    if let Ok(a) = doc.create_element("a") {
                        if let Ok(a) = a.dyn_into::<web_sys::HtmlAnchorElement>() {
                            a.set_href(&url);
                            a.set_download(&format!("{}.sha256", name));
                            a.click();
                        }
                    }
                }
            }
            let _ = Url::revoke_object_url(&url);
        }
        ui_owned
            .status
            .set_text_content(Some("SHA-256 выгружен как sidecar-файл."));
    });
    ui.hash_download_btn
        .set_onclick(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
}

/// Выбранный движок.
pub fn current_engine(ui: &Ui) -> super::state::Engine {
    super::state::Engine::from_str(&ui.engine_select.value())
}