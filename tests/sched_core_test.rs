//! Host-тесты модуля sched_pipeline: логика очередей и приоритетов.
//! Браузерные API (Performance, rAF, IdleDeadline) не тестируются здесь —
//! они покрыты wasm-тестами.

use hello_wasm02::sched_pipeline::{Scheduler, TaskPriority};

#[test]
fn empty_scheduler_has_nothing_to_pop() {
    let mut s = Scheduler::new();
    assert!(s.is_empty());
    assert!(s.pop().is_none());
    assert_eq!(s.total_pending(), 0);
}

#[test]
fn push_and_pop_single_task() {
    let mut s = Scheduler::new();
    let id = s.push(TaskPriority::UserVisible, 3);
    assert_eq!(s.total_pending(), 1);
    let task = s.pop().expect("task");
    assert_eq!(task.id, id);
    assert_eq!(task.priority, TaskPriority::UserVisible);
    assert_eq!(task.work_units, 3);
    assert!(s.is_empty());
}

#[test]
fn high_priority_served_first() {
    let mut s = Scheduler::new();
    let bg = s.push(TaskPriority::Background, 1);
    let ub = s.push(TaskPriority::UserBlocking, 1);
    let uv = s.push(TaskPriority::UserVisible, 1);

    let first = s.pop().expect("first");
    assert_eq!(first.id, ub, "user-blocking должен выйти первым");

    let second = s.pop().expect("second");
    assert_eq!(second.id, uv, "затем user-visible");

    let third = s.pop().expect("third");
    assert_eq!(third.id, bg, "background последним");
}

#[test]
fn fifo_within_priority() {
    let mut s = Scheduler::new();
    let a = s.push(TaskPriority::UserVisible, 1);
    let b = s.push(TaskPriority::UserVisible, 1);
    let c = s.push(TaskPriority::UserVisible, 1);

    assert_eq!(s.pop().unwrap().id, a);
    assert_eq!(s.pop().unwrap().id, b);
    assert_eq!(s.pop().unwrap().id, c);
}

#[test]
fn pending_by_priority_counts_correctly() {
    let mut s = Scheduler::new();
    s.push(TaskPriority::Background, 1);
    s.push(TaskPriority::Background, 1);
    s.push(TaskPriority::UserVisible, 1);

    assert_eq!(s.pending_by_priority(TaskPriority::Background), 2);
    assert_eq!(s.pending_by_priority(TaskPriority::UserVisible), 1);
    assert_eq!(s.pending_by_priority(TaskPriority::UserBlocking), 0);
    assert_eq!(s.total_pending(), 3);
}

#[test]
fn completed_counter_increments() {
    let mut s = Scheduler::new();
    s.push(TaskPriority::UserVisible, 1);
    s.pop();
    s.mark_completed();
    assert_eq!(s.completed(), 1);
}

#[test]
fn priority_ordering_is_total() {
    assert!(TaskPriority::UserBlocking > TaskPriority::UserVisible);
    assert!(TaskPriority::UserVisible > TaskPriority::Background);
    assert!(TaskPriority::UserBlocking > TaskPriority::Background);
}