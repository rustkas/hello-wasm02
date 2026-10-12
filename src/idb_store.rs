//! IndexedDB: структурированное хранилище через crate `idb`.
//!
//! **Модель.** Открываем базу (`hello-wasm-db`) с одним object
//! store (`files`). В store хранятся записи `{ id, name, mime,
//! bytes }`. IndexedDB хранит их нативно как JS-объекты,
//! `bytes` — как `Uint8Array`.
//!
//! **Про сериализацию.** Crate `idb` работает с `JsValue`, а не
//! с Rust-структурами. Преобразование в обе стороны — через
//! `serde_wasm_bindgen::to_value` / `from_value`. Это добавляет
//! одну копию байтов при чтении, но упрощает код и даёт типовую
//! безопасность на уровне Rust.

use idb::{
    Database, DatabaseEvent, Factory, KeyPath, ObjectStoreParams,
    TransactionMode,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::Blob;

/// Запись, хранимая в IndexedDB.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredEntry {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Открывает базу данных с одним object store `files`.
pub async fn open_db() -> Result<Database, JsValue> {
    let factory = Factory::new()
        .map_err(|e| JsValue::from_str(&format!("Factory::new: {e:?}")))?;

    let mut open_request = factory
        .open("hello-wasm-db", Some(1))
        .map_err(|e| JsValue::from_str(&format!("open: {e:?}")))?;

    // Явная аннотация типа `db: Database` внутри замыкания —
    // компилятор не может вывести его сам.
    open_request.on_upgrade_needed(|event| {
        let db: Database = match event.database() {
            Ok(d) => d,
            Err(_) => return,
        };
        let mut params = ObjectStoreParams::new();
        params.key_path(Some(KeyPath::new_single("id")));
        // `create_object_store` возвращает `Result` — для демо
        // игнорируем ошибку, если store уже существует.
        let _ = db.create_object_store("files", params);
    });

    let db = open_request
        .await
        .map_err(|e| JsValue::from_str(&format!("await open: {e:?}")))?;
    return Ok(db);
}

/// Сохраняет запись в IndexedDB. Перезаписывает по ключу `id`.
pub async fn put_entry(entry: &StoredEntry) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&["files"], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store("files")
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;

    // Сериализация StoredEntry → JsValue.
    // `store.put` принимает `&JsValue`, не Rust-структуру.
    let js_value = serde_wasm_bindgen::to_value(entry)
        .map_err(|e| JsValue::from_str(&format!("to_value: {e:?}")))?;

    store
        .put(&js_value, None)
        .map_err(|e| JsValue::from_str(&format!("put: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    return Ok(());
}

/// Читает запись по `id`.
pub async fn get_entry(id: &str) -> Result<Option<StoredEntry>, JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&["files"], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store("files")
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;

    let key: JsValue = JsValue::from_str(id);

    // idb 0.6.5: store.get() возвращает Option<JsValue>,
    // а не JsValue. Отсутствие записи = None.
    let value_opt: Option<JsValue> = store
        .get(key)
        .map_err(|e| JsValue::from_str(&format!("get: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await get: {e:?}")))?;

    let value = match value_opt {
        Some(v) => v,
        None => return Ok(None),
    };

    let entry: StoredEntry = serde_wasm_bindgen::from_value(value)
        .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?;
    return Ok(Some(entry));
}

/// Возвращает все записи в store.
pub async fn list_entries() -> Result<Vec<StoredEntry>, JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&["files"], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store("files")
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;

    // idb 0.6.5: store.get_all() возвращает Vec<JsValue>,
    // а не единый JsValue. Десериализуем каждую запись отдельно.
    let values: Vec<JsValue> = store
        .get_all(None, None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await get_all: {e:?}")))?;

    let mut entries: Vec<StoredEntry> = Vec::with_capacity(values.len());
    for value in values {
        let entry: StoredEntry = serde_wasm_bindgen::from_value(value)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?;
        entries.push(entry);
    }
    return Ok(entries);
}

/// Удаляет запись по `id`. Идемпотентна.
pub async fn delete_entry(id: &str) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&["files"], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx
        .object_store("files")
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;

    let key: JsValue = JsValue::from_str(id);
    store
        .delete(key)
        .map_err(|e| JsValue::from_str(&format!("delete: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    return Ok(());
}

/// Создаёт `StoredEntry` из Blob.
pub async fn entry_from_blob(
    id: &str,
    name: &str,
    blob: &Blob,
) -> Result<StoredEntry, JsValue> {
    let array_promise = blob.array_buffer();
    let value = wasm_bindgen_futures::JsFuture::from(array_promise).await?;
    let array_buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
    return Ok(StoredEntry {
        id: id.to_string(),
        name: name.to_string(),
        mime: blob.type_(),
        bytes,
    });
}

/// Восстанавливает Blob из `StoredEntry`.
pub fn blob_from_entry(entry: &StoredEntry) -> Result<Blob, JsValue> {
    let uint8 = js_sys::Uint8Array::from(&entry.bytes[..]);
    let parts = js_sys::Array::new();
    parts.push(&uint8.buffer());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(&entry.mime);
    return Blob::new_with_u8_array_sequence_and_options(&parts, &options);
}