//! Глава 42: WebGL — GPU-рендеринг в браузере.
//!
//! WebGL — низкоуровневый API для доступа к GPU из JavaScript.
//! Он основан на OpenGL ES 2.0/3.0 и предоставляет программируемый
//! конвейер: вершинные и фрагментные шейдеры, буферы, текстуры,
//! uniform-переменные.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

pub mod shader;
pub mod buffer;
pub mod program;
pub mod texture;
pub mod geometry;

use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use crate::format_str::format_str;

/// Универсальная ошибка WebGL-операций.
#[derive(Debug, Clone)]
pub struct WebGlError {
    pub message: String,
}

impl WebGlError {
    pub fn new(msg: String) -> Self {
        return Self { message: msg };
    }
}

impl std::fmt::Display for WebGlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.message);
    }
}

impl From<JsValue> for WebGlError {
    fn from(v: JsValue) -> Self {
        let msg = v
            .as_string()
            .unwrap_or_else(|| format_str(format_args!("{v:?}")));
        return Self { message: msg };
    }
}

impl From<String> for WebGlError {
    fn from(s: String) -> Self {
        return Self { message: s };
    }
}

/// Получает WebGL2-контекст из canvas.
///
/// WebGL2 предпочтительнее: он даёт VAO, instancing, 3D-текстуры
/// и другие возможности OpenGL ES 3.0.
pub fn context(
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<web_sys::WebGl2RenderingContext, WebGlError> {
    let ctx = canvas
        .get_context("webgl2")
        .map_err(|e| WebGlError::from(e))?
        .ok_or_else(|| WebGlError::new("webgl2 context unavailable".to_string()))?;
    let gl: web_sys::WebGl2RenderingContext = ctx
        .dyn_into()
        .map_err(|_| WebGlError::new("not a WebGl2RenderingContext".to_string()))?;
    return Ok(gl);
}

/// Читает код ошибки из GL и возвращает читаемую строку.
///
/// `0` (NO_ERROR) — всё в порядке, возвращаем `None`.
/// Любое другое значение — ошибка, возвращаем её имя и hex-код.
///
/// ПРАВИЛО: после каждого draw-вызова проверяйте `check_error`.
/// GPU не бросает исключения, он молча ставит флаг ошибки.
pub fn check_error(gl: &web_sys::WebGl2RenderingContext) -> Option<String> {
    let err = gl.get_error();
    if err == web_sys::WebGl2RenderingContext::NO_ERROR {
        return None;
    }
    let name = match err {
        web_sys::WebGl2RenderingContext::INVALID_ENUM => "INVALID_ENUM",
        web_sys::WebGl2RenderingContext::INVALID_VALUE => "INVALID_VALUE",
        web_sys::WebGl2RenderingContext::INVALID_OPERATION => "INVALID_OPERATION",
        web_sys::WebGl2RenderingContext::OUT_OF_MEMORY => "OUT_OF_MEMORY",
        web_sys::WebGl2RenderingContext::INVALID_FRAMEBUFFER_OPERATION => {
            "INVALID_FRAMEBUFFER_OPERATION"
        }
        _ => "UNKNOWN",
    };
    return Some(format_str(format_args!("GL error: {name} (0x{err:x})")));
}