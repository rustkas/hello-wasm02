//! Создание и заполнение вершинных буферов (VBO) и VAO.

use super::WebGlError;

/// Создаёт буфер и загружает в него массив f32.
///
/// Возвращает `WebGlBuffer`, готовый к использованию через
/// `gl.bind_buffer(ARRAY_BUFFER, &buf)`.
pub fn create_f32_buffer(
    gl: &web_sys::WebGl2RenderingContext,
    data: &[f32],
    usage: u32,
) -> Result<web_sys::WebGlBuffer, WebGlError> {
    let buffer = gl
        .create_buffer()
        .ok_or_else(|| WebGlError::new("create_buffer failed".to_string()))?;
    gl.bind_buffer(web_sys::WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));

    // Создаём Float32Array поверх памяти Rust и передаём в GPU.
    let array = js_sys::Float32Array::from(data);
    gl.buffer_data_with_array_buffer_view(
        web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
        &array,
        usage,
    );
    return Ok(buffer);
}

/// Создаёт буфер для индексов (EBO).
pub fn create_index_buffer(
    gl: &web_sys::WebGl2RenderingContext,
    indices: &[u16],
    usage: u32,
) -> Result<web_sys::WebGlBuffer, WebGlError> {
    let buffer = gl
        .create_buffer()
        .ok_or_else(|| WebGlError::new("create_buffer failed".to_string()))?;
    gl.bind_buffer(
        web_sys::WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
        Some(&buffer),
    );

    let array = js_sys::Uint16Array::from(indices);
    gl.buffer_data_with_array_buffer_view(
        web_sys::WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
        &array,
        usage,
    );
    return Ok(buffer);
}

/// Создаёт VAO и настраивает атрибуты — состояние сохраняется
/// между draw-вызовами, не нужно переустанавливать каждый кадр.
///
/// `attrs` — срез кортежей `(location, size, stride, offset)`:
///   - `location` — индекс атрибута (`get_attrib_location`);
///   - `size`     — число компонент (1..4);
///   - `stride`   — размер шага в байтах;
///   - `offset`   — смещение внутри шага в байтах.
pub fn create_vao_with_attrs(
    gl: &web_sys::WebGl2RenderingContext,
    vbo: &web_sys::WebGlBuffer,
    attrs: &[(u32, i32, i32, i32)],
) -> Result<web_sys::WebGlVertexArrayObject, WebGlError> {
    let vao = gl
        .create_vertex_array()
        .ok_or_else(|| WebGlError::new("create_vertex_array failed".to_string()))?;

    // Привязываем VAO: все последующие вызовы enable_vertex_attrib_array
    // и vertex_attrib_pointer_with_i32 записываются в него.
    gl.bind_vertex_array(Some(&vao));
    gl.bind_buffer(
        web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
        Some(vbo),
    );

    for &(loc, size, stride, offset) in attrs {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(
            loc,
            size,
            web_sys::WebGl2RenderingContext::FLOAT,
            false,
            stride,
            offset,
        );
    }

    // Снимаем привязку VAO, чтобы случайно не изменить его.
    gl.bind_vertex_array(None);
    return Ok(vao);
}