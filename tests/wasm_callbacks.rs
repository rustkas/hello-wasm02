//! Тесты для `Closure` и callback API.
use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::Event;                     // ← HtmlElement убран

use hello_wasm02::callbacks::{self, EventClosures};

wasm_bindgen_test_configure!(run_in_browser);

mod common;

// ============================================================
// Closure: базовое использование
// ============================================================

#[wasm_bindgen_test]
fn test_closure_captures_state() {
    let counter = Rc::new(RefCell::new(0));
    let counter_clone = counter.clone();

    let closure = Closure::<dyn FnMut()>::new(move || {
        *counter_clone.borrow_mut() += 1;
    });

    // Приводим Closure к Function и вызываем
    let f: &js_sys::Function = closure.as_ref().unchecked_ref();
    f.call0(&JsValue::NULL).unwrap();
    f.call0(&JsValue::NULL).unwrap();
    f.call0(&JsValue::NULL).unwrap();

    assert_eq!(*counter.borrow(), 3);
}

#[wasm_bindgen_test]
fn test_closure_with_arg() {
    let collected = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected_clone = collected.clone();

    let closure = Closure::<dyn FnMut(String)>::new(move |s: String| {
        collected_clone.borrow_mut().push(s);
    });

    let f: &js_sys::Function = closure.as_ref().unchecked_ref();
    f.call1(&JsValue::NULL, &JsValue::from_str("a")).unwrap();
    f.call1(&JsValue::NULL, &JsValue::from_str("b")).unwrap();

    let items = collected.borrow();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], "a");
    assert_eq!(items[1], "b");
}

// ============================================================
// Closure: время жизни
// ============================================================

#[wasm_bindgen_test]
fn test_closure_dropped() {
    let counter = Rc::new(RefCell::new(0));
    let counter_clone = counter.clone();

    {
        let _closure = Closure::<dyn FnMut()>::new(move || {
            *counter_clone.borrow_mut() += 1;
        });
        // _closure удаляется при выходе из блока
    }

    // Счётчик остаётся 0: замыкание не было вызвано
    // и не будет доступно после drop.
    assert_eq!(*counter.borrow(), 0);
}

#[wasm_bindgen_test]
fn test_closure_forget_persists() {
    let counter = Rc::new(RefCell::new(0));
    let counter_clone = counter.clone();

    let closure = Closure::<dyn FnMut()>::new(move || {
        *counter_clone.borrow_mut() += 1;
    });

    // Сохраняем функцию и «забываем» Closure
let f: js_sys::Function = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
closure.forget();

    // Функция продолжает работать
    f.call0(&JsValue::NULL).unwrap();
    f.call0(&JsValue::NULL).unwrap();

    assert_eq!(*counter.borrow(), 2);
}

// ============================================================
// Fn / FnMut / FnOnce
// ============================================================

#[wasm_bindgen_test]
fn test_fn_immutable() {
    // Fn: замыкание только читает захваченные данные
    let source = "hello";
    let closure = Closure::<dyn Fn() -> String>::new(move || source.to_uppercase());

    let f: &js_sys::Function = closure.as_ref().unchecked_ref();
    let result = f.call0(&JsValue::NULL).unwrap();

    assert_eq!(result.as_string(), Some("HELLO".into()));
}

#[wasm_bindgen_test]
fn test_fn_mut_mutates() {
    // FnMut: замыкание мутирует захваченные данные
    let count = Rc::new(RefCell::new(0));
    let count_clone = count.clone();

    let closure = Closure::<dyn FnMut()>::new(move || {
        *count_clone.borrow_mut() += 1;
    });

    let f: &js_sys::Function = closure.as_ref().unchecked_ref();
    f.call0(&JsValue::NULL).unwrap();
    f.call0(&JsValue::NULL).unwrap();

    assert_eq!(*count.borrow(), 2);
}

#[wasm_bindgen_test]
fn test_fn_once_consumes() {
    // FnOnce: замыкание потребляет захваченные данные
    let data = vec![1, 2, 3];
    let closure = Closure::once(move || {
        let sum: i32 = data.iter().sum();
        JsValue::from_f64(sum as f64)
    });

    let f: &js_sys::Function = closure.as_ref().unchecked_ref();
    let result = f.call0(&JsValue::NULL).unwrap();

    assert_eq!(result.as_f64(), Some(6.0));
    closure.forget();
}

// ============================================================
// EventClosures: контейнер
// ============================================================

#[wasm_bindgen_test]
fn test_event_closures_store() {
    let mut store = EventClosures::new();

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    let closure = Closure::<dyn FnMut(Event)>::new(|_e: Event| {});
    store.push(closure);

    assert!(!store.is_empty());
    assert_eq!(store.len(), 1);

    store.clear();
    assert!(store.is_empty());
}

// ============================================================
// callbacks::listen
// ============================================================

#[wasm_bindgen_test]
fn test_listen_stores_closure() {
    let fixture = common::create_fixture();
    let store = Rc::new(RefCell::new(EventClosures::new()));

    let clicked = Rc::new(RefCell::new(false));
    let clicked_clone = clicked.clone();

    callbacks::listen(
        &fixture,
        "click",
        move |_event| {
            *clicked_clone.borrow_mut() = true;
        },
        &store,
    )
    .unwrap();

    assert_eq!(store.borrow().len(), 1);

    let event = Event::new("click").unwrap();
    fixture.dispatch_event(&event).unwrap();

    assert!(*clicked.borrow());

    common::cleanup_fixture(&fixture);
}

#[wasm_bindgen_test]
fn test_listen_multiple_handlers() {
    let fixture = common::create_fixture();
    let store = Rc::new(RefCell::new(EventClosures::new()));

    let count = Rc::new(RefCell::new(0));

    for _ in 0..3 {
        let count = count.clone();
        callbacks::listen(
            &fixture,
            "click",
            move |_event| {
                *count.borrow_mut() += 1;
            },
            &store,
        )
        .unwrap();
    }

    assert_eq!(store.borrow().len(), 3);

    let event = Event::new("click").unwrap();
    fixture.dispatch_event(&event).unwrap();

    // Все три обработчика вызваны
    assert_eq!(*count.borrow(), 3);

    common::cleanup_fixture(&fixture);
}

// ============================================================
// callbacks::listen_typed
// ============================================================

#[wasm_bindgen_test]
fn test_listen_typed_keyboard() {
    use web_sys::KeyboardEvent;

    let fixture = common::create_fixture();
    let store = Rc::new(RefCell::new(EventClosures::new()));

    let received = Rc::new(RefCell::new(Vec::<String>::new()));
    let received_clone = received.clone();

    callbacks::listen_typed::<KeyboardEvent, _>(
        &fixture,
        "keydown",
        move |event: KeyboardEvent| {
            received_clone.borrow_mut().push(event.key());
        },
        &store,
    )
    .unwrap();

    let event = KeyboardEvent::new("keydown").unwrap();
    fixture.dispatch_event(&event).unwrap();

    assert_eq!(received.borrow().len(), 1);

    common::cleanup_fixture(&fixture);
}

// ============================================================
// callbacks::listen_once
// ============================================================

#[wasm_bindgen_test]
fn test_listen_once() {
    let fixture = common::create_fixture();

    let called = Rc::new(RefCell::new(0));
    let called_clone = called.clone();

    callbacks::listen_once(&fixture, "click", move |_event| {
        *called_clone.borrow_mut() += 1;
    })
    .unwrap();

    let event = Event::new("click").unwrap();
    fixture.dispatch_event(&event).unwrap();
    fixture.dispatch_event(&event).unwrap();

    // Обработчик вызван только один раз
    assert_eq!(*called.borrow(), 1);

    common::cleanup_fixture(&fixture);
}

// ============================================================
// Rust-callback API: Notifier
// ============================================================

#[wasm_bindgen_test]
fn test_notifier_subscribe_and_notify() {
    use hello_wasm02::notifier::Notifier;

    let notifier = Notifier::new();
    let received = Rc::new(RefCell::new(Vec::<String>::new()));

    let received_clone = received.clone();
    notifier.subscribe(move |msg: &str| {
        received_clone.borrow_mut().push(msg.to_string());
    });

    notifier.notify("hello");
    notifier.notify("world");

    let items = received.borrow();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], "hello");
    assert_eq!(items[1], "world");
}

#[wasm_bindgen_test]
fn test_notifier_multiple_subscribers() {
    use hello_wasm02::notifier::Notifier;

    let notifier = Notifier::new();
    let a = Rc::new(RefCell::new(0));
    let b = Rc::new(RefCell::new(0));

    let a_clone = a.clone();
    notifier.subscribe(move |_msg| {
        *a_clone.borrow_mut() += 1;
    });

    let b_clone = b.clone();
    notifier.subscribe(move |_msg| {
        *b_clone.borrow_mut() += 10;
    });

    notifier.notify("test");

    assert_eq!(*a.borrow(), 1);
    assert_eq!(*b.borrow(), 10);
    assert_eq!(notifier.subscriber_count(), 2);
}