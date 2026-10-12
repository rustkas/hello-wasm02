//! Host-тесты модуля idb_core: константы схемы и сериализация моделей.
//! Никаких JsValue/IndexedDB здесь быть не должно — только чистая Rust-логика.
//! Реальные операции с IndexedDB — в tests/idb_library_test.rs (wasm-таргет).

use hello_wasm02::idb_core::{
    DB_NAME, DB_VERSION, IDX_BOOKS_BY_AUTHOR, IDX_BOOKS_BY_YEAR,
    STORE_AUTHORS, STORE_BOOKS,
};
use hello_wasm02::idb_library::{Author, Book};

#[test]
fn db_constants_are_stable() {
    assert_eq!(DB_NAME, "hello-wasm-library");
    assert_eq!(DB_VERSION, 1);
    assert_eq!(STORE_AUTHORS, "authors");
    assert_eq!(STORE_BOOKS, "books");
    assert_eq!(IDX_BOOKS_BY_AUTHOR, "by_author");
    assert_eq!(IDX_BOOKS_BY_YEAR, "by_year");
}

#[test]
fn author_json_roundtrip() {
    let a = Author::new("tolstoy", "Лев Толстой");
    let s = serde_json::to_string(&a).unwrap();
    let back: Author = serde_json::from_str(&s).unwrap();
    assert_eq!(a, back);
}

#[test]
fn book_json_roundtrip_with_cover() {
    let mut b = Book::new("978-5-04-118457-7", "Война и мир", "tolstoy", 1869);
    b.cover_mime = "image/png".into();
    b.cover_bytes = vec![0x89, 0x50, 0x4E, 0x47];
    let s = serde_json::to_string(&b).unwrap();
    let back: Book = serde_json::from_str(&s).unwrap();
    assert_eq!(b, back);
    assert_eq!(back.cover_bytes, vec![0x89, 0x50, 0x4E, 0x47]);
}