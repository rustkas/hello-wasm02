//! Логика синхронизации: push, pull, merge, разрешение конфликтов.
//!
//! Стратегия: Last-Write-Wins по `updated_at`, при равенстве — по `version`.
//! После успешного merge tombstone'ы (заметки с `deleted = true`)
//! физически удаляются из IndexedDB, чтобы не копить мусор.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::offline_core::{OnlineStatus, SyncQueue};
use crate::offline_server;
use crate::offline_state::{OfflineNote, SharedState};
use crate::offline_store;

/// Разрешение конфликта: Last-Write-Wins.
///
/// 1. Больший `updated_at` побеждает.
/// 2. При равенстве — больший `version` побеждает.
/// 3. При полном равенстве — локальная версия (стабильный выбор).
fn resolve_conflict(local: &OfflineNote, remote: &OfflineNote) -> OfflineNote {
    if local.updated_at > remote.updated_at {
        local.clone()
    } else if remote.updated_at > local.updated_at {
        remote.clone()
    } else if local.version >= remote.version {
        local.clone()
    } else {
        remote.clone()
    }
}

/// Полная синхронизация: push, pull, merge, очистка очереди и tombstone'ов.
///
/// Идемпотентна по эффекту: повторный вызов при пустой очереди и пустом
/// сервере не меняет состояние. Повторный вызов во время активной
/// синхронизации возвращает `Ok(())` без действий.
pub async fn sync(state: SharedState) -> Result<(), JsValue> {
    if state.borrow().online == OnlineStatus::Offline {
        return Err(JsValue::from_str("нет сети"));
    }
    if state.borrow().syncing {
        return Ok(());
    }
    state.borrow_mut().syncing = true;

    let result = sync_inner(state.clone()).await;
    state.borrow_mut().syncing = false;
    result
}

async fn sync_inner(state: SharedState) -> Result<(), JsValue> {
    // 1. Push локальных изменений (только тех, что есть в очереди).
    //    Если очередь пуста — не отправляем ничего, чтобы не перетирать
    //    серверные изменения локальными «устаревшими» копиями.
    let has_ops = !state.borrow().queue.is_empty();
    if has_ops {
        let local_notes = state.borrow().notes.clone();
        offline_server::server_push(local_notes).await?;
    }

    // 2. Pull серверных.
    let remote_notes = offline_server::server_pull().await?;

    // 3. Merge с разрешением конфликтов.
    let local_map: HashMap<String, OfflineNote> = state
        .borrow()
        .notes
        .iter()
        .map(|n| (n.id.clone(), n.clone()))
        .collect();

    let mut merged: Vec<OfflineNote> = Vec::with_capacity(remote_notes.len());
    for remote in remote_notes {
        match local_map.get(&remote.id) {
            Some(local) => merged.push(resolve_conflict(local, &remote)),
            None => merged.push(remote),
        }
    }
    // Локальные заметки, которых нет на сервере (ещё не пушились).
    for local in &state.borrow().notes {
        if !merged.iter().any(|n| n.id == local.id) {
            merged.push(local.clone());
        }
    }

    // 4. Физически удаляем tombstone'ы из IndexedDB.
    let tombstones: Vec<String> = merged
        .iter()
        .filter(|n| n.deleted)
        .map(|n| n.id.clone())
        .collect();
    for id in &tombstones {
        let _ = offline_store::hard_delete_note(id).await;
    }
    merged.retain(|n| !n.deleted);

    // 5. Обновляем состояние и очищаем очередь.
    state.borrow_mut().notes = merged;
    state.borrow_mut().queue = SyncQueue::new();
    Ok(())
}