//! WASM-тесты главы 42: WebGL.
//!
//! Запуск:
//!   wasm-pack test --chrome --headless -- --test wasm_webgl
//!
//! WebGL требует реального GPU-контекста. В headless Chrome
//! используется программная растеризация (SwiftShader), но
//! API работает полностью.

use wasm_bindgen_test::*;

use hello_wasm02::webgl::{
    self,
    buffer, geometry, shader,
};

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// 1. Геометрия — чистая математика, без GPU
// ============================================================

#[wasm_bindgen_test]
fn wasm_triangle_vertex_count() {
    let data = geometry::triangle();
    // 3 вершины × 5 float (x, y, r, g, b)
    assert_eq!(data.len(), 15);
}

#[wasm_bindgen_test]
fn wasm_quad_vertex_count() {
    let data = geometry::quad();
    // 6 вершин (2 треугольника) × 4 float (x, y, u, v)
    assert_eq!(data.len(), 24);
}

#[wasm_bindgen_test]
fn wasm_triangle_bounds() {
    let data = geometry::triangle();
    for (i, v) in data.iter().enumerate() {
        if i % 5 < 2 {
            // координаты x, y ∈ [-1, 1]
            assert!(*v >= -1.0 && *v <= 1.0, "vertex out of bounds");
        } else {
            // цвета ∈ [0, 1]
            assert!(*v >= 0.0 && *v <= 1.0, "color out of range");
        }
    }
}

#[wasm_bindgen_test]
fn wasm_quad_uv_range() {
    let data = geometry::quad();
    // UV-координаты ∈ [0, 1]
    for chunk in data.chunks(4) {
        assert!(chunk[2] >= 0.0 && chunk[2] <= 1.0, "u out of range");
        assert!(chunk[3] >= 0.0 && chunk[3] <= 1.0, "v out of range");
    }
}

// ============================================================
// 2. WebGL-контекст
// ============================================================

fn make_canvas() -> web_sys::HtmlCanvasElement {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_width(64);
    canvas.set_height(64);
    return canvas;
}

use wasm_bindgen::JsCast;

#[wasm_bindgen_test]
fn wasm_canvas_context_available() {
    let canvas = make_canvas();
    let ctx = canvas.get_context("webgl2").unwrap();
    assert!(ctx.is_some(), "webgl2 context должен быть доступен");
}

#[wasm_bindgen_test]
fn wasm_context_returns_valid_gl() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).expect("context() должен вернуть Ok");
    let version = gl
        .get_parameter(web_sys::WebGl2RenderingContext::VERSION)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    assert!(!version.is_empty());
    assert!(version.contains("WebGL"), "version: {version}");
}

// ============================================================
// 3. Компиляция шейдеров
// ============================================================

const GOOD_VS: &str = r#"#version 300 es
in vec2 a_position;
void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

const GOOD_FS: &str = r#"#version 300 es
precision mediump float;
out vec4 outColor;
void main() {
    outColor = vec4(1.0, 0.0, 0.0, 1.0);
}
"#;

const BAD_VS: &str = r#"#version 300 es
this is not valid GLSL
"#;

#[wasm_bindgen_test]
fn wasm_compile_valid_vertex_shader() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    let result = shader::compile_vertex(&gl, GOOD_VS);
    assert!(result.is_ok(), "валидный шейдер должен компилироваться");
}

#[wasm_bindgen_test]
fn wasm_compile_valid_fragment_shader() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    let result = shader::compile_fragment(&gl, GOOD_FS);
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn wasm_compile_invalid_shader_fails() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    let result = shader::compile_vertex(&gl, BAD_VS);
    assert!(result.is_err(), "невалидный шейдер должен дать ошибку");
}

#[wasm_bindgen_test]
fn wasm_link_program_ok() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    let vs = shader::compile_vertex(&gl, GOOD_VS).unwrap();
    let fs = shader::compile_fragment(&gl, GOOD_FS).unwrap();
    let program = shader::link_program(&gl, &vs, &fs);
    assert!(program.is_ok());
}

// ============================================================
// 4. Создание буфера
// ============================================================

#[wasm_bindgen_test]
fn wasm_create_f32_buffer() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    let data = geometry::triangle();
    let result = buffer::create_f32_buffer(
        &gl, &data,
        web_sys::WebGl2RenderingContext::STATIC_DRAW,
    );
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn wasm_create_vao_with_attrs() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    let data = geometry::triangle();
    let vbo = buffer::create_f32_buffer(
        &gl, &data,
        web_sys::WebGl2RenderingContext::STATIC_DRAW,
    ).unwrap();
    // stride = 5 * 4 байта, два атрибута: position(2), color(3).
    let attrs = [
        (0u32, 2i32, 5 * 4, 0),
        (1u32, 3i32, 5 * 4, 2 * 4),
    ];
    let result = buffer::create_vao_with_attrs(&gl, &vbo, &attrs);
    assert!(result.is_ok(), "VAO должен создаться");
}

// ============================================================
// 5. Полный pipeline — компиляция + линковка + VBO + VAO
// ============================================================

#[wasm_bindgen_test]
fn wasm_full_pipeline_init() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();

    let vs = shader::compile_vertex(&gl, GOOD_VS).unwrap();
    let fs = shader::compile_fragment(&gl, GOOD_FS).unwrap();
    let program = shader::link_program(&gl, &vs, &fs).unwrap();

    let data = geometry::triangle();
    let vbo = buffer::create_f32_buffer(
        &gl, &data,
        web_sys::WebGl2RenderingContext::STATIC_DRAW,
    ).unwrap();

    // Позиционный атрибут должен существовать.
    let pos_attr = gl.get_attrib_location(&program, "a_position");
    assert!(pos_attr >= 0, "a_position должен быть найден");

    // VAO с двумя атрибутами: position(2) + color(3),
    // stride = 5 * 4 байта.
    let attrs = [
        (pos_attr as u32, 2i32, 5 * 4, 0),
        (pos_attr as u32 + 1, 3i32, 5 * 4, 2 * 4),
    ];
    let vao = buffer::create_vao_with_attrs(&gl, &vbo, &attrs);
    assert!(vao.is_ok(), "VAO должен создаться в полном pipeline");

    // Проверка ошибок GL после инициализации.
    let err = webgl::check_error(&gl);
    assert!(err.is_none(), "не должно быть GL-ошибок: {err:?}");
}

// ============================================================
// 6. Проверка ошибок GL
// ============================================================

#[wasm_bindgen_test]
fn wasm_check_error_clean() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    // После чистого контекста ошибок нет.
    assert!(webgl::check_error(&gl).is_none());
}

#[wasm_bindgen_test]
fn wasm_check_error_after_invalid_call() {
    let canvas = make_canvas();
    let gl = webgl::context(&canvas).unwrap();
    // Дёргаем заведомо неверный параметр — должна быть INVALID_ENUM.
    let _ = gl.get_parameter(0xDEADBEEF);
    let err = webgl::check_error(&gl);
    assert!(err.is_some(), "должна быть зафиксирована ошибка");
}