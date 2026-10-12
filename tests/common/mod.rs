//! Общие помощники для интеграционных тестов.
//!
//! Каждый тестовый файл подключает этот модуль через `mod common;`.
//! Поскольку разные файлы используют разные функции, часть из них
//! остаётся неиспользуемой в конкретном файле. `dead_code` здесь — норма.
#![allow(dead_code)]

use wasm_bindgen::JsCast;
use web_sys::Element;

/// Создаёт изолированный контейнер `<div>` в `document.body`
/// и возвращает его. Используется как fixture для DOM-тестов.
pub fn create_fixture() -> Element {
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");

    let fixture = document
        .create_element("div")
        .expect("failed to create fixture");

    fixture.set_attribute("data-test", "true").unwrap();
    document
        .body()
        .expect("no body")
        .append_child(&fixture)
        .expect("failed to append fixture");

    fixture
}

/// Удаляет fixture из DOM после теста.
pub fn cleanup_fixture(fixture: &Element) {
    fixture.remove();
}

/// Утверждение: элемент содержит дочерний узел с заданным тегом.
pub fn assert_has_child(parent: &Element, tag: &str) {
    let children = parent.children();
    let found = (0..children.length()).any(|i: u32| {
        children
            .item(i)
            .and_then(|node| node.dyn_into::<Element>().ok())
            .map(|el| el.tag_name().eq_ignore_ascii_case(tag))
            .unwrap_or(false)
    });
    assert!(found, "expected child <{}> in fixture", tag);
}

/// Приводит `Element` к `T` через `dyn_into`.
///
/// # Panics
///
/// Паникует, если приведение не удалось.
pub fn cast<T: JsCast>(el: &Element) -> T {
    el.clone()
        .dyn_into::<T>()
        .expect("failed to cast element to expected type")
}