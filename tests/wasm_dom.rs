//! Тесты, требующие реального браузера (DOM, форма).
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

mod common;

use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, HtmlInputElement, HtmlSelectElement};

#[wasm_bindgen_test]
fn test_document_exists() {
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    assert!(document.body().is_some());
}

#[wasm_bindgen_test]
fn test_create_paragraph() {
    let fixture = common::create_fixture();

    let p = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("p")
        .unwrap();
    p.set_text_content(Some("test paragraph"));
    fixture.append_child(&p).unwrap();

    common::assert_has_child(&fixture, "p");

    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_set_attribute() {
    let fixture = common::create_fixture();

    fixture.set_attribute("data-role", "test").unwrap();
    assert_eq!(fixture.get_attribute("data-role").unwrap(), "test");

    fixture.remove_attribute("data-role").unwrap();
    assert!(fixture.get_attribute("data-role").is_none());

    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_class_list() {
    let fixture = common::create_fixture();
    let html: HtmlElement = fixture.clone().unchecked_into();

    html.class_list().add_1("foo").unwrap();
    html.class_list().add_1("bar").unwrap();

    assert!(html.class_list().contains("foo"));
    assert!(html.class_list().contains("bar"));
    assert!(!html.class_list().contains("baz"));

    html.class_list().remove_1("foo").unwrap();
    assert!(!html.class_list().contains("foo"));
    assert!(html.class_list().contains("bar"));

    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_query_selector() {
    let fixture = common::create_fixture();

    let child = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("span")
        .unwrap();
    child.set_attribute("data-test-child", "yes").unwrap();
    fixture.append_child(&child).unwrap();

    let found = fixture
        .query_selector("[data-test-child='yes']")
        .unwrap()
        .unwrap();
    assert_eq!(found.tag_name(), "SPAN");

    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_input_element_dyn_into() {
    let fixture = common::create_fixture();

    let input = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("input")
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();

    input.set_type("text");
    input.set_value("hello");

    assert_eq!(input.value(), "hello");
    assert_eq!(input.type_(), "text");

    fixture.append_child(&input).unwrap();

    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_select_element_options() {
    let fixture = common::create_fixture();

    let document = web_sys::window().unwrap().document().unwrap();

    let select = document
        .create_element("select")
        .unwrap()
        .dyn_into::<HtmlSelectElement>()
        .unwrap();

    for (value, text) in [("a", "Option A"), ("b", "Option B")] {
        let option = document.create_element("option").unwrap();
        option.set_attribute("value", value).unwrap();
        option.set_text_content(Some(text));
        select.add_with_html_option_element(
            &option.dyn_into::<web_sys::HtmlOptionElement>().unwrap(),
        ).unwrap();
    }

    fixture.append_child(&select).unwrap();

    assert_eq!(select.options().length(), 2);
    assert_eq!(select.value(), "a");

    common::cleanup_fixture(&fixture);
}