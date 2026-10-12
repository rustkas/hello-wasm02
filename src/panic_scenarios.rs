//! 48. Panic-сценарии для запуска в Web Worker.
//!
//! ## Зачем отдельный модуль
//!
//! Эти функции **намеренно паникуют**. Вызывать их из main thread
//! нельзя — модуль умрёт. Единственный безопасный контекст —
//! Worker: паника убьёт Worker, но main thread продолжит работу.

use wasm_bindgen::prelude::*;

/// Намеренная паника. Запускается в Worker'е.
///
/// ## Что произойдёт
///
/// - Rust-паника → `unreachable` trap → WASM-trap.
/// - Worker получает uncaught error и завершается.
/// - Main thread получает событие `error` на объекте Worker.
/// - **Main thread продолжает работать.**
#[wasm_bindgen]
pub fn panic_worker_entry() {
    panic!("Demonstration panic inside Web Worker");
}

/// Безопасный ping для проверки, что Worker жив.
#[wasm_bindgen]
pub fn ping_worker_entry() -> u32 {
    return 42;
}