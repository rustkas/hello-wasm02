/// Тесты чистого Rust-ядра — без браузера.
///
/// Запускаются через `wasm-pack test --node`.
///
/// ВАЖНО: не добавляйте сюда `wasm_bindgen_test_configure!(run_in_browser)`.
/// Иначе тесты будут пропускаться в Node.js.
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn test_pure_logic() {
    // Здесь будут тесты чистой логики:
    // парсинг, вычисления, сериализация, валидация.
    assert_eq!(2 + 2, 4);
}