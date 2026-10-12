//! IndexedDB: открытие БД, upgrade-схема, общие константы.
//! Автономный модуль — не зависит от других модулей проекта.

use idb::{
    Database, DatabaseEvent, Factory, KeyPath, ObjectStoreParams, TransactionMode,
};
use wasm_bindgen::prelude::*;

pub const DB_NAME: &str = "hello-wasm-library";
pub const DB_VERSION: u32 = 1;

pub const STORE_AUTHORS: &str = "authors";
pub const STORE_BOOKS: &str = "books";

pub const IDX_BOOKS_BY_AUTHOR: &str = "by_author";
pub const IDX_BOOKS_BY_YEAR: &str = "by_year";

/// Открывает БД `hello-wasm-library` v1.
/// При первом открытии создаёт stores `authors`, `books`
/// и индексы `by_author`, `by_year` на `books`.
pub async fn open() -> Result<Database, JsValue> {
    let factory = Factory::new()
        .map_err(|e| JsValue::from_str(&format!("Factory::new: {e:?}")))?;

    let mut request = factory
        .open(DB_NAME, Some(DB_VERSION))
        .map_err(|e| JsValue::from_str(&format!("open: {e:?}")))?;

    // `DatabaseEvent` — это трейт (в idb 0.6.5), а не структура.
    // Аннотацию типа для параметра замыкания писать нельзя —
    // тип выводится. Импорт трейта нужен, чтобы в scope был
    // метод `event.database()`.
    request.on_upgrade_needed(|event| {
        let db: Database = match event.database() {
            Ok(d) => d,
            Err(_) => return,
        };

        // store `authors` — ключ `id`
        let mut authors_params = ObjectStoreParams::new();
        authors_params.key_path(Some(KeyPath::new_single("id")));
        let _ = db.create_object_store(STORE_AUTHORS, authors_params);

        // store `books` — ключ `isbn`, два индекса
        let mut books_params = ObjectStoreParams::new();
        books_params.key_path(Some(KeyPath::new_single("isbn")));
        if let Ok(store) = db.create_object_store(STORE_BOOKS, books_params) {
            let _ = store.create_index(
                IDX_BOOKS_BY_AUTHOR,
                KeyPath::new_single("author_id"),
                None,
            );
            let _ = store.create_index(
                IDX_BOOKS_BY_YEAR,
                KeyPath::new_single("year"),
                None,
            );
        }
    });

    request
        .await
        .map_err(|e| JsValue::from_str(&format!("await open: {e:?}")))
}

/// ReadWrite-транзакция на произвольный набор stores.
///
/// В idb 0.6.5 `idb::Transaction` — без generic-параметров
/// времени жизни.
pub fn write_tx(
    db: &Database,
    stores: &[&str],
) -> Result<idb::Transaction, JsValue> {
    db.transaction(stores, TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))
}