//! Помощники для управления временем жизни `Closure`.
//!
//! ## Зачем это нужно
//!
//! Когда Rust-замыкание передаётся в JavaScript (например, через
//! `add_event_listener_with_callback`), `wasm-bindgen` создаёт
//! объект `Closure`, который хранит замыкание и предоставляет
//! JS-функцию-обёртку. JavaScript держит ссылку на эту функцию,
//! но **не владеет** `Closure`.
//!
//! Если `Closure` удаляется (drop), JS-функция становится
//! недействительной. Если `Closure` «забывается» через `forget()`,
//! он остаётся в памяти навсегда — утечка.
//!
//! Правильное решение — **хранить `Closure` в структуре или
//! контейнере**, время жизни которого совпадает с временем
//! использования обработчика. Когда контейнер удаляется,
//! все `Closure` внутри него тоже удаляются, а JS-обработчики
//! автоматически отписываются (после `remove_event_listener`).
//!
//! ## Что в этом модуле
//!
//! - `EventClosures` — контейнер для `Closure<dyn FnMut(Event)>`.
//! - `listen` — регистрирует обработчик и сохраняет `Closure`
//!   в контейнере.
//! - `listen_typed<T>` — то же, но для конкретного типа события.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Event, EventTarget};

// ============================================================
// Контейнер для Closure
// ============================================================

/// Хранилище для `Closure<dyn FnMut(Event)>`.
///
/// Пока контейнер жив, все замыкания активны. Когда контейнер
/// удаляется, все `Closure` внутри него тоже удаляются, и
/// соответствующие JS-обработчики перестают работать.
///
/// Обычно контейнер оборачивают в `Rc<RefCell<EventClosures>>`,
/// чтобы разделить владение между несколькими обработчиками.
#[derive(Default)]
pub struct EventClosures {
    closures: Vec<Closure<dyn FnMut(Event)>>,
}

impl EventClosures {
    /// Создаёт пустой контейнер.
    pub fn new() -> Self {
        Self {
            closures: Vec::new(),
        }
    }

    /// Добавляет `Closure` в контейнер.
    pub fn push(&mut self, closure: Closure<dyn FnMut(Event)>) {
        self.closures.push(closure);
    }

    /// Количество хранимых замыканий.
    pub fn len(&self) -> usize {
        self.closures.len()
    }

    /// Проверяет, пуст ли контейнер.
    pub fn is_empty(&self) -> bool {
        self.closures.is_empty()
    }

    /// Удаляет все замыкания.
    ///
    /// **Важно:** это не отписывает JS-обработчики.
    /// `add_event_listener_with_callback` не запоминает функцию,
    /// чтобы её можно было удалить. Если нужен полный cleanup,
    /// используйте `remove_event_listener_with_callback` перед
    /// `clear`.
    pub fn clear(&mut self) {
        self.closures.clear();
    }
}

// ============================================================
// Регистрация обработчиков
// ============================================================

/// Регистрирует обработчик события и сохраняет `Closure` в контейнере.
///
/// Это правильный способ подписки на события: `Closure` не «забывается»,
/// а живёт столько, сколько живёт контейнер.
pub fn listen<F>(
    target: &EventTarget,
    event_type: &str,
    mut handler: F,
    store: &Rc<RefCell<EventClosures>>,
) -> Result<(), JsValue>
where
    F: FnMut(Event) + 'static,
{
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        handler(event);
    });
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    store.borrow_mut().push(closure);
    Ok(())
}

/// Регистрирует обработчик с конкретным типом события.
///
/// Например, для `KeyboardEvent` или `MouseEvent`. Внутри
/// `Event` приводится к `T` через `dyn_into`; если приведение
/// не удалось, обработчик не вызывается.
pub fn listen_typed<T, F>(
    target: &EventTarget,
    event_type: &str,
    mut handler: F,
    store: &Rc<RefCell<EventClosures>>,
) -> Result<(), JsValue>
where
    T: JsCast + 'static,
    F: FnMut(T) + 'static,
{
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        if let Ok(typed) = event.dyn_into::<T>() {
            handler(typed);
        }
    });
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    store.borrow_mut().push(closure);
    Ok(())
}

// ============================================================
// Одноразовые и «забытые» обработчики
// ============================================================

/// Регистрирует одноразовый обработчик.
///
/// JS-функция сама себя удаляет после первого вызова.
/// `Closure` остаётся в памяти навсегда — это компромисс
/// ради простоты. Для одноразовых событий (`load`, `error`)
/// утечка несущественна.
pub fn listen_once<F>(
    target: &EventTarget,
    event_type: &str,
     handler: F,
) -> Result<(), JsValue>
where
    F: FnOnce(Event) + 'static,
{
    let closure = Closure::once(move |event: Event| {
        handler(event);
    });
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

/// Регистрирует обработчик и «забывает» `Closure`.
///
/// Простой, но неаккуратный способ: `Closure` остаётся
/// в памяти навсегда. Подходит для обработчиков, которые
/// живут всё время работы приложения.
pub fn listen_forget<F>(
    target: &EventTarget,
    event_type: &str,
    mut handler: F,
) -> Result<(), JsValue>
where
    F: FnMut(Event) + 'static,
{
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        handler(event);
    });
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}