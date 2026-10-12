//! Компиляция и линковка шейдеров WebGL2.
//!
//! Отличие от главы 42: поддержка `transformFeedbackVaryings` —
//! список выходов вершинного шейдера, которые пойдут в буфер.

use crate::format_str::format_str;
use super::WebGl2Error;

/// Компилирует вершинный шейдер.
pub fn compile_vertex(
    gl: &web_sys::WebGl2RenderingContext,
    source: &str,
) -> Result<web_sys::WebGlShader, WebGl2Error> {
    return compile(gl, source, web_sys::WebGl2RenderingContext::VERTEX_SHADER);
}

/// Компилирует фрагментный шейдер.
pub fn compile_fragment(
    gl: &web_sys::WebGl2RenderingContext,
    source: &str,
) -> Result<web_sys::WebGlShader, WebGl2Error> {
    return compile(gl, source, web_sys::WebGl2RenderingContext::FRAGMENT_SHADER);
}

fn compile(
    gl: &web_sys::WebGl2RenderingContext,
    source: &str,
    kind: u32,
) -> Result<web_sys::WebGlShader, WebGl2Error> {
    let shader = gl
        .create_shader(kind)
        .ok_or_else(|| WebGl2Error::new("create_shader failed".to_string()))?;
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
        return Err(WebGl2Error::new(format_str(format_args!(
            "shader compile error: {log}",
        ))));
    }
    return Ok(shader);
}

/// Линкует программу без transform feedback.
pub fn link(
    gl: &web_sys::WebGl2RenderingContext,
    vs: &web_sys::WebGlShader,
    fs: &web_sys::WebGlShader,
) -> Result<web_sys::WebGlProgram, WebGl2Error> {
    return link_with_varyings(gl, vs, fs, None);
}

/// Линкует программу с опциональным transform feedback.
///
/// `varyings` — список имён выходов вершинного шейдера, которые
/// будут записаны в `TRANSFORM_FEEDBACK_BUFFER`. Передаётся
/// **до** `link_program`.
pub fn link_with_varyings(
    gl: &web_sys::WebGl2RenderingContext,
    vs: &web_sys::WebGlShader,
    fs: &web_sys::WebGlShader,
    varyings: Option<&[&str]>,
) -> Result<web_sys::WebGlProgram, WebGl2Error> {
    let program = gl
        .create_program()
        .ok_or_else(|| WebGl2Error::new("create_program failed".to_string()))?;
    gl.attach_shader(&program, vs);
    gl.attach_shader(&program, fs);

    // transformFeedbackVaryings должен вызываться до link_program.
    if let Some(names) = varyings {
        let array = js_sys::Array::new();
        for n in names {
            array.push(&wasm_bindgen::JsValue::from_str(n));
        }
        gl.transform_feedback_varyings(
            &program,
            &array,
            web_sys::WebGl2RenderingContext::SEPARATE_ATTRIBS,
        );
    }

    gl.link_program(&program);

    let status = gl
        .get_program_parameter(&program, web_sys::WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !status {
        let log = gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "unknown error".to_string());
        return Err(WebGl2Error::new(format_str(format_args!(
            "program link error: {log}",
        ))));
    }
    return Ok(program);
}