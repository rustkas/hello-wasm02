//! Помощники для работы с элементами формы.
//!
//! Этот модуль — переиспользуемая библиотека. В текущем `app.rs`
//! используется только часть функций; остальные понадобятся
//! в следующих главах. Подавляем `dead_code`, чтобы предупреждения
//! не засоряли вывод сборки.
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlButtonElement, HtmlFormElement, HtmlInputElement,
    HtmlOptionElement, HtmlSelectElement, HtmlTextAreaElement,
};

use crate::dom;

// ============================================================
// Создание типизированных элементов
// ============================================================

/// Создаёт `<input>` с заданным типом и `id`.
pub fn create_input(input_type: &str, id: &str) -> Result<HtmlInputElement, JsValue> {
    let el = dom::create_element("input")?;
    let input = el.dyn_into::<HtmlInputElement>().map_err(|_| {
        JsValue::from_str("failed to cast Element to HtmlInputElement")
    })?;
    input.set_type(input_type);
    input.set_id(id);
    Ok(input)
}

/// Создаёт `<textarea>` с заданным `id`.
pub fn create_textarea(id: &str) -> Result<HtmlTextAreaElement, JsValue> {
    let el = dom::create_element("textarea")?;
    let ta = el.dyn_into::<HtmlTextAreaElement>().map_err(|_| {
        JsValue::from_str("failed to cast Element to HtmlTextAreaElement")
    })?;
    ta.set_id(id);
    Ok(ta)
}

/// Создаёт `<select>` с заданным `id`.
pub fn create_select(id: &str) -> Result<HtmlSelectElement, JsValue> {
    let el = dom::create_element("select")?;
    let select = el.dyn_into::<HtmlSelectElement>().map_err(|_| {
        JsValue::from_str("failed to cast Element to HtmlSelectElement")
    })?;
    select.set_id(id);
    Ok(select)
}

/// Создаёт `<option>` с заданным значением и текстом.
pub fn create_option(value: &str, text: &str) -> Result<HtmlOptionElement, JsValue> {
    let el = dom::create_element("option")?;
    let option = el.dyn_into::<HtmlOptionElement>().map_err(|_| {
        JsValue::from_str("failed to cast Element to HtmlOptionElement")
    })?;
    option.set_value(value);
    option.set_text(text);
    Ok(option)
}

/// Создаёт `<button>` с заданным типом и текстом.
pub fn create_button(button_type: &str, text: &str) -> Result<HtmlButtonElement, JsValue> {
    let el = dom::create_element("button")?;
    let button = el.dyn_into::<HtmlButtonElement>().map_err(|_| {
        JsValue::from_str("failed to cast Element to HtmlButtonElement")
    })?;
    button.set_type(button_type);
    button.set_text_content(Some(text));
    Ok(button)
}

/// Создаёт `<form>` с заданным `id`.
pub fn create_form(id: &str) -> Result<HtmlFormElement, JsValue> {
    let el = dom::create_element("form")?;
    let form = el.dyn_into::<HtmlFormElement>().map_err(|_| {
        JsValue::from_str("failed to cast Element to HtmlFormElement")
    })?;
    form.set_id(id);
    Ok(form)
}

// ============================================================
// Чтение значений
// ============================================================

/// Читает значение `<input>`.
pub fn value_of(input: &HtmlInputElement) -> String {
    input.value()
}

/// Читает значение `<textarea>`.
pub fn textarea_value(ta: &HtmlTextAreaElement) -> String {
    ta.value()
}

/// Читает выбранное значение `<select>`.
pub fn select_value(select: &HtmlSelectElement) -> String {
    select.value()
}

// ============================================================
// Приведение из EventTarget
// ============================================================

/// Приводит `EventTarget` к `HtmlInputElement`.
///
/// Возвращает `Result`, потому что `EventTarget` может быть
/// любым элементом, а не только input.
pub fn target_as_input(target: &web_sys::EventTarget) -> Result<HtmlInputElement, JsValue> {
    target
        .dyn_ref::<HtmlInputElement>()
        .cloned()
        .ok_or_else(|| JsValue::from_str("event target is not an HtmlInputElement"))
}

/// Приводит `EventTarget` к `HtmlTextAreaElement`.
pub fn target_as_textarea(
    target: &web_sys::EventTarget,
) -> Result<HtmlTextAreaElement, JsValue> {
    target
        .dyn_ref::<HtmlTextAreaElement>()
        .cloned()
        .ok_or_else(|| JsValue::from_str("event target is not an HtmlTextAreaElement"))
}

/// Приводит `EventTarget` к `HtmlSelectElement`.
pub fn target_as_select(
    target: &web_sys::EventTarget,
) -> Result<HtmlSelectElement, JsValue> {
    target
        .dyn_ref::<HtmlSelectElement>()
        .cloned()
        .ok_or_else(|| JsValue::from_str("event target is not an HtmlSelectElement"))
}

// ============================================================
// Валидация
// ============================================================

/// Проверяет валидность `<input>` по встроенным правилам HTML.
pub fn is_valid(input: &HtmlInputElement) -> bool {
    input.check_validity()
}

/// Возвращает сообщение об ошибке валидации.
pub fn validation_message(input: &HtmlInputElement) -> String {
    input.validation_message().unwrap_or_default()
}

/// Проверяет, пустое ли значение.
pub fn is_empty(value: &str) -> bool {
    value.trim().is_empty()
}