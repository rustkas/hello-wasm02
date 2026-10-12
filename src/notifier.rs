//! Система уведомлений с Rust-callback API.
//!
//! Демонстрирует паттерн «observer»: подписчики регистрируются
//! через `subscribe`, уведомляются через `notify`. Подписчики —
//! `Fn(&str)`, хранятся в `Vec<Box<dyn Fn(&str)>>`.
//!
//! Этот модуль не зависит от DOM или `web_sys`. Он может
//! использоваться как в браузере, так и в нативных приложениях.
//! Именно поэтому он вынесен из `app.rs` — приложение меняется
//! от главы к главе, а `Notifier` — стабильный компонент.

use std::cell::RefCell;
use std::rc::Rc;

/// Простая система уведомлений с callback API.
pub struct Notifier {
    listeners: Rc<RefCell<Vec<Box<dyn Fn(&str)>>>>,
}

impl Notifier {
    /// Создаёт пустой `Notifier`.
    pub fn new() -> Self {
        Self {
            listeners: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Регистрирует подписчика.
    ///
    /// Замыкание вызывается при каждом `notify`. Несколько
    /// подписчиков вызываются в порядке регистрации.
    pub fn subscribe<F>(&self, callback: F)
    where
        F: Fn(&str) + 'static,
    {
        self.listeners.borrow_mut().push(Box::new(callback));
    }

    /// Отправляет сообщение всем подписчикам.
    pub fn notify(&self, message: &str) {
        for listener in self.listeners.borrow().iter() {
            listener(message);
        }
    }

    /// Количество подписчиков.
    pub fn subscriber_count(&self) -> usize {
        self.listeners.borrow().len()
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}