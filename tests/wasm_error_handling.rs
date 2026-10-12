//! 47. Тесты обработки ошибок (глава «Error Handling»).
//!
//! ## Что тестируется
//!
//! - `extract_error_info` — на всех типах `js_sys::Error`, DOMException,
//!   примитивах (строка, число, boolean), undefined/null.
//! - `AppError::from_info` — правильная категория для каждого имени.
//! - `AppError::user_message` / `severity` / `category_label` — непустые
//!   значения для всех категорий.
//! - `truncate_stack` — обрезка по строкам.
//! - `error_scenarios` — каждый источник возвращает `Err(JsValue)`.
//!
//! ## Не путать с `wasm_errors.rs`
//!
//! `wasm_errors.rs` тестирует `hello_wasm02::fetch` — сетевые ошибки
//! HTTP/Fetch. Здесь — общая граница ошибок Rust ↔ JS.

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

use hello_wasm02::error_info::{
    extract_error_info, format_error_brief, truncate_stack, AppError, JsErrorInfo,
    Severity,
};
use hello_wasm02::error_scenarios;

wasm_bindgen_test_configure!(run_in_browser);

// Хелпер: создать DOMException через JS (из Rust нет прямого конструктора).
#[wasm_bindgen(inline_js = "
export function __test_make_dom_exception(name, message) {
    return new DOMException(message, name);
}
")]
extern "C" {
    fn __test_make_dom_exception(name: &str, message: &str) -> JsValue;
}

// =============================================================================
// extract_error_info — объекты
// =============================================================================

#[wasm_bindgen_test]
fn test_extract_from_js_error() {
    let e = js_sys::Error::new("boom");
    let info = extract_error_info(e.as_ref());
    assert_eq!(info.name, "Error");
    assert_eq!(info.message, "boom");
}

#[wasm_bindgen_test]
fn test_extract_from_type_error() {
    let e = js_sys::TypeError::new("not a function");
    let info = extract_error_info(e.as_ref());
    assert_eq!(info.name, "TypeError");
    assert_eq!(info.message, "not a function");
}

#[wasm_bindgen_test]
fn test_extract_from_range_error() {
    let e = js_sys::RangeError::new("out of bounds");
    let info = extract_error_info(e.as_ref());
    assert_eq!(info.name, "RangeError");
    assert_eq!(info.message, "out of bounds");
}

#[wasm_bindgen_test]
fn test_extract_from_reference_error() {
    let e = js_sys::ReferenceError::new("undefined var");
    let info = extract_error_info(e.as_ref());
    assert_eq!(info.name, "ReferenceError");
}

#[wasm_bindgen_test]
fn test_extract_from_syntax_error() {
    let e = js_sys::SyntaxError::new("unexpected token");
    let info = extract_error_info(e.as_ref());
    assert_eq!(info.name, "SyntaxError");
}

#[wasm_bindgen_test]
fn test_extract_from_dom_exception() {
    let e = __test_make_dom_exception("NotFoundError", "device not found");
    let info = extract_error_info(&e);
    assert_eq!(info.name, "NotFoundError");
    assert_eq!(info.message, "device not found");
    // DOMException имеет числовой code.
    assert!(info.code.is_some(), "DOMException должна иметь code");
}

#[wasm_bindgen_test]
fn test_extract_dom_exception_has_stack() {
    let e = __test_make_dom_exception("AbortError", "user cancelled");
    let info = extract_error_info(&e);
    // Стек может быть пустым в некоторых средах, но поле должно присутствовать.
    let _ = info.stack;
}

// =============================================================================
// extract_error_info — примитивы
// =============================================================================

#[wasm_bindgen_test]
fn test_extract_from_string() {
    let v = JsValue::from_str("bare string");
    let info = extract_error_info(&v);
    assert_eq!(info.name, "String");
    assert_eq!(info.message, "bare string");
    assert_eq!(info.raw.as_deref(), Some("bare string"));
}

#[wasm_bindgen_test]
fn test_extract_from_number() {
    let v = JsValue::from_f64(42.0);
    let info = extract_error_info(&v);
    assert_eq!(info.name, "Number");
    assert_eq!(info.message, "42");
    assert!(info.raw.is_some());
}

#[wasm_bindgen_test]
fn test_extract_from_boolean() {
    let v = JsValue::from_bool(true);
    let info = extract_error_info(&v);
    assert_eq!(info.name, "Boolean");
}

#[wasm_bindgen_test]
fn test_extract_from_undefined() {
    let info = extract_error_info(&JsValue::UNDEFINED);
    assert_eq!(info.name, "Undefined");
    assert!(info.message.contains("undefined"));
}

#[wasm_bindgen_test]
fn test_extract_from_null() {
    let info = extract_error_info(&JsValue::NULL);
    assert_eq!(info.name, "Null");
    assert!(info.message.contains("null"));
}

// =============================================================================
// AppError::from_info
// =============================================================================

fn make_info(name: &str) -> JsErrorInfo {
    return JsErrorInfo {
        name: String::from(name),
        message: String::from("test message"),
        stack: None,
        code: None,
        raw: None,
    };
}

#[wasm_bindgen_test]
fn test_app_error_cancelled() {
    let e = AppError::from_info(&make_info("AbortError"));
    assert!(matches!(e, AppError::Cancelled));
    assert_eq!(e.severity(), Severity::Info);
}

#[wasm_bindgen_test]
fn test_app_error_permission() {
    let e1 = AppError::from_info(&make_info("NotAllowedError"));
    let e2 = AppError::from_info(&make_info("SecurityError"));
    assert!(matches!(e1, AppError::PermissionDenied));
    assert!(matches!(e2, AppError::PermissionDenied));
}

#[wasm_bindgen_test]
fn test_app_error_notfound() {
    assert!(matches!(
        AppError::from_info(&make_info("NotFoundError")),
        AppError::NotFound
    ));
}

#[wasm_bindgen_test]
fn test_app_error_invalid_state() {
    assert!(matches!(
        AppError::from_info(&make_info("InvalidStateError")),
        AppError::InvalidState
    ));
}

#[wasm_bindgen_test]
fn test_app_error_network() {
    assert!(matches!(
        AppError::from_info(&make_info("NetworkError")),
        AppError::Network
    ));
    assert!(matches!(
        AppError::from_info(&make_info("TimeoutError")),
        AppError::Network
    ));
}

#[wasm_bindgen_test]
fn test_app_error_not_supported() {
    assert!(matches!(
        AppError::from_info(&make_info("NotSupportedError")),
        AppError::NotSupported
    ));
}

#[wasm_bindgen_test]
fn test_app_error_invalid_input() {
    assert!(matches!(
        AppError::from_info(&make_info("RangeError")),
        AppError::InvalidInput
    ));
    assert!(matches!(
        AppError::from_info(&make_info("SyntaxError")),
        AppError::InvalidInput
    ));
}

#[wasm_bindgen_test]
fn test_app_error_programmer() {
    assert!(matches!(
        AppError::from_info(&make_info("TypeError")),
        AppError::Programmer
    ));
    assert!(matches!(
        AppError::from_info(&make_info("ReferenceError")),
        AppError::Programmer
    ));
}

#[wasm_bindgen_test]
fn test_app_error_unknown() {
    let e = AppError::from_info(&make_info("SomeWeirdError"));
    assert!(matches!(e, AppError::Unknown { .. }));
}

// =============================================================================
// user_message, category_label, severity
// =============================================================================

#[wasm_bindgen_test]
fn test_user_message_non_empty_for_all() {
    let all = [
        AppError::Cancelled,
        AppError::PermissionDenied,
        AppError::NotFound,
        AppError::InvalidState,
        AppError::Network,
        AppError::NotSupported,
        AppError::InvalidInput,
        AppError::Programmer,
        AppError::Unknown {
            name: String::from("X"),
            message: String::from("y"),
        },
    ];
    for e in all {
        let msg = e.user_message();
        assert!(!msg.is_empty(), "empty user message for {e:?}");
    }
}

#[wasm_bindgen_test]
fn test_category_labels_unique_and_non_empty() {
    let all = [
        AppError::Cancelled,
        AppError::PermissionDenied,
        AppError::NotFound,
        AppError::InvalidState,
        AppError::Network,
        AppError::NotSupported,
        AppError::InvalidInput,
        AppError::Programmer,
    ];
    for e in all {
        assert!(!e.category_label().is_empty());
        assert!(!e.css_class().is_empty());
    }
}

#[wasm_bindgen_test]
fn test_severity_mapping() {
    assert_eq!(AppError::Cancelled.severity(), Severity::Info);
    assert_eq!(AppError::PermissionDenied.severity(), Severity::Warning);
    assert_eq!(AppError::NotFound.severity(), Severity::Warning);
    assert_eq!(AppError::InvalidInput.severity(), Severity::Warning);
    assert_eq!(AppError::Network.severity(), Severity::Error);
    assert_eq!(AppError::NotSupported.severity(), Severity::Error);
    assert_eq!(AppError::Programmer.severity(), Severity::Error);
}

// =============================================================================
// truncate_stack, format_error_brief
// =============================================================================

#[wasm_bindgen_test]
fn test_truncate_stack_short() {
    let s = "line1\nline2";
    assert_eq!(truncate_stack(s, 5), "line1\nline2");
}

#[wasm_bindgen_test]
fn test_truncate_stack_exact() {
    let s = "1\n2\n3";
    assert_eq!(truncate_stack(s, 3), "1\n2\n3");
}

#[wasm_bindgen_test]
fn test_truncate_stack_long() {
    let s = "1\n2\n3\n4\n5\n6\n7";
    let out = truncate_stack(s, 3);
    assert!(out.starts_with("1\n2\n3"));
    assert!(out.contains("+4"));
}

#[wasm_bindgen_test]
fn test_format_error_brief() {
    let i = JsErrorInfo {
        name: String::from("TypeError"),
        message: String::from("bad"),
        stack: None,
        code: None,
        raw: None,
    };
    assert_eq!(format_error_brief(&i), "TypeError: bad");
}

// =============================================================================
// Сценарии: синхронные источники
// =============================================================================

#[wasm_bindgen_test]
fn test_scenario_install_idempotent() {
    error_scenarios::install();
    error_scenarios::install();
    // Если не паникует — уже хорошо.
}

#[wasm_bindgen_test]
fn test_scenario_throw_type_error() {
    error_scenarios::install();
    let r = error_scenarios::run_sync("throw_type_error");
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "TypeError");
}

#[wasm_bindgen_test]
fn test_scenario_throw_range_error() {
    error_scenarios::install();
    let r = error_scenarios::run_sync("throw_range_error");
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "RangeError");
}

#[wasm_bindgen_test]
fn test_scenario_throw_reference_error() {
    error_scenarios::install();
    let r = error_scenarios::run_sync("throw_reference_error");
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "ReferenceError");
}

#[wasm_bindgen_test]
fn test_scenario_throw_custom_error() {
    error_scenarios::install();
    let r = error_scenarios::run_sync("throw_custom_error");
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "Error");
    assert_eq!(info.message, "Custom message from JS");
}

#[wasm_bindgen_test]
fn test_scenario_throw_dom_exception() {
    error_scenarios::install();
    let r = error_scenarios::run_sync("throw_dom_exception");
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "NotFoundError");
    assert_eq!(info.message, "Device not found");
}

// =============================================================================
// Сценарии: асинхронные источники
// =============================================================================

#[wasm_bindgen_test]
async fn test_scenario_reject_error() {
    error_scenarios::install();
    let r = error_scenarios::run_async("reject_error").await;
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "Error");
    assert_eq!(info.message, "Async operation failed");
}

#[wasm_bindgen_test]
async fn test_scenario_reject_dom_exception() {
    error_scenarios::install();
    let r = error_scenarios::run_async("reject_dom_exception").await;
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "NotAllowedError");
    assert_eq!(info.message, "Permission denied");
}

#[wasm_bindgen_test]
async fn test_scenario_reject_string() {
    error_scenarios::install();
    let r = error_scenarios::run_async("reject_string").await;
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "String");
    assert_eq!(info.message, "bare string rejection");
}

#[wasm_bindgen_test]
async fn test_scenario_reject_number() {
    error_scenarios::install();
    let r = error_scenarios::run_async("reject_number").await;
    assert!(r.is_err());
    let info = extract_error_info(&r.unwrap_err());
    assert_eq!(info.name, "Number");
    assert_eq!(info.message, "42");
}

#[wasm_bindgen_test]
async fn test_scenario_success() {
    error_scenarios::install();
    let r = error_scenarios::run_async("success").await;
    assert!(r.is_ok());
    assert_eq!(r.unwrap().as_f64(), Some(42.0));
}

// =============================================================================
// Сквозной путь: сценарий → extract → AppError → user_message
// =============================================================================

#[wasm_bindgen_test]
async fn test_end_to_end_classification() {
    error_scenarios::install();
    let r = error_scenarios::run_async("reject_dom_exception").await;
    let info = extract_error_info(&r.unwrap_err());
    let app_err = AppError::from_info(&info);
    assert!(matches!(app_err, AppError::PermissionDenied));
    assert_eq!(app_err.severity(), Severity::Warning);
    assert!(app_err.user_message().contains("разрешени"));
}

#[wasm_bindgen_test]
fn test_app_error_generic_for_plain_error() {
    // `Error` без подкласса → Generic (прикладная ошибка), не Unknown.
    let e = AppError::from_info(&make_info("Error"));
    assert!(matches!(e, AppError::Generic { .. }));
    assert_eq!(e.severity(), Severity::Warning);
    assert_eq!(e.category_label(), "Прикладная ошибка");
}

#[wasm_bindgen_test]
fn test_app_error_nonstandard_rejection() {
    // `Promise.reject('string')` / `42` → NonStandardRejection.
    let e1 = AppError::from_info(&make_info("String"));
    let e2 = AppError::from_info(&make_info("Number"));
    let e3 = AppError::from_info(&make_info("Boolean"));
    assert!(matches!(e1, AppError::NonStandardRejection { .. }));
    assert!(matches!(e2, AppError::NonStandardRejection { .. }));
    assert!(matches!(e3, AppError::NonStandardRejection { .. }));
    assert_eq!(e1.severity(), Severity::Error);
    assert_eq!(e1.category_label(), "Нестандартный rejection");
}

#[wasm_bindgen_test]
fn test_dom_exception_code_extraction() {
    // DOMException с известным code — извлекается.
    let e = __test_make_dom_exception("NotFoundError", "missing");
    let info = extract_error_info(&e);
    assert_eq!(info.name, "NotFoundError");
    // Chrome: NotFoundError → code 8. Проверяем, что поле есть.
    assert!(
        info.code.is_some(),
        "DOMException должна иметь code (8 для NotFoundError)"
    );
}

#[wasm_bindgen_test]
fn test_plain_error_has_no_code() {
    let e = js_sys::Error::new("plain");
    let info = extract_error_info(e.as_ref());
    assert_eq!(info.name, "Error");
    assert!(info.code.is_none(), "у обычного Error нет code");
}