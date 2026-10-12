//! PerformanceObserver: подписка на mark, measure, resource, navigation.
//! Автономный модуль — не зависит от других модулей проекта.
//!
//! ВАЖНО: `PerformanceObserverInit` в web-sys 0.3.105 —
//! dictionary с обязательным аргументом в `new`. Проще собрать
//! объект опций вручную через `js_sys::Object` и вызвать
//! `observer.observe({entryTypes: [...]})` через Reflect.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{PerformanceEntry, PerformanceObserver, PerformanceObserverEntryList};

/// Одна запись производительности в удобном виде.
#[derive(Debug, Clone)]
pub struct PerfEvent {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
}

/// Хранилище событий + сам observer. Держим observer живым, чтобы
/// колбэк не был собран GC.
pub struct PerfSubscription {
    _observer: PerformanceObserver,
    events: Rc<RefCell<Vec<PerfEvent>>>,
}

impl PerfSubscription {
    pub fn subscribe(types: &[&str]) -> Result<Self, JsValue> {
        let events: Rc<RefCell<Vec<PerfEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let events_in = events.clone();

        let cb = Closure::wrap(Box::new(
            move |list: PerformanceObserverEntryList, _obs: PerformanceObserver| {
                let entries = list.get_entries();
                let mut store = events_in.borrow_mut();
                for i in 0..entries.length() {
                    let entry: PerformanceEntry = entries.get(i).unchecked_into();
                    store.push(PerfEvent {
                        name: entry.name(),
                        entry_type: entry.entry_type(),
                        start_time: entry.start_time(),
                        duration: entry.duration(),
                    });
                }
            },
        ) as Box<dyn FnMut(_, _)>);

        let observer = PerformanceObserver::new(cb.as_ref().unchecked_ref())
            .map_err(|e| JsValue::from_str(&format!("PerformanceObserver::new: {e:?}")))?;
        cb.forget();

        // Собираем опции: { entryTypes: ["mark", "measure", ...] }
        let opts = js_sys::Object::new();
        let arr = js_sys::Array::new();
        for t in types {
            arr.push(&JsValue::from_str(t));
        }
        js_sys::Reflect::set(&opts, &JsValue::from_str("entryTypes"), &arr)
            .map_err(|e| JsValue::from_str(&format!("set entryTypes: {e:?}")))?;

        // observer.observe(opts) — через Reflect.
        let observe_fn = js_sys::Reflect::get(&observer, &JsValue::from_str("observe"))
            .map_err(|e| JsValue::from_str(&format!("Reflect observe: {e:?}")))?;
        let observe: js_sys::Function = observe_fn
            .dyn_into()
            .map_err(|_| JsValue::from_str("observe не функция"))?;
        observe
            .call1(&observer, &opts)
            .map_err(|e| JsValue::from_str(&format!("observe: {e:?}")))?;

        Ok(Self {
            _observer: observer,
            events,
        })
    }

    pub fn events(&self) -> Vec<PerfEvent> {
        self.events.borrow().clone()
    }

    pub fn clear(&self) {
        self.events.borrow_mut().clear();
    }

    pub fn disconnect(self) {
        let _ = self._observer.disconnect();
    }
}