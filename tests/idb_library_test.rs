//! Wasm-тесты практического проекта Главы 24 (IndexedDB).
//! Компилируются только под wasm32 — на host файл пуст.
//!
//! Тесты соответствуют публичному API `idb_library`:
//!   put_author / list_authors
//!   put_book   / list_books
//!   books_by_author (индекс by_author)
//!   books_by_year   (индекс by_year)
//!   remove_book / remove_author_cascade (идемпотентно)
//!   book_with_cover / cover_blob (binary data)

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use web_sys::Blob;

use hello_wasm02::idb_library::{
    book_with_cover, books_by_author, books_by_year, cover_blob, list_authors,
    list_books, put_author, put_book, remove_author_cascade, remove_book,
    Author, Book,
};

wasm_bindgen_test_configure!(run_in_browser);

/// Уникальный суффикс, чтобы тесты не сталкивались в одной и той же БД
/// при последовательных запусках.
fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64)
}

// ---------------- authors ----------------

#[wasm_bindgen_test]
async fn put_and_list_authors() {
    let id = format!("auth-{}", uid());
    let a = Author::new(&id, "Test Author");
    put_author(&a).await.expect("put_author");

    let list = list_authors().await.expect("list_authors");
    let found = list.iter().find(|x| x.id == id).expect("author present");
    assert_eq!(found.name, "Test Author");

    let _ = remove_author_cascade(&id).await;
}

#[wasm_bindgen_test]
async fn put_author_overwrites_by_id() {
    let id = format!("auth-{}", uid());
    put_author(&Author::new(&id, "First")).await.unwrap();
    put_author(&Author::new(&id, "Second")).await.unwrap();

    let list = list_authors().await.unwrap();
    let found = list.iter().find(|x| x.id == id).expect("author present");
    assert_eq!(found.name, "Second");

    let _ = remove_author_cascade(&id).await;
}

// ---------------- books ----------------

#[wasm_bindgen_test]
async fn put_and_list_books() {
    let isbn = format!("isbn-{}", uid());
    let b = Book::new(&isbn, "Some Book", "any-author", 2020);
    put_book(&b).await.expect("put_book");

    let books = list_books().await.expect("list_books");
    let found = books.iter().find(|x| x.isbn == isbn).expect("book present");
    assert_eq!(found.title, "Some Book");
    assert_eq!(found.year, 2020);

    let _ = remove_book(&isbn).await;
}

// ---------------- indexes ----------------

#[wasm_bindgen_test]
async fn index_books_by_author() {
    let suffix = uid();
    let author = Author::new(&format!("auth-{suffix}"), "Index Author");
    put_author(&author).await.expect("put_author");

    let b1 = Book::new(&format!("isbn-a-{suffix}"), "Книга 1", &author.id, 2000);
    let b2 = Book::new(&format!("isbn-b-{suffix}"), "Книга 2", &author.id, 2001);
    put_book(&b1).await.expect("put b1");
    put_book(&b2).await.expect("put b2");

    let by_author = books_by_author(&author.id).await.expect("by_author");
    assert_eq!(by_author.len(), 2, "индекс by_author должен вернуть 2 книги");
    assert!(by_author.iter().any(|x| x.isbn == b1.isbn));
    assert!(by_author.iter().any(|x| x.isbn == b2.isbn));

    let _ = remove_author_cascade(&author.id).await;
}

#[wasm_bindgen_test]
async fn index_books_by_year() {
    let suffix = uid();
    let author = Author::new(&format!("auth-{suffix}"), "Year Author");
    put_author(&author).await.unwrap();

    // Год 2000+, разный для каждого запуска — тест не сталкивается сам с собой.
    let year: u32 = 2000 + (suffix.parse::<u32>().unwrap_or(0) % 1000);
    let isbn = format!("isbn-year-{suffix}");
    put_book(&Book::new(&isbn, "Y", &author.id, year)).await.unwrap();

    let by_year = books_by_year(year).await.expect("by_year");
    assert!(by_year.iter().any(|x| x.isbn == isbn));

    let _ = remove_author_cascade(&author.id).await;
}

#[wasm_bindgen_test]
async fn books_by_author_empty_for_unknown() {
    let unknown = format!("no-author-{}", uid());
    let result = books_by_author(&unknown).await.expect("by_author");
    assert!(result.is_empty(), "индекс по неизвестному автору должен вернуть пусто");
}

// ---------------- cascade ----------------

#[wasm_bindgen_test]
async fn cascade_removes_books() {
    let suffix = uid();
    let author = Author::new(&format!("auth-{suffix}"), "Cascade");
    put_author(&author).await.unwrap();
    put_book(&Book::new(&format!("c-1-{suffix}"), "C1", &author.id, 1990)).await.unwrap();
    put_book(&Book::new(&format!("c-2-{suffix}"), "C2", &author.id, 1991)).await.unwrap();

    let removed = remove_author_cascade(&author.id).await.expect("cascade");
    assert_eq!(removed, 2);

    let left = books_by_author(&author.id).await.unwrap();
    assert!(left.is_empty());
    let authors = list_authors().await.unwrap();
    assert!(!authors.iter().any(|a| a.id == author.id));
}

#[wasm_bindgen_test]
async fn cascade_is_idempotent() {
    let r1 = remove_author_cascade("no-such-author").await.expect("first");
    assert_eq!(r1, 0);
    let r2 = remove_author_cascade("no-such-author").await.expect("second");
    assert_eq!(r2, 0);
}

#[wasm_bindgen_test]
async fn remove_book_idempotent() {
    remove_book("no-such-isbn").await.expect("first");
    remove_book("no-such-isbn").await.expect("second");
}

// ---------------- binary data (cover) ----------------

#[wasm_bindgen_test]
async fn cover_blob_roundtrip() {
    let bytes: [u8; 4] = [0x89, 0x50, 0x4E, 0x47]; // PNG-магия
    let blob = make_blob(&bytes, "image/png").expect("make_blob");

    let book = book_with_cover("cover-isbn", "Cover", "auth-1", 2020, &blob)
        .await
        .expect("book_with_cover");
    assert_eq!(book.cover_mime, "image/png");
    assert_eq!(book.cover_bytes, bytes.to_vec());

    let back = cover_blob(&book).expect("cover_blob");
    assert_eq!(back.type_(), "image/png");
    let back_bytes = blob_bytes(&back).await;
    assert_eq!(back_bytes, bytes.to_vec());
}

#[wasm_bindgen_test]
async fn cover_bytes_survive_indexeddb() {
    let suffix = uid();
    let author = Author::new(&format!("cover-auth-{suffix}"), "Cover Author");
    put_author(&author).await.unwrap();

    let bytes: Vec<u8> = (0..32u8).map(|i| i.wrapping_mul(7)).collect();
    let blob = make_blob(&bytes, "image/jpeg").unwrap();

    let book = book_with_cover(
        &format!("cover-isbn-{suffix}"),
        "Book with cover",
        &author.id,
        2021,
        &blob,
    )
    .await
    .unwrap();
    put_book(&book).await.unwrap();

    let loaded = list_books().await.unwrap();
    let found = loaded
        .iter()
        .find(|b| b.isbn == book.isbn)
        .expect("book loaded from IDB");
    assert_eq!(found.cover_mime, "image/jpeg");
    assert_eq!(found.cover_bytes, bytes, "binary data должны сохраниться без искажений");

    let _ = remove_author_cascade(&author.id).await;
}

// ---------------- helpers ----------------

fn make_blob(bytes: &[u8], mime: &str) -> Result<Blob, wasm_bindgen::JsValue> {
    let uint8 = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&uint8.buffer());
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
}

async fn blob_bytes(blob: &Blob) -> Vec<u8> {
    let buf = wasm_bindgen_futures::JsFuture::from(blob.array_buffer())
        .await
        .expect("array_buffer");
    js_sys::Uint8Array::new(&buf).to_vec()
}