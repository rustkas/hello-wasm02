//! Wasm-тесты синхронизации: push, pull, merge, разрешение конфликтов.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen_test::*;

use hello_wasm02::offline_core::OnlineStatus;
use hello_wasm02::offline_state::{AppState, OfflineNote, SharedState};
use hello_wasm02::offline_sync;

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64)
}

fn shared_online() -> SharedState {
    let state = Rc::new(RefCell::new(AppState::new()));
    state.borrow_mut().online = OnlineStatus::Online;
    state
}

#[wasm_bindgen_test]
async fn sync_empty_state() {
    let state = shared_online();
    assert!(offline_sync::sync(state).await.is_ok());
}

#[wasm_bindgen_test]
async fn sync_pushes_local_note() {
    let state = shared_online();
    let note = OfflineNote::new(&uid(), "Sync", "Body");
    state.borrow_mut().notes.push(note.clone());
    offline_sync::sync(state.clone()).await.unwrap();
    assert!(state.borrow().notes.iter().any(|n| n.id == note.id));
}

#[wasm_bindgen_test]
async fn sync_offline_returns_error() {
    let state = Rc::new(RefCell::new(AppState::new()));
    state.borrow_mut().online = OnlineStatus::Offline;
    assert!(offline_sync::sync(state).await.is_err());
}

#[wasm_bindgen_test]
async fn queue_cleared_after_sync() {
    let state = shared_online();
    state.borrow_mut().queue.push(hello_wasm02::offline_core::SyncOp::Upsert("x".into()));
    offline_sync::sync(state.clone()).await.unwrap();
    assert!(state.borrow().queue.is_empty());
}