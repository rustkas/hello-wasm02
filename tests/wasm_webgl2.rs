//! WASM-тесты главы 43: WebGL 2.
//!
//! Запуск:
//!   wasm-pack test --chrome --headless -- --test wasm_webgl2

use wasm_bindgen_test::*;
use wasm_bindgen::JsCast;

use hello_wasm02::webgl2::{
    self, geometry, instancing::InstancedRenderer, shader,
};

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// 1. Геометрия и генерация данных
// ============================================================

#[wasm_bindgen_test]
fn wasm_base_triangle_size() {
    let tri = geometry::base_triangle();
    // 3 вершины × 2 координаты.
    assert_eq!(tri.len(), 6);
}

#[wasm_bindgen_test]
fn wasm_generate_instances_size() {
    let data = geometry::generate_instances(100);
    // 100 инстансов × 6 float.
    assert_eq!(data.len(), 600);
}

#[wasm_bindgen_test]
fn wasm_instances_in_range() {
    let data = geometry::generate_instances(50);
    for chunk in data.chunks(6) {
        assert!(chunk[0] >= -0.9 && chunk[0] <= 0.9); // offset.x
        assert!(chunk[1] >= -0.9 && chunk[1] <= 0.9); // offset.y
        assert!(chunk[2] >= 0.25 && chunk[2] <= 1.0); // r
        assert!(chunk[5] > 0.0 && chunk[5] < 0.03);   // scale
    }
}

// ============================================================
// 2. WebGL2-контекст
// ============================================================

fn make_canvas() -> web_sys::HtmlCanvasElement {
    let doc = web_sys::window().unwrap().document().unwrap();
    let canvas: web_sys::HtmlCanvasElement = doc
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_width(64);
    canvas.set_height(64);
    return canvas;
}

#[wasm_bindgen_test]
fn wasm_webgl2_context_created() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).expect("webgl2 должен быть доступен");
    let version = gl
        .get_parameter(web_sys::WebGl2RenderingContext::VERSION)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    assert!(version.contains("WebGL 2.0"), "version: {version}");
}

// ============================================================
// 3. Capabilities
// ============================================================

#[wasm_bindgen_test]
fn wasm_capabilities_reasonable() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let caps = webgl2::query_capabilities(&gl);

    // Минимальные гарантии спецификации OpenGL ES 3.0.
    assert!(caps.max_vertex_attribs >= 16, "минимум 16 атрибутов");
    assert!(caps.max_vertex_uniform_vectors >= 128);
    assert!(caps.max_texture_size >= 2048);
    assert!(caps.max_3d_texture_size >= 256);
    assert!(caps.max_samples >= 4);
}

// ============================================================
// 4. Шейдеры с instancing
// ============================================================

const VS_INSTANCED: &str = r#"#version 300 es
in vec2 a_position;    // per-vertex
in vec2 a_instance;    // per-instance: offset
in vec3 a_color;       // per-instance: color
in float a_scale;      // per-instance: scale

uniform float u_time;
out vec3 v_color;

void main() {
    vec2 p = a_position * a_scale + a_instance;
    gl_Position = vec4(p, 0.0, 1.0);
    v_color = a_color;
}
"#;

const FS_PASSTHROUGH: &str = r#"#version 300 es
precision mediump float;
in vec3 v_color;
out vec4 outColor;
void main() {
    outColor = vec4(v_color, 1.0);
}
"#;

#[wasm_bindgen_test]
fn wasm_instanced_shader_compiles() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let vs = shader::compile_vertex(&gl, VS_INSTANCED);
    assert!(vs.is_ok(), "шейдер с instancing-атрибутами должен компилироваться");
    let fs = shader::compile_fragment(&gl, FS_PASSTHROUGH);
    assert!(fs.is_ok());
}

#[wasm_bindgen_test]
fn wasm_instanced_program_links() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let vs = shader::compile_vertex(&gl, VS_INSTANCED).unwrap();
    let fs = shader::compile_fragment(&gl, FS_PASSTHROUGH).unwrap();
    let program = shader::link(&gl, &vs, &fs);
    assert!(program.is_ok());
}

// ============================================================
// 5. InstancedRenderer
// ============================================================

#[wasm_bindgen_test]
fn wasm_instanced_renderer_creates() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let tri = geometry::base_triangle();
    let result = InstancedRenderer::new(&gl, VS_INSTANCED, FS_PASSTHROUGH, &tri);
    assert!(result.is_ok(), "InstancedRenderer должен создаться");
    let r = result.unwrap();
    assert_eq!(r.vertex_count, 3, "3 вершины в базовой геометрии");
    assert_eq!(r.loaded_instances, 0);
}

#[wasm_bindgen_test]
fn wasm_instanced_renderer_uploads() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let tri = geometry::base_triangle();
    let mut r = InstancedRenderer::new(&gl, VS_INSTANCED, FS_PASSTHROUGH, &tri).unwrap();

    let data = geometry::generate_instances(1000);
    let count = r.upload_instances(&gl, &data).unwrap();
    assert_eq!(count, 1000);
    assert_eq!(r.loaded_instances, 1000);
}

#[wasm_bindgen_test]
fn wasm_instanced_draw_ok() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let tri = geometry::base_triangle();
    let mut r = InstancedRenderer::new(&gl, VS_INSTANCED, FS_PASSTHROUGH, &tri).unwrap();
    let data = geometry::generate_instances(500);
    r.upload_instances(&gl, &data).unwrap();

    gl.clear_color(0.0, 0.0, 0.0, 1.0);
    gl.clear(web_sys::WebGl2RenderingContext::COLOR_BUFFER_BIT);

    let calls = r.draw(&gl, 0.0);
    assert_eq!(calls, 1, "instancing = 1 draw call на N инстансов");

    let err = webgl2::check_error(&gl);
    assert!(err.is_none(), "не должно быть GL-ошибок: {err:?}");
}

#[wasm_bindgen_test]
fn wasm_instanced_draw_zero_instances() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let tri = geometry::base_triangle();
    let mut r = InstancedRenderer::new(&gl, VS_INSTANCED, FS_PASSTHROUGH, &tri).unwrap();
    // Без загрузки данных — draw должен вернуть 0.
    let calls = r.draw(&gl, 0.0);
    assert_eq!(calls, 0);
}

// ============================================================
// 6. Большой N
// ============================================================

#[wasm_bindgen_test]
fn wasm_instanced_10k_ok() {
    let canvas = make_canvas();
    let gl = webgl2::context(&canvas).unwrap();
    let tri = geometry::base_triangle();
    let mut r = InstancedRenderer::new(&gl, VS_INSTANCED, FS_PASSTHROUGH, &tri).unwrap();
    let data = geometry::generate_instances(10_000);
    r.upload_instances(&gl, &data).unwrap();
    let calls = r.draw(&gl, 0.0);
    assert_eq!(calls, 1);
    assert!(webgl2::check_error(&gl).is_none());
}