//! 23.7. WebCodecs + WASM/GPU: обработка пикселей.
//!
//! Два пути обработки `VideoFrame`:
//!
//! - **CPU**: `copyTo(rgba)` → Rust-цикл → `putImageData`.
//! - **GPU**: `texImage2D(frame)` → WebGL2 shader.
//!
//! ## Варианты GpuInverter
//!
//! - `GpuInverter::new(w, h)` — владеет `OffscreenCanvas`. Используется
//!   в тестах: `apply(frame)` возвращает `ImageBitmap`.
//! - `GpuInverter::from_canvas(canvas)` — использует видимый
//!   WebGL-канвас. `render(frame)` рисует прямо в него, без
//!   `transferToImageBitmap` и `drawImage`.
//!
//! ## Замечания по web-sys
//!
//! - WebGL-константы — associated constants типа
//!   (`WebGl2RenderingContext::VERTEX_SHADER`), а не методы.
//! - Метод uniform называется `uniform1i`, не `uniform_1i`.
//! - `bufferData` и `texImage2D` вызываются через `invoke` —
//!   сигнатуры перегружены, типизированные методы нестабильны.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::Clamped;
use web_sys::{
    HtmlCanvasElement, ImageData, OffscreenCanvas,
    WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlShader,
    WebGlTexture, WebGlUniformLocation,
};

use crate::video_frame::{format_str, get_fn, invoke, num, set_prop};

// =============================================================================
// Чистые функции (host-testable)
// =============================================================================

/// Инвертирует RGB-каналы in-place. Alpha не трогает.
///
/// Self-inverse: двойное применение возвращает исходные данные.
/// Обрабатывает любой размер буфера: если длина не кратна 4,
/// оставшиеся байты пропускаются.
pub fn invert_rgba(bytes: &mut [u8]) {
    let full_pixels = bytes.len() / 4;
    for i in 0..full_pixels {
        let base = i * 4;
        bytes[base] = 255 - bytes[base];
        bytes[base + 1] = 255 - bytes[base + 1];
        bytes[base + 2] = 255 - bytes[base + 2];
    }
}

/// Средний RGB по всем пикселям. Alpha игнорируется.
///
/// Возвращает `(r, g, b)` в диапазоне 0.0–255.0. Для пустого
/// буфера — `(0.0, 0.0, 0.0)`.
pub fn average_rgb(bytes: &[u8]) -> (f32, f32, f32) {
    let full_pixels = bytes.len() / 4;
    if full_pixels == 0 {
        return (0.0, 0.0, 0.0);
    }
    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    for i in 0..full_pixels {
        let base = i * 4;
        r_sum += bytes[base] as u64;
        g_sum += bytes[base + 1] as u64;
        b_sum += bytes[base + 2] as u64;
    }
    let n = full_pixels as f32;
    (r_sum as f32 / n, g_sum as f32 / n, b_sum as f32 / n)
}

// =============================================================================
// Feature detection
// =============================================================================

/// `WebGL2RenderingContext` доступен на данном устройстве.
pub fn has_webgl2_support() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Some(document) = window.document() else {
        return false;
    };
    let Ok(el) = document.create_element("canvas") else {
        return false;
    };
    let Ok(canvas) = el.dyn_into::<HtmlCanvasElement>() else {
        return false;
    };
    match canvas.get_context("webgl2") {
        Ok(Some(_)) => true,
        _ => false,
    }
}

/// Возвращает строку, описывающую WebGL-backend.
///
/// ## Почему RENDERER, а не UNMASKED_RENDERER_WEBGL
///
/// `UNMASKED_RENDERER_WEBGL` (0x9246) даёт точное имя GPU, но
/// требует активации устаревающего расширения
/// `WEBGL_debug_renderer_info`. Firefox уже помечает его как
/// deprecated, Chrome следует за ним.
///
/// `RENDERER` (0x1F01) — стандартный параметр без расширения.
/// Может вернуть masked-строку с суффиксом «or similar», но она
/// всё равно содержит имя backend'а (Intel / NVIDIA / SwiftShader),
/// чего достаточно для наших целей.
///
/// ## Примеры значений
///
/// - `"ANGLE (Intel, Intel(R) HD Graphics Direct3D11 vs_5_0 ps_5_0), or similar"`
/// - `"ANGLE (NVIDIA, NVIDIA GeForce RTX ...), or similar"`
/// - `"Google SwiftShader"` — software-рендер.
pub fn get_webgl_renderer() -> Option<String> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let el = document.create_element("canvas").ok()?;
    let canvas = el.dyn_into::<HtmlCanvasElement>().ok()?;
    let ctx = canvas.get_context("webgl2").ok()??;
    let gl = ctx.dyn_into::<WebGl2RenderingContext>().ok()?;

    // gl.RENDERER = 0x1F01. Стандартный параметр, без расширений.
    gl.get_parameter(0x1F01)
        .ok()?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// `true`, если рендеринг идёт через софтверный backend
/// (SwiftShader на Linux headless, llvmpipe и т. п.).
///
/// В таких окружениях WebGL не даёт преимущества — вся работа
/// выполняется на CPU. Демо предупреждает пользователя.
pub fn is_software_renderer() -> bool {
    get_webgl_renderer()
        .map(|s| {
            let lower = s.to_lowercase();
            lower.contains("swiftshader")
                || lower.contains("llvmpipe")
                || lower.contains("software")
                || lower.contains("softpipe")
        })
        .unwrap_or(false)
}

// =============================================================================
// WebGL2 шейдеры
// =============================================================================

const VERTEX_SHADER_SRC: &str = r#"#version 300 es
in vec2 a_pos;
out vec2 v_uv;
void main() {
    v_uv = (a_pos + 1.0) * 0.5;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const FRAGMENT_SHADER_SRC: &str = r#"#version 300 es
precision highp float;
in vec2 v_uv;
out vec4 out_color;
uniform sampler2D u_tex;
void main() {
    vec4 c = texture(u_tex, v_uv);
    out_color = vec4(1.0 - c.rgb, c.a);
}
"#;

// =============================================================================
// GpuInverter
// =============================================================================

/// GPU-инвертор одного кадра.
///
/// Владеет WebGL2-контекстом и всеми ресурсами (программа, VBO,
/// текстура). Если создан через `new(w, h)` — владеет и
/// `OffscreenCanvas` для `apply()`. Если через `from_canvas()` —
/// рисует прямо в переданный canvas.
pub struct GpuInverter {
    gl: WebGl2RenderingContext,
    program: WebGlProgram,
    u_tex: WebGlUniformLocation,
    /// VBO и текстура держатся в структуре, чтобы не удалились
    /// сборщиком мусора до конца работы.
    _vbo: WebGlBuffer,
    _texture: WebGlTexture,
    width: u32,
    height: u32,
    /// Some — владеем OffscreenCanvas, можем вызвать
    /// `transfer_to_image_bitmap`. None — рисуем в чужой canvas.
    owned_canvas: Option<OffscreenCanvas>,
}

impl GpuInverter {
    /// Создаёт инвертор с внутренним `OffscreenCanvas`.
    ///
    /// `apply(frame)` после рендера вернёт `ImageBitmap`.
    pub fn new(width: u32, height: u32) -> Result<Self, JsValue> {
        let canvas = OffscreenCanvas::new(width, height)?;
        let gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("webgl2 not available"))?
            .dyn_into::<WebGl2RenderingContext>()?;
        let mut inv = Self::from_gl(gl, width, height)?;
        inv.owned_canvas = Some(canvas);
        Ok(inv)
    }

    /// Создаёт инвертор на базе видимого `<canvas>`.
    ///
    /// `render(frame)` рисует прямо в этот canvas — без
    /// `transferToImageBitmap` и `drawImage`. Быстрее, чем `apply`,
    /// потому что не создаёт промежуточный `ImageBitmap`.
    pub fn from_canvas(canvas: &HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width();
        let height = canvas.height();
        let gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("webgl2 not available on canvas"))?
            .dyn_into::<WebGl2RenderingContext>()?;
        Self::from_gl(gl, width, height)
    }

    /// Общая инициализация: шейдеры, буферы, текстура.
    fn from_gl(
        gl: WebGl2RenderingContext,
        width: u32,
        height: u32,
    ) -> Result<Self, JsValue> {
        let vert = compile_shader(
            &gl,
            WebGl2RenderingContext::VERTEX_SHADER,
            VERTEX_SHADER_SRC,
        )?;
        let frag = compile_shader(
            &gl,
            WebGl2RenderingContext::FRAGMENT_SHADER,
            FRAGMENT_SHADER_SRC,
        )?;
        let program = link_program(&gl, &vert, &frag)?;

        let vbo = gl
            .create_buffer()
            .ok_or_else(|| JsValue::from_str("create_buffer failed"))?;
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&vbo));
        let verts: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
        let verts_arr = js_sys::Float32Array::from(&verts[..]);
        invoke(
            gl.as_ref(),
            "bufferData",
            &[
                num(WebGl2RenderingContext::ARRAY_BUFFER as f64),
                verts_arr.into(),
                num(WebGl2RenderingContext::STATIC_DRAW as f64),
            ],
        )?;

        let a_pos = gl.get_attrib_location(&program, "a_pos") as u32;
        gl.enable_vertex_attrib_array(a_pos);
        gl.vertex_attrib_pointer_with_i32(
            a_pos,
            2,
            WebGl2RenderingContext::FLOAT,
            false,
            0,
            0,
        );

        let texture = gl
            .create_texture()
            .ok_or_else(|| JsValue::from_str("create_texture failed"))?;
        gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&texture));
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_MIN_FILTER,
            WebGl2RenderingContext::NEAREST as i32,
        );
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_MAG_FILTER,
            WebGl2RenderingContext::NEAREST as i32,
        );
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_WRAP_S,
            WebGl2RenderingContext::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_WRAP_T,
            WebGl2RenderingContext::CLAMP_TO_EDGE as i32,
        );
        gl.pixel_storei(WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL, 1);

        let u_tex = gl
            .get_uniform_location(&program, "u_tex")
            .ok_or_else(|| JsValue::from_str("u_tex not found"))?;

        Ok(Self {
            gl,
            program,
            u_tex,
            _vbo: vbo,
            _texture: texture,
            width,
            height,
            owned_canvas: None,
        })
    }

    /// Рендерит инвертированный кадр в свой GL-контекст.
    ///
    /// Для `from_canvas()` — результат появляется прямо в видимом
    /// canvas. Для `new()` — результат внутри OffscreenCanvas,
    /// доступен через `apply()`.
    pub fn render(&self, frame: &JsValue) -> Result<(), JsValue> {
        let gl = &self.gl;
        gl.viewport(0, 0, self.width as i32, self.height as i32);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

        gl.use_program(Some(&self.program));
        gl.active_texture(WebGl2RenderingContext::TEXTURE0);
        gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&self._texture));

        invoke(
            gl.as_ref(),
            "texImage2D",
            &[
                num(WebGl2RenderingContext::TEXTURE_2D as f64),
                num(0.0),
                num(WebGl2RenderingContext::RGBA as f64),
                num(WebGl2RenderingContext::RGBA as f64),
                num(WebGl2RenderingContext::UNSIGNED_BYTE as f64),
                frame.clone(),
            ],
        )?;

        gl.uniform1i(Some(&self.u_tex), 0);
        gl.draw_arrays(WebGl2RenderingContext::TRIANGLE_STRIP, 0, 4);
        Ok(())
    }

    /// Рендерит и забирает результат как `ImageBitmap`.
    ///
    /// Работает только для варианта, созданного через `new()`
    /// (у которого есть OffscreenCanvas).
    pub fn apply(&self, frame: &JsValue) -> Result<JsValue, JsValue> {
        self.render(frame)?;
        let canvas = self
            .owned_canvas
            .as_ref()
            .ok_or_else(|| {
                JsValue::from_str(
                    "apply() требует вариант, созданный через GpuInverter::new()",
                )
            })?;
        let bitmap = canvas.transfer_to_image_bitmap()?;
        Ok(bitmap.into())
    }
}

// =============================================================================
// Компиляция шейдеров
// =============================================================================

fn compile_shader(
    gl: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| JsValue::from_str("create_shader failed"))?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    let success = gl
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !success {
        let log = gl.get_shader_info_log(&shader).unwrap_or_default();
        gl.delete_shader(Some(&shader));
        return Err(JsValue::from_str(&format_str(format_args!(
            "shader compile error: {log}"
        ))));
    }
    Ok(shader)
}

fn link_program(
    gl: &WebGl2RenderingContext,
    vert: &WebGlShader,
    frag: &WebGlShader,
) -> Result<WebGlProgram, JsValue> {
    let program = gl
        .create_program()
        .ok_or_else(|| JsValue::from_str("create_program failed"))?;
    gl.attach_shader(&program, vert);
    gl.attach_shader(&program, frag);
    gl.link_program(&program);
    let success = gl
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);
    if !success {
        let log = gl.get_program_info_log(&program).unwrap_or_default();
        gl.delete_program(Some(&program));
        return Err(JsValue::from_str(&format_str(format_args!(
            "program link error: {log}"
        ))));
    }
    Ok(program)
}

// =============================================================================
// CPU-инверсия VideoFrame
// =============================================================================

/// Извлекает пиксели из `VideoFrame` в RGBA-буфер через `copyTo`.
///
/// Возвращает `Vec<u8>` размером `w * h * 4`. Для canvas-based
/// frame работает всегда; для ImageDecoder-generated может
/// падать с `No media resource` (см. 23.6).
pub async fn copy_frame_to_rgba(
    frame: &JsValue,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, JsValue> {
    if width == 0 || height == 0 {
        return Err(JsValue::from_str("copy_frame_to_rgba: zero dimensions"));
    }
    let size = (width as usize) * (height as usize) * 4;
    let buffer = js_sys::Uint8Array::new_with_length(size as u32);
    let opts = js_sys::Object::new();
    set_prop(&opts, "format", &JsValue::from_str("RGBA"))?;
    let f = get_fn(frame, "copyTo")?;
    let promise = f
        .call2(frame, &buffer, &opts)?
        .dyn_into::<js_sys::Promise>()?;
    wasm_bindgen_futures::JsFuture::from(promise).await?;
    Ok(buffer.to_vec())
}

/// Кладёт RGBA-буфер на canvas через `ImageData` + `putImageData`.
///
/// `putImageData` вызывается через `invoke`: сигнатура `dx`/`dy`
/// меняется между stable и unstable-сборками web-sys.
pub fn blit_rgba_to_canvas(
    canvas: &HtmlCanvasElement,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(), JsValue> {
    if width == 0 || height == 0 {
        return Err(JsValue::from_str("blit_rgba_to_canvas: zero dimensions"));
    }
    if canvas.width() != width {
        canvas.set_width(width);
    }
    if canvas.height() != height {
        canvas.set_height(height);
    }
    let clamped = Clamped(&rgba[..]);
    let img = ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height)?;
    let ctx = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d"))?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
    invoke(
        ctx.as_ref(),
        "putImageData",
        &[img.into(), num(0.0), num(0.0)],
    )?;
    Ok(())
}

/// Рисует `ImageBitmap` на видимом 2d-canvas через `drawImage`.
///
/// Оставлено для случаев, когда GPU-путь идёт через ImageBitmap
/// (вариант `GpuInverter::new` + `apply`).
pub fn blit_bitmap_to_canvas(
    canvas: &HtmlCanvasElement,
    bitmap: &JsValue,
    width: u32,
    height: u32,
) -> Result<(), JsValue> {
    if canvas.width() != width {
        canvas.set_width(width);
    }
    if canvas.height() != height {
        canvas.set_height(height);
    }
    let ctx = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d"))?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
    invoke(
        ctx.as_ref(),
        "drawImage",
        &[
            bitmap.clone(),
            num(0.0),
            num(0.0),
            num(width as f64),
            num(height as f64),
        ],
    )?;
    Ok(())
}


/// Сокращает длинную строку renderer'а до ключевого слова для UI.
///
/// `"ANGLE (Intel, Intel(R) HD Graphics Direct3D11 vs_5_0 ps_5_0), or similar"`
///   → `"Intel HD Graphics"`
/// `"Google SwiftShader"` → `"SwiftShader"`
/// `"ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 ...)"` → `"NVIDIA GeForce RTX 3060"`
pub fn shorten_renderer(s: &str) -> String {
    // Извлекаем первый осмысленный фрагмент после "ANGLE (Vendor, ".
    if let Some(rest) = s.strip_prefix("ANGLE (") {
        // Убираем хвост ", or similar" и последнюю ).
        let trimmed = rest.trim_end_matches(|c| c == ')' || c == ' ');
        let trimmed = trimmed.trim_end_matches(", or similar");
        // Ожидаем "Vendor, Model ...". Берём фрагмент после первой запятой.
        if let Some((_vendor, model)) = trimmed.split_once(", ") {
            // Обрезаем длинные suffixes вроде " Direct3D11 vs_5_0 ps_5_0".
            return model
                .split(" Direct3D")
                .next()
                .unwrap_or(model)
                .trim()
                .to_string();
        }
        return trimmed.to_string();
    }
    s.to_string()
}
