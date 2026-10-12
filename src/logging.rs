/// Инициализация инфраструктуры логирования.
///
/// Устанавливает panic hook, чтобы паники в Rust отображались
/// в консоли браузера с нормальным сообщением и стеком.
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Логирование в консоль браузера.
#[macro_export]
macro_rules! log {
    ($($t:tt)*) => {
        web_sys::console::log_1(&format!($($t)*).into())
    };
}