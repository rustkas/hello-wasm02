//! Загрузка текстур.


use super::WebGlError;

/// Создаёт текстуру из ImageData.
pub fn from_image_data(
    gl: &web_sys::WebGl2RenderingContext,
    image: &web_sys::ImageData,
) -> Result<web_sys::WebGlTexture, WebGlError> {
    let texture = gl
        .create_texture()
        .ok_or_else(|| WebGlError::new("create_texture failed".to_string()))?;
    gl.bind_texture(web_sys::WebGl2RenderingContext::TEXTURE_2D, Some(&texture));

    // Загружаем ImageData в текстуру.
    gl.tex_image_2d_with_u32_and_u32_and_image_data(
        web_sys::WebGl2RenderingContext::TEXTURE_2D,
        0,
        web_sys::WebGl2RenderingContext::RGBA as i32,
        web_sys::WebGl2RenderingContext::RGBA,
        web_sys::WebGl2RenderingContext::UNSIGNED_BYTE,
        image,
    ).map_err(|e| WebGlError::from(e))?;

    // Параметры фильтрации.
    gl.tex_parameteri(
        web_sys::WebGl2RenderingContext::TEXTURE_2D,
        web_sys::WebGl2RenderingContext::TEXTURE_MIN_FILTER,
        web_sys::WebGl2RenderingContext::LINEAR as i32,
    );
    gl.tex_parameteri(
        web_sys::WebGl2RenderingContext::TEXTURE_2D,
        web_sys::WebGl2RenderingContext::TEXTURE_MAG_FILTER,
        web_sys::WebGl2RenderingContext::LINEAR as i32,
    );

    return Ok(texture);
}