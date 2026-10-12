//! Тесты событий: click, input, keyboard, mouse, pointer.
use hello_wasm02::events;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Event, EventInit, HtmlElement, KeyboardEvent, MouseEvent, PointerEvent};

wasm_bindgen_test_configure!(run_in_browser);

mod common;

// ============================================================
// Базовые события
// ============================================================

#[wasm_bindgen_test]
fn test_click_event() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let fixture = common::create_fixture();

    let button = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("button")
        .unwrap();
    button.set_text_content(Some("click me"));
    fixture.append_child(&button).unwrap();

    let button: HtmlElement = button.dyn_into().unwrap();

    let clicked = Rc::new(RefCell::new(false));
    let clicked_clone = clicked.clone();

    let closure = Closure::<dyn FnMut(Event)>::new(move |_event: Event| {
        *clicked_clone.borrow_mut() = true;
    });

    button
        .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .unwrap();

    let event = Event::new("click").unwrap();
    button.dispatch_event(&event).unwrap();

    assert!(*clicked.borrow(), "click handler was not called");

    closure.forget();
    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_event_prevent_default() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let fixture = common::create_fixture();

    let link = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("a")
        .unwrap();
    link.set_attribute("href", "https://example.com").unwrap();
    fixture.append_child(&link).unwrap();

    let prevented = Rc::new(RefCell::new(false));
    let prevented_clone = prevented.clone();

    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        event.prevent_default();
        *prevented_clone.borrow_mut() = event.default_prevented();
    });

    link.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .unwrap();

    // ВАЖНО: создаём отменяемое событие (cancelable: true).
    // По умолчанию Event::new создаёт non-cancelable событие,
    // и prevent_default() не имеет эффекта.
    let init = EventInit::new();
    init.set_cancelable(true);
    let event = Event::new_with_event_init_dict("click", &init).unwrap();

    link.dispatch_event(&event).unwrap();

    assert!(*prevented.borrow(), "default should be prevented");

    closure.forget();
    common::cleanup_fixture(&fixture);
}

// ============================================================
// Клавиатурные события
// ============================================================

#[wasm_bindgen_test]
fn test_keyboard_event_dyn_into() {
    let event = KeyboardEvent::new("keydown").unwrap();
    let generic: Event = event.clone().dyn_into().unwrap();

    let keyboard: KeyboardEvent = generic.dyn_into().unwrap();
    assert_eq!(keyboard.type_(), "keydown");
}

#[wasm_bindgen_test]
fn test_key_helpers_on_empty_event() {
    let event = KeyboardEvent::new("keydown").unwrap();

    assert!(!events::is_arrow(&event));
    assert!(!events::is_enter(&event));
    assert!(!events::is_escape(&event));
    assert!(!events::is_arrow_up(&event));
    assert!(!events::is_arrow_down(&event));
}

// ============================================================
// Мышиные события
// ============================================================

#[wasm_bindgen_test]
fn test_mouse_event_dyn_into() {
    let event = MouseEvent::new("click").unwrap();
    let generic: Event = event.clone().dyn_into().unwrap();

    let mouse: MouseEvent = generic.dyn_into().unwrap();
    assert_eq!(mouse.type_(), "click");
}

#[wasm_bindgen_test]
fn test_left_button_check() {
    let event = MouseEvent::new("click").unwrap();
    // button() по умолчанию 0 (левая кнопка)
    assert_eq!(event.button(), 0);
}

// ============================================================
// Pointer-события
// ============================================================

#[wasm_bindgen_test]
fn test_pointer_event_dyn_into() {
    let event = PointerEvent::new("pointerdown").unwrap();
    let generic: Event = event.clone().dyn_into().unwrap();

    let pointer: PointerEvent = generic.dyn_into().unwrap();
    assert_eq!(pointer.type_(), "pointerdown");
}

// ============================================================
// Управление распространением
// ============================================================

#[wasm_bindgen_test]
fn test_stop_propagation() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let fixture = common::create_fixture();

    let parent = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("div")
        .unwrap();
    let child = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("button")
        .unwrap();

    fixture.append_child(&parent).unwrap();
    parent.append_child(&child).unwrap();

    let parent_called = Rc::new(RefCell::new(false));
    let parent_called_clone = parent_called.clone();

    // Обработчик на родителе
    let parent_closure = Closure::<dyn FnMut(Event)>::new(move |_event: Event| {
        *parent_called_clone.borrow_mut() = true;
    });
    parent
        .add_event_listener_with_callback("click", parent_closure.as_ref().unchecked_ref())
        .unwrap();
    parent_closure.forget();

    // Обработчик на ребёнке останавливает распространение
    let child_closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        event.stop_propagation();
    });
    child
        .add_event_listener_with_callback("click", child_closure.as_ref().unchecked_ref())
        .unwrap();
    child_closure.forget();

    // ВАЖНО: событие должно быть bubbling (bubbles: true),
    // иначе оно не поднимется к родителю и тест ничего не проверит.
    let init = EventInit::new();
    init.set_bubbles(true);
    let event = Event::new_with_event_init_dict("click", &init).unwrap();

    child.dispatch_event(&event).unwrap();

    assert!(
        !*parent_called.borrow(),
        "parent should not receive event after stop_propagation"
    );

    common::cleanup_fixture(&fixture);
}
