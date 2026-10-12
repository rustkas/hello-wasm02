//! Глава 44: WebGPU.

pub mod device;
pub mod buffer;
pub mod pipeline;
pub mod compute;

use wasm_bindgen::{JsCast, JsValue};
use crate::format_str::format_str;

#[derive(Debug, Clone)]
pub struct WebGpuError { pub message: String }

impl WebGpuError {
    pub fn new(msg: String) -> Self { return Self { message: msg }; }
}

impl std::fmt::Display for WebGpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.message);
    }
}

impl From<JsValue> for WebGpuError {
    fn from(v: JsValue) -> Self {
        let msg = v.as_string().unwrap_or_else(|| format_str(format_args!("{v:?}")));
        return Self { message: msg };
    }
}

impl From<String> for WebGpuError {
    fn from(s: String) -> Self { return Self { message: s }; }
}

/// Возвращает `navigator.gpu`.
pub fn gpu() -> Result<web_sys::Gpu, WebGpuError> {
    let window = web_sys::window().ok_or_else(|| WebGpuError::new("no window".to_string()))?;
    let navigator = window.navigator();
    let gpu_js = js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu"))
        .map_err(WebGpuError::from)?;
    if gpu_js.is_undefined() || gpu_js.is_null() {
        return Err(WebGpuError::new(
            "WebGPU не поддерживается. Chrome 113+, Firefox 141+ (Windows), Safari 26+.".to_string(),
        ));
    }
    let gpu: web_sys::Gpu = gpu_js.dyn_into()
        .map_err(|_| WebGpuError::new("navigator.gpu не является Gpu".to_string()))?;
    return Ok(gpu);
}

pub fn is_available() -> bool { return gpu().is_ok(); }

// ============================================================
//                    Reflect helpers
// ============================================================
//
// Все WebGPU-методы вызываются через Reflect — сигнатуры в
// web-sys 0.3.105 нестабильны (одни возвращают Result, другие
// значение напрямую; дескрипторы — типизированные структуры).

pub fn get_property(target: &JsValue, name: &str) -> Result<JsValue, WebGpuError> {
    return js_sys::Reflect::get(target, &JsValue::from_str(name)).map_err(WebGpuError::from);
}

pub fn call_method0(target: &JsValue, name: &str) -> Result<JsValue, WebGpuError> {
    let func: js_sys::Function = get_property(target, name)?
        .dyn_into()
        .map_err(|_| WebGpuError::new(format_str(format_args!("{name}: not a function"))))?;
    return func.call0(target).map_err(WebGpuError::from);
}

pub fn call_method1(target: &JsValue, name: &str, a: &JsValue) -> Result<JsValue, WebGpuError> {
    let func: js_sys::Function = get_property(target, name)?
        .dyn_into()
        .map_err(|_| WebGpuError::new(format_str(format_args!("{name}: not a function"))))?;
    return func.call1(target, a).map_err(WebGpuError::from);
}

pub fn call_method_apply(target: &JsValue, name: &str, args: &js_sys::Array) -> Result<JsValue, WebGpuError> {
    let func: js_sys::Function = get_property(target, name)?
        .dyn_into()
        .map_err(|_| WebGpuError::new(format_str(format_args!("{name}: not a function"))))?;
    return func.apply(target, args).map_err(WebGpuError::from);
}

pub fn set_prop(obj: &js_sys::Object, key: &str, val: &JsValue) -> Result<(), WebGpuError> {
    js_sys::Reflect::set(obj, &JsValue::from_str(key), val).map_err(WebGpuError::from)?;
    return Ok(());
}