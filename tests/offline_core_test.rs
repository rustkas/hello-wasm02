//! Host-тесты ядра offline-first: очередь и модель заметки.

use hello_wasm02::offline_core::{SyncOp, SyncQueue};
use hello_wasm02::offline_state::OfflineNote;

#[test]
fn sync_queue_dedup() {
    let mut q = SyncQueue::new();
    q.push(SyncOp::Upsert("a".into()));
    q.push(SyncOp::Upsert("a".into()));
    assert_eq!(q.len(), 1);
    q.push(SyncOp::Delete("a".into()));
    assert_eq!(q.len(), 2);
}

#[test]
fn sync_queue_drain() {
    let mut q = SyncQueue::new();
    q.push(SyncOp::Upsert("a".into()));
    let ops = q.drain();
    assert_eq!(ops.len(), 1);
    assert!(q.is_empty());
}

#[test]
fn note_touch_updates_version() {
    let mut n = OfflineNote::new("1", "t", "b");
    assert_eq!(n.version, 1);
    n.touch();
    assert_eq!(n.version, 2);
    assert!(!n.deleted);
}