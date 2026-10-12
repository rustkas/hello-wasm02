//! Эмуляция сервера для синхронизации: localStorage + искусственная задержка.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, Storage};

use crate::offline_state::OfflineNote;

const SERVER_KEY: &str = "offline_server_notes";

fn storage() -> Result<Storage, JsValue> {
    window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .local_storage()
        .map_err(|e| JsValue::from_str(&format!("localStorage: {e:?}")))?
        .ok_or_else(|| JsValue::from_str("localStorage disabled"))
}

pub fn load_server_notes() -> Vec<OfflineNote> {
    storage()
        .ok()
        .and_then(|s| s.get_item(SERVER_KEY).ok())
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_server_notes(notes: &[OfflineNote]) -> Result<(), JsValue> {
    let raw = serde_json::to_string(notes)
        .map_err(|e| JsValue::from_str(&format!("serialize: {e:?}")))?;
    storage()?
        .set_item(SERVER_KEY, &raw)
        .map_err(|e| JsValue::from_str(&format!("set_item: {e:?}")))
}

async fn delay(ms: i32) -> Result<(), JsValue> {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("delay: {e:?}")))
}

/// Эмулирует push на сервер с задержкой.
pub async fn server_push(notes: Vec<OfflineNote>) -> Result<(), JsValue> {
    delay(200).await?;
    let mut server_notes = load_server_notes();
    for note in notes {
        // Last-write-wins на стороне сервера.
        match server_notes.iter().position(|n| n.id == note.id) {
            Some(pos) => {
                let existing = &server_notes[pos];
                if note.updated_at > existing.updated_at
                    || (note.updated_at == existing.updated_at && note.version > existing.version)
                {
                    server_notes[pos] = note;
                }
            }
            None => server_notes.push(note),
        }
    }
    save_server_notes(&server_notes)
}

/// Эмулирует pull с сервера с задержкой.
pub async fn server_pull() -> Result<Vec<OfflineNote>, JsValue> {
    delay(200).await?;
    Ok(load_server_notes())
}