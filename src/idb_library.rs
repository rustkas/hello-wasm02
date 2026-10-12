//! Практический проект Главы 24: каталог домашней библиотеки.
//!
//! Модель:
//!   authors: { id, name }
//!   books:   { isbn, title, author_id, year, cover_mime, cover_bytes }
//!
//! Индексы на books: `by_author`, `by_year`.
//! Удаление автора каскадно удаляет его книги в одной транзакции.

use idb::{Query, TransactionMode};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::idb_core::{
    self, IDX_BOOKS_BY_AUTHOR, IDX_BOOKS_BY_YEAR, STORE_AUTHORS, STORE_BOOKS,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Author {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Book {
    pub isbn: String,
    pub title: String,
    pub author_id: String,
    pub year: u32,
    pub cover_mime: String,
    pub cover_bytes: Vec<u8>,
}

impl Author {
    pub fn new(id: &str, name: &str) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Book {
    pub fn new(isbn: &str, title: &str, author_id: &str, year: u32) -> Self {
        Self {
            isbn: isbn.into(),
            title: title.into(),
            author_id: author_id.into(),
            year,
            cover_mime: String::new(),
            cover_bytes: Vec::new(),
        }
    }
}

// ---------------- authors ----------------

pub async fn put_author(a: &Author) -> Result<(), JsValue> {
    let db = idb_core::open().await?;
    let tx = idb_core::write_tx(&db, &[STORE_AUTHORS])?;
    let store = tx.object_store(STORE_AUTHORS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let value = serde_wasm_bindgen::to_value(a)
        .map_err(|e| JsValue::from_str(&format!("to_value: {e:?}")))?;
    store.put(&value, None)
        .map_err(|e| JsValue::from_str(&format!("put: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(())
}

pub async fn list_authors() -> Result<Vec<Author>, JsValue> {
    let db = idb_core::open().await?;
    let tx = db.transaction(&[STORE_AUTHORS], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx.object_store(STORE_AUTHORS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let values: Vec<JsValue> = store.get_all(None, None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await: {e:?}")))?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        out.push(serde_wasm_bindgen::from_value(v)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?);
    }
    Ok(out)
}

// ---------------- books ----------------

pub async fn put_book(b: &Book) -> Result<(), JsValue> {
    let db = idb_core::open().await?;
    let tx = idb_core::write_tx(&db, &[STORE_BOOKS])?;
    let store = tx.object_store(STORE_BOOKS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let value = serde_wasm_bindgen::to_value(b)
        .map_err(|e| JsValue::from_str(&format!("to_value: {e:?}")))?;
    store.put(&value, None)
        .map_err(|e| JsValue::from_str(&format!("put: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(())
}

pub async fn list_books() -> Result<Vec<Book>, JsValue> {
    let db = idb_core::open().await?;
    let tx = db.transaction(&[STORE_BOOKS], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx.object_store(STORE_BOOKS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let values: Vec<JsValue> = store.get_all(None, None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await: {e:?}")))?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        out.push(serde_wasm_bindgen::from_value(v)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?);
    }
    Ok(out)
}

/// Выборка книг по автору через индекс `by_author`.
pub async fn books_by_author(author_id: &str) -> Result<Vec<Book>, JsValue> {
    let db = idb_core::open().await?;
    let tx = db.transaction(&[STORE_BOOKS], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx.object_store(STORE_BOOKS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let idx = store.index(IDX_BOOKS_BY_AUTHOR)
        .map_err(|e| JsValue::from_str(&format!("index: {e:?}")))?;
    let query = Query::Key(JsValue::from_str(author_id));
    let values: Vec<JsValue> = idx.get_all(Some(query), None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await: {e:?}")))?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        out.push(serde_wasm_bindgen::from_value(v)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?);
    }
    Ok(out)
}

/// Книги, изданные в указанном году (по индексу `by_year`).
pub async fn books_by_year(year: u32) -> Result<Vec<Book>, JsValue> {
    let db = idb_core::open().await?;
    let tx = db.transaction(&[STORE_BOOKS], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("tx: {e:?}")))?;
    let store = tx.object_store(STORE_BOOKS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    let idx = store.index(IDX_BOOKS_BY_YEAR)
        .map_err(|e| JsValue::from_str(&format!("index: {e:?}")))?;
    let query = Query::Key(JsValue::from_f64(year as f64));
    let values: Vec<JsValue> = idx.get_all(Some(query), None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await: {e:?}")))?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        out.push(serde_wasm_bindgen::from_value(v)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?);
    }
    Ok(out)
}

// ---------------- удаление и каскад ----------------

/// Удаляет книгу. Идемпотентно.
pub async fn remove_book(isbn: &str) -> Result<(), JsValue> {
    let db = idb_core::open().await?;
    let tx = idb_core::write_tx(&db, &[STORE_BOOKS])?;
    let store = tx.object_store(STORE_BOOKS)
        .map_err(|e| JsValue::from_str(&format!("store: {e:?}")))?;
    store.delete(JsValue::from_str(isbn))
        .map_err(|e| JsValue::from_str(&format!("delete: {e:?}")))?;
    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(())
}

/// Удаляет автора вместе с его книгами в одной транзакции.
/// Возвращает число удалённых книг. Идемпотентно.
pub async fn remove_author_cascade(author_id: &str) -> Result<u32, JsValue> {
    let db = idb_core::open().await?;
    let tx = idb_core::write_tx(&db, &[STORE_AUTHORS, STORE_BOOKS])?;

    let authors = tx.object_store(STORE_AUTHORS)
        .map_err(|e| JsValue::from_str(&format!("authors store: {e:?}")))?;
    let books = tx.object_store(STORE_BOOKS)
        .map_err(|e| JsValue::from_str(&format!("books store: {e:?}")))?;
    let idx = books.index(IDX_BOOKS_BY_AUTHOR)
        .map_err(|e| JsValue::from_str(&format!("index: {e:?}")))?;

    let query = Query::Key(JsValue::from_str(author_id));
    let values: Vec<JsValue> = idx.get_all(Some(query), None)
        .map_err(|e| JsValue::from_str(&format!("get_all: {e:?}")))?
        .await
        .map_err(|e| JsValue::from_str(&format!("await: {e:?}")))?;

    let mut removed: u32 = 0;
    for v in values {
        let book: Book = serde_wasm_bindgen::from_value(v)
            .map_err(|e| JsValue::from_str(&format!("from_value: {e:?}")))?;
        books.delete(JsValue::from_str(&book.isbn))
            .map_err(|e| JsValue::from_str(&format!("delete: {e:?}")))?;
        removed += 1;
    }

    authors.delete(JsValue::from_str(author_id))
        .map_err(|e| JsValue::from_str(&format!("delete author: {e:?}")))?;

    tx.commit()
        .map_err(|e| JsValue::from_str(&format!("commit: {e:?}")))?;
    Ok(removed)
}

// ---------------- binary data ----------------

/// Загружает байты из Blob и собирает Book с обложкой.
pub async fn book_with_cover(
    isbn: &str,
    title: &str,
    author_id: &str,
    year: u32,
    cover: &web_sys::Blob,
) -> Result<Book, JsValue> {
    let buf = wasm_bindgen_futures::JsFuture::from(cover.array_buffer()).await?;
    let array_buffer: js_sys::ArrayBuffer = buf
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
    let mut b = Book::new(isbn, title, author_id, year);
    b.cover_mime = cover.type_();
    b.cover_bytes = bytes;
    Ok(b)
}

/// Восстанавливает Blob из Book (для показа в UI).
pub fn cover_blob(b: &Book) -> Result<web_sys::Blob, JsValue> {
    let uint8 = js_sys::Uint8Array::from(&b.cover_bytes[..]);
    let parts = js_sys::Array::new();
    parts.push(&uint8.buffer());
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(&b.cover_mime);
    web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
}