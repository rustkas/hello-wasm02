//! Тесты чистой логики, которые не зависят от браузера.
//!
//! Запускаются через `cargo test` на хосте.
//! Не используют `web_sys`, `wasm_bindgen` или другие
//! Wasm-специфичные API.

#[test]
fn test_arithmetic() {
    assert_eq!(2 + 2, 4);
}

#[test]
fn test_string_processing() {
    let input = "hello";
    let output = input.to_uppercase();
    assert_eq!(output, "HELLO");
}