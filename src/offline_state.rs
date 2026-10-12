//! Состояние offline-приложения и модель заметки.

use serde::{Deserialize, Serialize};

use crate::offline_core::{OnlineStatus, SyncQueue};

/// Заметка с поддержкой offline-синхронизации.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfflineNote {
    pub id: String,
    pub title: String,
    pub body: String,
    pub updated_at: f64,
    pub deleted: bool,
    pub version: u32,
}

impl OfflineNote {
    pub fn new(id: &str, title: &str, body: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: body.into(),
            updated_at: js_sys::Date::now(),
            deleted: false,
            version: 1,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = js_sys::Date::now();
        self.version += 1;
    }
}

/// Общее состояние приложения.
pub struct AppState {
    pub notes: Vec<OfflineNote>,
    pub online: OnlineStatus,
    pub queue: SyncQueue,
    pub syncing: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            online: OnlineStatus::Offline,
            queue: SyncQueue::new(),
            syncing: false,
        }
    }
}

pub type SharedState = std::rc::Rc<std::cell::RefCell<AppState>>;

pub fn new_shared_state() -> SharedState {
    std::rc::Rc::new(std::cell::RefCell::new(AppState::new()))
}