//! 48. Panic и восстановление: prevention, diagnostics, recovery.
//!
//! ## Ключевой факт
//!
//! На `wasm32-unknown-unknown` дефолтная стратегия — `panic = "abort"`.
//! Паника вызывает `unreachable`-инструкцию, что приводит к WASM-trap.
//! Модуль становится непригодным, `std::panic::catch_unwind` не работает.
//!
//! ## Что здесь
//!
//! - **Prevention** — функции `safe_*`, возвращающие `Result` вместо паники.
//! - **Diagnostics** — `install_panic_hook()` для читаемых сообщений в консоли.
//! - **Recovery** — сохранение/загрузка состояния через `localStorage`.
//! - **Classification** — категоризация panic-сообщений для UI.

use crate::video_frame::format_str;

// =============================================================================
// Diagnostics — panic hook
// =============================================================================

/// Устанавливает `console_error_panic_hook::set_once()`.
///
/// Без хука паника в WASM выглядит как `RuntimeError: unreachable` —
/// без сообщения, без стека, без места. С хуком в консоль попадает
/// полноценное сообщение: `panicked at 'msg', src/file.rs:line`.
///
/// ## Идемпотентность
///
/// `set_once()` — идемпотентна. Повторный вызов — no-op.
/// Можно безопасно вызывать в начале `run()`.
pub fn install_panic_hook() {
    console_error_panic_hook::set_once();
}

// =============================================================================
// Prevention — Result вместо panic
// =============================================================================

/// Безопасное деление. `b == 0` → `Err`, не паника.
///
/// `a / 0.0` для `f64` даёт `inf`/`NaN`, но **целочисленное** деление
/// на ноль паникует. Здесь мы явно отсекаем оба случая и возвращаем
/// `Result`, который вызывающий обработает.
pub fn safe_divide(a: f64, b: f64) -> Result<f64, String> {
    if !a.is_finite() || !b.is_finite() {
        return Err(String::from("нечисловой операнд"));
    }
    if b == 0.0 {
        return Err(String::from("деление на ноль"));
    }
    return Ok(a / b);
}

/// Безопасный доступ по индексу. `index >= len` → `Err`, не паника.
pub fn safe_at_u32(data: &[u32], index: usize) -> Result<u32, String> {
    if index >= data.len() {
        return Err(format_str(format_args!(
            "индекс {index} вне диапазона (длина {})",
            data.len()
        )));
    }
    return Ok(data[index]);
}

/// Безопасный парсинг `i32`.
pub fn safe_parse_int(s: &str) -> Result<i32, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(String::from("пустая строка"));
    }
    return trimmed
        .parse::<i32>()
        .map_err(|e| format_str(format_args!("не число: {e}")));
}

/// Обёртка над `Option`. `None` → `Err` с указанием, что не найдено.
pub fn safe_unwrap_or_err<T>(opt: Option<T>, what: &str) -> Result<T, String> {
    return opt.ok_or_else(|| format_str(format_args!("{what}: значение отсутствует")));
}

// =============================================================================
// Classification — категория паники по сообщению
// =============================================================================

/// Категория паники по тексту сообщения. Используется в UI,
/// чтобы показать пользователю понятную подсказку.
/// Категория паники по тексту сообщения. Используется в UI,
/// чтобы показать пользователю понятную подсказку.
///
/// ## Порядок проверок важен
///
/// Более специфичные паттерны идут **раньше** общих. Например,
/// `"capacity overflow"` содержит подстроку `"overflow"` — если
/// сначала проверить `overflow`, то `"capacity overflow"` никогда
/// не дойдёт до своей ветки.
pub fn classify_panic_msg(msg: &str) -> &'static str {
    let lower = msg.to_lowercase();
    if lower.contains("index out of bounds") {
        return "IndexOutOfBounds";
    }
    if lower.contains("divide by zero") || lower.contains("division by zero") {
        return "DivideByZero";
    }
    if lower.contains("called `option::unwrap()`") {
        return "UnwrapOnNone";
    }
    if lower.contains("called `result::unwrap()`") {
        return "UnwrapOnErr";
    }
    if lower.contains("assertion failed") || lower.contains("assertion `") {
        return "AssertionFailed";
    }
    if lower.contains("unreachable") {
        return "Unreachable";
    }
    // Специфичный паттерн раньше общего: "capacity overflow"
    // содержит "overflow" как подстроку.
    if lower.contains("capacity overflow") {
        return "CapacityOverflow";
    }
    if lower.contains("overflow") {
        return "ArithmeticOverflow";
    }
    return "Explicit";
}

// =============================================================================
// Recovery — localStorage
// =============================================================================

/// Сохраняет значение по ключу в `localStorage`.
pub fn save_state(key: &str, value: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| String::from("no window"))?;
    let storage = window
        .local_storage()
        .map_err(|e| format_str(format_args!("localStorage: {e:?}")))?
        .ok_or_else(|| String::from("localStorage недоступен"))?;
    storage
        .set_item(key, value)
        .map_err(|e| format_str(format_args!("setItem: {e:?}")))?;
    return Ok(());
}

/// Загружает значение по ключу. `None`, если ключа нет или
/// `localStorage` недоступен.
pub fn load_state(key: &str) -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok()??;
    return storage.get_item(key).ok()?;
}

/// Удаляет ключ. Идемпотентно: удаление отсутствующего ключа — успех.
pub fn clear_state(key: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| String::from("no window"))?;
    let storage = window
        .local_storage()
        .map_err(|e| format_str(format_args!("localStorage: {e:?}")))?
        .ok_or_else(|| String::from("localStorage недоступен"))?;
    storage
        .remove_item(key)
        .map_err(|e| format_str(format_args!("removeItem: {e:?}")))?;
    return Ok(());
}