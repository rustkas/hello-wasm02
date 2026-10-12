//! Ядро offline-first: статус сети и очередь синхронизации.
//! Автономный модуль — не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;
use web_sys::window;

/// Статус сети.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineStatus {
    Online,
    Offline,
}

/// Возвращает текущий статус сети через `navigator.onLine`.
pub fn online_status() -> OnlineStatus {
    let nav = match window() {
        Some(w) => w.navigator(),
        None => return OnlineStatus::Offline,
    };
    let online = js_sys::Reflect::get(&nav, &JsValue::from_str("onLine"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if online {
        OnlineStatus::Online
    } else {
        OnlineStatus::Offline
    }
}

/// Операция, ожидающая синхронизации.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncOp {
    Upsert(String),
    Delete(String),
}

/// Очередь операций для синхронизации.
#[derive(Debug, Default)]
pub struct SyncQueue {
    ops: Vec<SyncOp>,
}

impl SyncQueue {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Идемпотентно: повторная операция не добавляется.
    pub fn push(&mut self, op: SyncOp) {
        if !self.ops.contains(&op) {
            self.ops.push(op);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn drain(&mut self) -> Vec<SyncOp> {
        std::mem::take(&mut self.ops)
    }
}