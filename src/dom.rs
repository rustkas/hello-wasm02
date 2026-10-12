//! Помощники для работы с DOM.

#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlElement, Node, Window};

// ============================================================
// Получение базовых объектов
// ============================================================

/// Возвращает глобальный объект `window`.
///
/// # Panics
///
/// Паникует, если код выполняется вне браузерного контекста.
///
/// # Пример
///
/// ```no_run
/// use hello_wasm02::dom;
/// let w = dom::window();
/// assert!(w.document().is_some());
/// ```
pub fn window() -> Window {
    web_sys::window().expect("no global `window` exists")
}

/// Возвращает `document` из `window`.
pub fn document() -> Document {
    window()
        .document()
        .expect("should have a document on window")
}

/// Возвращает `<body>` из `document`.
pub fn body() -> HtmlElement {
    document()
        .body()
        .expect("document should have a body")
        .unchecked_into()
}

// ============================================================
// Поиск элементов
// ============================================================

/// Ищет элемент по CSS-селектору. Возвращает первый совпавший.
pub fn query_selector(selector: &str) -> Result<Option<Element>, JsValue> {
    document().query_selector(selector)
}

/// Ищет все элементы по CSS-селектору.
pub fn query_selector_all(selector: &str) -> Result<web_sys::NodeList, JsValue> {
    document().query_selector_all(selector)
}

/// Ищет элемент по `id`. Возвращает `None`, если не найден.
pub fn get_element_by_id(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}

// ============================================================
// Создание элементов
// ============================================================

/// Создаёт элемент с заданным тегом.
pub fn create_element(tag: &str) -> Result<Element, JsValue> {
    document().create_element(tag)
}

/// Создаёт `<p>` с заданным текстом.
pub fn create_paragraph(text: &str) -> Result<Element, JsValue> {
    let p = create_element("p")?;
    p.set_text_content(Some(text));
    Ok(p)
}

/// Создаёт `<h1>`–`<h6>` с заданным текстом.
pub fn create_heading(level: u8, text: &str) -> Result<Element, JsValue> {
    let tag = format!("h{}", level.clamp(1, 6));
    let h = create_element(&tag)?;
    h.set_text_content(Some(text));
    Ok(h)
}

/// Создаёт `<li>` с заданным текстом.
pub fn create_list_item(text: &str) -> Result<Element, JsValue> {
    let li = create_element("li")?;
    li.set_text_content(Some(text));
    Ok(li)
}

// ============================================================
// Вложенность DOM
// ============================================================

/// Добавляет дочерний узел к родителю.
pub fn append(parent: &Node, child: &Node) -> Result<(), JsValue> {
    parent.append_child(child)?;
    Ok(())
}

/// Удаляет элемент из DOM.
pub fn remove(el: &Element) {
    el.remove();
}

/// Удаляет произвольный узел из DOM через родителя.
pub fn remove_node(node: &Node) -> Result<(), JsValue> {
    if let Some(parent) = node.parent_node() {
        parent.remove_child(node)?;
    }
    Ok(())
}

// ============================================================
// Атрибуты
// ============================================================

/// Устанавливает атрибут на элементе.
pub fn set_attribute(el: &Element, name: &str, value: &str) -> Result<(), JsValue> {
    el.set_attribute(name, value)
}

/// Читает атрибут. Возвращает `None`, если атрибута нет.
pub fn get_attribute(el: &Element, name: &str) -> Option<String> {
    el.get_attribute(name)
}

/// Удаляет атрибут.
pub fn remove_attribute(el: &Element, name: &str) -> Result<(), JsValue> {
    el.remove_attribute(name)
}

// ============================================================
// Классы
// ============================================================

/// Добавляет один или несколько CSS-классов.
///
/// Строка разбивается по пробелам. Это защищает от `DOMException`,
/// который возникает при передаче строки с пробелом в `classList.add`.
pub fn add_class(el: &HtmlElement, class: &str) -> Result<(), JsValue> {
    let class_list = el.class_list();
    for token in class.split_whitespace() {
        class_list.add_1(token)?;
    }
    Ok(())
}

/// Удаляет один или несколько CSS-классов.
///
/// Строка разбивается по пробелам — аналогично `add_class`.
pub fn remove_class(el: &HtmlElement, class: &str) -> Result<(), JsValue> {
    let class_list = el.class_list();
    for token in class.split_whitespace() {
        class_list.remove_1(token)?;
    }
    Ok(())
}

/// Проверяет наличие CSS-класса.
pub fn has_class(el: &HtmlElement, class: &str) -> bool {
    el.class_list().contains(class)
}

// ============================================================
// Приведение типов
// ============================================================

/// Приводит `Element` к `HtmlElement` без проверки типа.
///
/// # Panics
///
/// Паникует, если элемент не является `HtmlElement`.
pub fn as_html(el: &Element) -> HtmlElement {
    el.clone().unchecked_into()
}

/// Безопасно пытается привести `Element` к `HtmlElement`.
pub fn try_as_html(el: &Element) -> Option<HtmlElement> {
    el.clone().dyn_into::<HtmlElement>().ok()
}