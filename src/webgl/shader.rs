//! Компиляция и линковка шейдеров.


use crate::format_str::format_str;
use super::WebGlError;

/// Компилирует вершинный шейдер из исходного кода.
pub fn compile_vertex(
    gl: &web_sys::WebGl2RenderingContext,
    source: &str,
) -> Result<web_sys::WebGlShader, WebGlError> {
    return compile(gl, source, web_sys::WebGl2RenderingContext::VERTEX_SHADER);
}

/// Компилирует фрагментный шейдер из исходного кода.
pub fn compile_fragment(
    gl: &web_sys::WebGl2RenderingContext,
    source: &str,
) -> Result<web_sys::WebGlShader, WebGlError> {
    return compile(gl, source, web_sys::WebGl2RenderingContext::FRAGMENT_SHADER);
}

fn compile(
    gl: &web_sys::WebGl2RenderingContext,
    source: &str,
    kind: u32,
) -> Result<web_sys::WebGlShader, WebGlError> {
    let shader = gl
        .create_shader(kind)
        .ok_or_else(|| WebGlError::new("create_shader failed".to_string()))?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    let status = gl
        .get_shader_parameter(&shader, web_sys::WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !status {
        let log = gl
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "unknown error".to_string());
        return Err(WebGlError::new(format_str(format_args!(
            "shader compile error: {log}",
        ))));
    }
    return Ok(shader);
}

/// Линкует вершинный и фрагментный шейдеры в программу.
pub fn link_program(
    gl: &web_sys::WebGl2RenderingContext,
    vs: &web_sys::WebGlShader,
    fs: &web_sys::WebGlShader,
) -> Result<web_sys::WebGlProgram, WebGlError> {
    let program = gl
        .create_program()
        .ok_or_else(|| WebGlError::new("create_program failed".to_string()))?;
    gl.attach_shader(&program, vs);
    gl.attach_shader(&program, fs);
    gl.link_program(&program);

    let status = gl
        .get_program_parameter(&program, web_sys::WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !status {
        let log = gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "unknown error".to_string());
        return Err(WebGlError::new(format_str(format_args!(
            "program link error: {log}",
        ))));
    }
    return Ok(program);
}