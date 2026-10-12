//! Персистентность offline-заметок в IndexedDB.

use idb::{
    Database, DatabaseEvent, Factory, KeyPath, ObjectStoreParams, TransactionMode,
};
use wasm_bindgen::prelude::*;

use crate::offline_state::OfflineNote;

const DB_NAME: &str = "offline-notes-db";
const DB_VERSION: u32 = 1;
const STORE: &str = "notes";

async fn open_db() -> Result<Database, JsValue> {
    let factory = Factory::new()
        .map_err(|e| JsValue::from_str(&format!("Factory: {e:?}")))?;
    let mut request = factory
        .open(DB_NAME, Some(DB_VERSION))
        .map_err(|e| JsValue::from_str(&format!("open: {e:?}")))?;
    request.on_upgrade_needed(|event| {
        // `event.database()` — метод трейта `DatabaseEvent`.
        let db: Database = match event.database() {
            Ok(d) => d,
            Err(_) => return,
        };
        let mut params = ObjectStoreParams::new();
        params.key_path(Some(KeyPath::new_single("id")));
        let _ = db.create_object_store(STORE, params);
    });
    request
        .await
        .map_err(|e| JsValue::from_str(&format!("await open: {e:?}")))
}

pub async fn save_note(note: &OfflineNote) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let value = serde_wasm_bindgen::to_value(note)
        .map_err(|e| JsValue::from_str(&format!("to_value: {e:?}")))?;
    store
        .put(&value, None)
        .map_err(|e| JsValue::from_str(&format!("put: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(())
}

pub async fn load_all_notes() -> Result<Vec<OfflineNote>, JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let values: Vec<JsValue> = store
        .get_all(None, None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await get_all: {e:?}")))?;
    let mut notes = Vec::with_capacity(values.len());
    for v in values {
        let note: OfflineNote = serde_wasm_bindgen::from_value(v)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?;
        notes.push(note);
    }
    Ok(notes)
}

pub async fn delete_note(id: &str) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    store
        .delete(JsValue::from_str(id))
        .map_err(|e| JsValue::from_str(&format!("delete: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(())
}

/// Физическое удаление из IndexedDB (после успешной синхронизации tombstone).
pub async fn hard_delete_note(id: &str) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    store
        .delete(JsValue::from_str(id))
        .map_err(|e| JsValue::from_str(&format!("delete: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(())
}