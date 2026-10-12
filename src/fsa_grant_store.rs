//! Сохранение handle в IndexedDB и восстановление.
//!
//! IDB хранит FileSystemDirectoryHandle как структурированный объект,
//! но **разрешение не сохраняется**: после перезагрузки
//! `queryPermission` на восстановленном handle вернёт `prompt`.
//!
//! Это типовой паттерн реальных приложений: при старте нужно показать
//! пользователю «Reconnect to folder» как **отдельное действие с кликом**,
//! а не молча восстанавливать handle.

use idb::{
    Database, DatabaseEvent, Factory, ObjectStoreParams, TransactionMode,
};
use wasm_bindgen::prelude::*;

use crate::fsa_choose_options::format_str;

const DB_NAME: &str = "fsa_grant_demo";
const STORE_NAME: &str = "handles";
const DB_VERSION: u32 = 1;

async fn open_db() -> Result<Database, JsValue> {
    let factory = Factory::new().map_err(|e| {
        JsValue::from_str(&format_str(format_args!("Factory: {e:?}")))
    })?;

    let mut open_req = factory.open(DB_NAME, Some(DB_VERSION)).map_err(|e| {
        JsValue::from_str(&format_str(format_args!("open: {e:?}")))
    })?;

    open_req.on_upgrade_needed(|event| {
        let result: Result<(), idb::Error> = (|| {
            let db = event.database()?;
            let names = db.store_names();
            if !names.iter().any(|n| n == STORE_NAME) {
                db.create_object_store(STORE_NAME, ObjectStoreParams::new())?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            let msg = format_str(format_args!("upgrade: {e:?}"));
            web_sys::console::error_1(&msg.into());
        }
    });

    let db = open_req.await.map_err(|e| {
        JsValue::from_str(&format_str(format_args!("open await: {e:?}")))
    })?;
    return Ok(db);
}

/// Сохранить handle под указанным ключом.
pub async fn save_handle(key: &str, handle: &JsValue) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE_NAME], TransactionMode::ReadWrite)
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("tx: {e:?}")))
        })?;
    let store = tx.object_store(STORE_NAME).map_err(|e| {
        JsValue::from_str(&format_str(format_args!("store: {e:?}")))
    })?;

    // Ключ и значение передаём по владению — иначе не сработает Into<Query>.
    let key_js = JsValue::from_str(key);
    store
        .put(handle, Some(&key_js))
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("put: {e:?}")))
        })?
        .await
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("put await: {e:?}")))
        })?;
    return Ok(());
}

/// Загрузить handle. `Ok(None)`, если записи нет.
pub async fn load_handle(key: &str) -> Result<Option<JsValue>, JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE_NAME], TransactionMode::ReadOnly)
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("tx: {e:?}")))
        })?;
    let store = tx.object_store(STORE_NAME).map_err(|e| {
        JsValue::from_str(&format_str(format_args!("store: {e:?}")))
    })?;

    // `get` принимает impl Into<Query>. `Query: From<JsValue>` есть,
    // `From<&JsValue>` — нет. Передаём владение.
    let key_js = JsValue::from_str(key);
    let opt: Option<JsValue> = store
        .get(key_js)
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("get: {e:?}")))
        })?
        .await
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("get await: {e:?}")))
        })?;
    return Ok(opt);
}

/// Удалить запись.
pub async fn clear_handle(key: &str) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[STORE_NAME], TransactionMode::ReadWrite)
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("tx: {e:?}")))
        })?;
    let store = tx.object_store(STORE_NAME).map_err(|e| {
        JsValue::from_str(&format_str(format_args!("store: {e:?}")))
    })?;

    // `delete` — та же история: `Query: From<JsValue>`, не `From<&JsValue>`.
    let key_js = JsValue::from_str(key);
    store
        .delete(key_js)
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("delete: {e:?}")))
        })?
        .await
        .map_err(|e| {
            JsValue::from_str(&format_str(format_args!("delete await: {e:?}")))
        })?;
    return Ok(());
}