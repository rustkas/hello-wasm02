//! 50. Архитектура: JS-эквиваленты задач из `arch_pure.rs`.
//!
//! ## Зачем
//!
//! Мы не утверждаем, что Rust «всегда быстрее» — мы даём студенту
//! **инструмент проверки**. Каждая функция возвращает
//! `js_sys::Function`, готовую к вызову. Компиляция функции
//! происходит до старта таймера, чтобы измерять только исполнение.
//!
//! ## Реализация
//!
//! `js_sys::Function::new_with_args(args, body)` — обёртка вокруг
//! `new Function(args, body)`. Она создаёт анонимную функцию:
//! внутри тела **нет** ни глобальных имён, ни самой себя. Поэтому
//! рекурсивные алгоритмы нужно объявлять как function declaration
//! внутри тела — см. `compile_fib`.

use js_sys::{Function, JsString, Uint8Array, Uint32Array};
use wasm_bindgen::prelude::*;

// =============================================================================
// Компиляторы JS-функций
// =============================================================================

/// Компилирует рекурсивный Fibonacci.
///
/// ## Почему `function fib(x) {...}` внутри тела
///
/// `new Function("n", body)` — анонимная функция. Имя `fib` не
/// попадает в её область видимости. Чтобы рекурсия работала,
/// объявляем `fib` через function declaration внутри тела.
/// Function declaration hoisted в начало тела — работает
/// и в strict, и в sloppy mode.
pub fn compile_fib() -> Function {
    return Function::new_with_args(
        "n",
        "function fib(x) { if (x < 2) return x; return fib(x - 1) + fib(x - 2); } return fib(n);",
    );
}

pub fn compile_sort_u32() -> Function {
    return Function::new_with_args(
        "arr",
        "arr.sort((a, b) => a - b); return arr.length;",
    );
}

pub fn compile_crc32() -> Function {
    return Function::new_with_args(
        "bytes",
        "let crc = 0xFFFFFFFF;\
         for (let i = 0; i < bytes.length; i++) {\
           crc ^= bytes[i];\
           for (let j = 0; j < 8; j++) {\
             crc = (crc >>> 1) ^ (0xEDB88320 & -(crc & 1));\
           }\
         }\
         return (crc ^ 0xFFFFFFFF) >>> 0;",
    );
}

pub fn compile_count_code_points() -> Function {
    return Function::new_with_args(
        "text",
        "let n = 0; for (const _ of text) n++; return n;",
    );
}

/// Компилирует парсер кадров `55 AA len payload CRC16` с проверкой
/// CRC-16/CCITT-FALSE — ровно так же, как `parse_frames_count`
/// в `arch_pure.rs`. Без извлечения payload — нам нужен только
/// счётчик валидных кадров.
pub fn compile_parse_frames() -> Function {
    return Function::new_with_args(
        "bytes",
        "function crc16(arr, start, len) {\
           let crc = 0xFFFF;\
           for (let i = 0; i < len; i++) {\
             crc ^= arr[start + i] << 8;\
             for (let j = 0; j < 8; j++) {\
               if (crc & 0x8000) crc = ((crc << 1) ^ 0x1021) & 0xFFFF;\
               else crc = (crc << 1) & 0xFFFF;\
             }\
           }\
           return crc & 0xFFFF;\
         }\
         let count = 0;\
         let i = 0;\
         while (i + 5 <= bytes.length) {\
           if (bytes[i] !== 0x55 || bytes[i + 1] !== 0xAA) break;\
           const len = bytes[i + 2];\
           if (i + 5 + len > bytes.length) break;\
           const crcExpected = crc16(bytes, i + 3, len);\
           const crcActual = bytes[i + 3 + len] | (bytes[i + 4 + len] << 8);\
           if (crcExpected === crcActual) count++;\
           i += 5 + len;\
         }\
         return count;",
    );
}

// =============================================================================
// Типизированные обёртки для вызова
// =============================================================================

pub fn call_fib(f: &Function, n: u32) -> Result<u64, String> {
    let result = f
        .call1(&JsValue::NULL, &JsValue::from_f64(n as f64))
        .map_err(|e| crate::video_frame::format_str(format_args!("call_fib: {e:?}")))?;
    let val = result
        .as_f64()
        .ok_or_else(|| String::from("call_fib: результат не число"))?;
    return Ok(val as u64);
}

pub fn call_sort_u32(f: &Function, data: &[u32]) -> Result<u32, String> {
    let js_arr = Uint32Array::from(data);
    let arg: JsValue = js_arr.into();
    let result = f
        .call1(&JsValue::NULL, &arg)
        .map_err(|e| crate::video_frame::format_str(format_args!("call_sort_u32: {e:?}")))?;
    let val = result
        .as_f64()
        .ok_or_else(|| String::from("call_sort_u32: результат не число"))?;
    return Ok(val as u32);
}

pub fn call_crc32(f: &Function, data: &[u8]) -> Result<u32, String> {
    let js_arr = Uint8Array::from(data);
    let arg: JsValue = js_arr.into();
    let result = f
        .call1(&JsValue::NULL, &arg)
        .map_err(|e| crate::video_frame::format_str(format_args!("call_crc32: {e:?}")))?;
    let val = result
        .as_f64()
        .ok_or_else(|| String::from("call_crc32: результат не число"))?;
    return Ok(val as u32);
}

pub fn call_count_code_points(f: &Function, text: &str) -> Result<u32, String> {
    let js_str = JsString::from(text);
    let arg: JsValue = js_str.into();
    let result = f
        .call1(&JsValue::NULL, &arg)
        .map_err(|e| {
            crate::video_frame::format_str(format_args!("call_count_code_points: {e:?}"))
        })?;
    let val = result
        .as_f64()
        .ok_or_else(|| String::from("call_count_code_points: результат не число"))?;
    return Ok(val as u32);
}

pub fn call_parse_frames(f: &Function, data: &[u8]) -> Result<u32, String> {
    let js_arr = Uint8Array::from(data);
    let arg: JsValue = js_arr.into();
    let result = f
        .call1(&JsValue::NULL, &arg)
        .map_err(|e| crate::video_frame::format_str(format_args!("call_parse_frames: {e:?}")))?;
    let val = result
        .as_f64()
        .ok_or_else(|| String::from("call_parse_frames: результат не число"))?;
    return Ok(val as u32);
}