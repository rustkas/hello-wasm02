//! Обёртка над `format!` для хвостовых позиций.
//!
//! rust-analyzer регулярно теряет тип `format!`, когда макрос оказывается
//! последним выражением ветки `if`/`else`, блока `match`-арма или тела
//! функции — и сообщает `expected String, found ()`. Обход: вызывать
//! функцию с явной сигнатурой `-> String`. Тогда инференс не зависит от
//! контекста, в котором стоит вызов.
//!
//! ## Использование
//!
//! ```ignore
//! // Плохо: format! в tail-позиции
//! fn render(x: u32) -> String {
//!     if x == 0 { format!("zero") } else { format!("{x}") }
//! }
//!
//! // Хорошо:
//! use crate::format_str::format_str;
//! fn render(x: u32) -> String {
//!     if x == 0 {
//!         format_str(format_args!("zero"))
//!     } else {
//!         format_str(format_args!("{x}"))
//!     }
//! }
//! ```
//!
//! ## Варианты вызова
//!
//! - функция: `format_str(format_args!("..."))` — явно, без макросов;
//! - макрос: `fmt_str!("...")` — раскрывается в `let s: String = ...; s`,
//!   читается как обычный `format!`.

use std::fmt::Arguments;

/// Возвращает `args` как `String`. Единственная цель — дать rust-analyzer
/// точку, где тип результата жёстко зафиксирован сигнатурой.
#[inline]
pub fn format_str(args: Arguments<'_>) -> String {
    args.to_string()
}

/// Удобный макрос: делает то же, что `format!`, но гарантированно возвращает
/// `String` — раскрывается в `let __s: String = format!(...); __s`. Это
/// исключает попадание `format!` в tail-позицию вообще.
#[macro_export]
macro_rules! fmt_str {
    ($($arg:tt)*) => {{
        let __s: String = ::std::format!($($arg)*);
        __s
    }};
}