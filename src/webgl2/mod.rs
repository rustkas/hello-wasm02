//! Глава 43: WebGL 2 — продвинутые возможности GPU.
//!
//! WebGL 2 основан на OpenGL ES 3.0 и добавляет к базовому
//! конвейеру (глава 42):
//!   - instancing (`drawArraysInstanced` + `vertexAttribDivisor`);
//!   - transform feedback (запись вывода вершинного шейдера в буфер);
//!   - floating-point textures (RGBA32F, R32F);
//!   - 3D-текстуры, MRT, integer attributes.
//!
//! ПРАВИЛО: format! в tail-позиции — через format_str(format_args!(...)).

pub mod shader;
pub mod instancing;
pub mod geometry;

use std::collections::HashMap;
use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use crate::format_str::format_str;

// ============================================================
//                    ОШИБКИ
// ============================================================

/// Универсальная ошибка WebGL2-операций.
#[derive(Debug, Clone)]
pub struct WebGl2Error {
    pub message: String,
}

impl WebGl2Error {
    pub fn new(msg: String) -> Self {
        return Self { message: msg };
    }
}

impl std::fmt::Display for WebGl2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.message);
    }
}

impl From<JsValue> for WebGl2Error {
    fn from(v: JsValue) -> Self {
        let msg = v
            .as_string()
            .unwrap_or_else(|| format_str(format_args!("{v:?}")));
        return Self { message: msg };
    }
}

impl From<String> for WebGl2Error {
    fn from(s: String) -> Self {
        return Self { message: s };
    }
}

// ============================================================
//                    КОНТЕКСТ
// ============================================================

/// Получает WebGL2-контекст из canvas.
pub fn context(
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<web_sys::WebGl2RenderingContext, WebGl2Error> {
    let ctx = canvas
        .get_context("webgl2")
        .map_err(WebGl2Error::from)?
        .ok_or_else(|| WebGl2Error::new("webgl2 context unavailable".to_string()))?;
    let gl: web_sys::WebGl2RenderingContext = ctx
        .dyn_into()
        .map_err(|_| WebGl2Error::new("not a WebGl2RenderingContext".to_string()))?;
    return Ok(gl);
}

// ============================================================
//                    CAPABILITIES
// ============================================================

/// Возможности конкретного WebGL2-контекста.
pub struct Capabilities {
    pub max_vertex_attribs: i32,
    pub max_vertex_uniform_vectors: i32,
    pub max_texture_size: i32,
    pub max_3d_texture_size: i32,
    pub max_samples: i32,
    pub max_elements_vertices: i32,
    /// EXT_color_buffer_float — рендер в floating-point текстуры.
    pub color_buffer_float: bool,
    /// EXT_disjoint_timer_query_webgl2 — точные GPU-тайминги.
    pub timer_query: bool,
    /// OES_texture_float_linear — линейная фильтрация FP-текстур.
    pub float_linear: bool,
}

/// Запрашивает capabilities из контекста.
pub fn query_capabilities(gl: &web_sys::WebGl2RenderingContext) -> Capabilities {
    let get_i32 = |pname: u32| -> i32 {
        gl.get_parameter(pname)
            .ok()
            .and_then(|v| v.as_f64())
            .map(|f| f as i32)
            .unwrap_or(0)
    };
    let has_ext = |name: &str| -> bool {
        gl.get_extension(name).ok().flatten().is_some()
    };
    return Capabilities {
        max_vertex_attribs: get_i32(web_sys::WebGl2RenderingContext::MAX_VERTEX_ATTRIBS),
        max_vertex_uniform_vectors: get_i32(web_sys::WebGl2RenderingContext::MAX_VERTEX_UNIFORM_VECTORS),
        max_texture_size: get_i32(web_sys::WebGl2RenderingContext::MAX_TEXTURE_SIZE),
        max_3d_texture_size: get_i32(web_sys::WebGl2RenderingContext::MAX_3D_TEXTURE_SIZE),
        max_samples: get_i32(web_sys::WebGl2RenderingContext::MAX_SAMPLES),
        max_elements_vertices: get_i32(web_sys::WebGl2RenderingContext::MAX_ELEMENTS_VERTICES),
        color_buffer_float: has_ext("EXT_color_buffer_float"),
        timer_query: has_ext("EXT_disjoint_timer_query_webgl2"),
        float_linear: has_ext("OES_texture_float_linear"),
    };
}

// ============================================================
//                    PROGRAM WRAPPER
// ============================================================

/// Программа с кэшем uniform/attrib-локаций.
///
/// Кэширование критично в hot loop: `getUniformLocation` — это
/// синхронный вызов JS↔GPU. Кэш на HashMap делает его бесплатным.
pub struct Program {
    pub raw: web_sys::WebGlProgram,
    uniforms: HashMap<String, Option<web_sys::WebGlUniformLocation>>,
    attributes: HashMap<String, u32>,
}

impl Program {
    pub fn new(raw: web_sys::WebGlProgram) -> Self {
        return Self {
            raw,
            uniforms: HashMap::new(),
            attributes: HashMap::new(),
        };
    }

    pub fn uniform_location(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
        name: &str,
    ) -> Option<web_sys::WebGlUniformLocation> {
        if let Some(cached) = self.uniforms.get(name) {
            return cached.clone();
        }
        let loc = gl.get_uniform_location(&self.raw, name);
        self.uniforms.insert(name.to_string(), loc.clone());
        return loc;
    }

    /// `get_attrib_location` возвращает `i32`, но все последующие
    /// вызовы принимают `u32`. Кастуем один раз при кэшировании.
    pub fn attrib_location(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
        name: &str,
    ) -> u32 {
        if let Some(cached) = self.attributes.get(name) {
            return *cached;
        }
        let loc_i32 = gl.get_attrib_location(&self.raw, name);
        let loc = loc_i32 as u32;
        self.attributes.insert(name.to_string(), loc);
        return loc;
    }

    pub fn use_program(&self, gl: &web_sys::WebGl2RenderingContext) {
        gl.use_program(Some(&self.raw));
    }
}

// ============================================================
//                    CHECK ERROR
// ============================================================

/// Читает код ошибки из GL. `None` = всё в порядке.
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