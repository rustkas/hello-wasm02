//! Обёртка над WebGlProgram с кэшированием uniform-локаций.

use std::collections::HashMap;

/// Программа с кэшем uniform-локаций по имени.
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

    /// Возвращает локацию uniform-переменной (с кэшированием).
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

    /// Возвращает индекс атрибута (с кэшированием).
    ///
    /// `get_attrib_location` возвращает `i32`, но все последующие
    /// вызовы `enable_vertex_attrib_array` / `vertex_attrib_pointer_with_i32`
    /// принимают `u32`. Приводим один раз при кэшировании.
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