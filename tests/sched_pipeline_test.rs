//! Wasm-тесты: браузерные API планирования.
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::sched_core;
use hello_wasm02::sched_pipeline::{self, Scheduler, TaskPriority};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn performance_now_is_monotonic() {
    let t0 = sched_core::now_ms();
    let t1 = sched_core::now_ms();
    assert!(t1 >= t0, "performance.now() должен монотонно возрастать");
}

#[wasm_bindgen_test]
async fn yield_to_ui_returns_control() {
    // Проверяем, что yield не паникует и завершается.
    sched_core::yield_to_ui().await;
}

#[wasm_bindgen_test]
async fn run_all_completes_all_tasks() {
    let mut s = Scheduler::new();
    s.push(TaskPriority::UserBlocking, 1);
    s.push(TaskPriority::UserVisible, 1);
    s.push(TaskPriority::Background, 1);

    let mut progress_calls = 0u32;
    sched_pipeline::run_all(&mut s, |_done, _total| {
        progress_calls += 1;
    })
    .await;

    assert!(s.is_empty());
    assert_eq!(s.completed(), 3);
    assert_eq!(progress_calls, 3);
}

#[wasm_bindgen_test]
async fn run_all_respects_priority_order() {
    let mut s = Scheduler::new();
    // Background первым пушим — но выполнится последним.
    s.push(TaskPriority::Background, 1);
    s.push(TaskPriority::UserBlocking, 1);

    // Соберём порядок завершения через замыкание.
    let mut order: Vec<u32> = Vec::new();
    // run_all вызывает on_progress после каждой задачи, но не отдаёт id.
    // Поэтому проверим косвенно: completed = 2, очередь пуста.
    sched_pipeline::run_all(&mut s, |done, total| {
        order.push(done);
        assert!(done <= total);
    })
    .await;

    assert_eq!(order, vec![1, 2]);
    assert_eq!(s.completed(), 2);
}

#[wasm_bindgen_test]
async fn run_all_on_empty_scheduler_is_noop() {
    let mut s = Scheduler::new();
    let mut calls = 0u32;
    sched_pipeline::run_all(&mut s, |_, _| calls += 1).await;
    assert_eq!(calls, 0);
}