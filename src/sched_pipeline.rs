//! Практический проект Главы 30: планировщик задач с приоритетами.
//!
//! Задачи делятся на три приоритета. Очередь с наибольшим приоритетом
//! обслуживается первой. Внутри приоритета — FIFO.
//!
//! Выполнение чанками с yield через `TimeoutFuture::new(0).await`
//! не даёт UI «зависнуть».

use std::collections::VecDeque;

use crate::sched_core;

/// Уровень приоритета задачи.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Background = 0,
    UserVisible = 1,
    UserBlocking = 2,
}

/// Задача в очереди.
#[derive(Debug, Clone)]
pub struct SchedTask {
    pub id: u32,
    pub priority: TaskPriority,
    /// Объём работы в «единицах» (условные чанки).
    pub work_units: u32,
}

/// Планировщик с тремя очередями.
#[derive(Debug, Default)]
pub struct Scheduler {
    queues: [VecDeque<SchedTask>; 3],
    next_id: u32,
    completed: u32,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            queues: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            next_id: 1,
            completed: 0,
        }
    }

    pub fn push(&mut self, priority: TaskPriority, work_units: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let task = SchedTask { id, priority, work_units };
        self.queues[priority as usize].push_back(task);
        id
    }

    /// Извлекает задачу с наивысшим приоритетом.
    pub fn pop(&mut self) -> Option<SchedTask> {
        for q in self.queues.iter_mut().rev() {
            if let Some(task) = q.pop_front() {
                return Some(task);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.queues.iter().all(|q| q.is_empty())
    }

    pub fn total_pending(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    pub fn pending_by_priority(&self, p: TaskPriority) -> usize {
        self.queues[p as usize].len()
    }

    pub fn completed(&self) -> u32 {
        self.completed
    }

    pub fn mark_completed(&mut self) {
        self.completed += 1;
    }
}

/// Одна единица работы — условная «полезная нагрузка».
/// В реальном коде здесь был бы парсинг, хеширование, индексация.
pub fn do_work_unit() -> u64 {
    let mut acc: u64 = 0;
    for i in 0..10_000u64 {
        acc = acc.wrapping_add(i.wrapping_mul(2654435761));
    }
    acc
}

/// Выполняет все задачи планировщика, yield'я UI после каждого чанка.
///
/// `on_progress(done, total)` вызывается после каждой выполненной задачи.
pub async fn run_all<F>(scheduler: &mut Scheduler, mut on_progress: F)
where
    F: FnMut(u32, u32),
{
    let total = scheduler.total_pending() as u32;
    let mut done = 0u32;

    while let Some(task) = scheduler.pop() {
        for _ in 0..task.work_units {
            let _ = do_work_unit();
            sched_core::yield_to_ui().await;
        }
        scheduler.mark_completed();
        done += 1;
        on_progress(done, total);
    }
}

/// Выполняет задачи с троттлингом yield'ов.
///
/// Yield происходит не чаще, чем раз в `yield_interval_ms`.
/// 16 мс ≈ 60 Гц — обновления UI попадают в кадры, не чаще.
pub async fn run_all_throttled<F>(
    scheduler: &mut Scheduler,
    yield_interval_ms: f64,
    mut on_progress: F,
) where
    F: FnMut(u32, u32),
{
    let total = scheduler.total_pending() as u32;
    let mut done = 0u32;
    let mut last_yield = sched_core::now_ms();

    while let Some(task) = scheduler.pop() {
        for _ in 0..task.work_units {
            let _ = do_work_unit();
            let now = sched_core::now_ms();
            if now - last_yield >= yield_interval_ms {
                sched_core::yield_to_ui().await;
                last_yield = sched_core::now_ms();
            }
        }
        scheduler.mark_completed();
        done += 1;
        on_progress(done, total);
    }
    // Финальный yield — чтобы последнее обновление UI стало видимым.
    sched_core::yield_to_ui().await;
}