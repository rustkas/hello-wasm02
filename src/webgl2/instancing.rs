//! Инстансинг — главная фича WebGL 2.
//!
//! Идея: одна базовая геометрия (треугольник) переиспользуется
//! N раз. Каждый инстанс имеет собственные атрибуты — позицию,
//! цвет, масштаб — которые передаются в вершинный шейдер через
//! `vertexAttribDivisor(1)`.
//!
//! Сравнение:
//!   Без instancing: N × (3 вершины × 5 float) = 15N float в VBO.
//!   С instancing:  3 × 2 float (base) + N × 7 float (instance) = 6 + 7N.
//! Экономия — на порядок при больших N.

use super::{check_error, shader, Program, WebGl2Error};

/// Рендерер с инстансингом.
pub struct InstancedRenderer {
    pub program: Program,
    /// VAO хранит состояние всех атрибутов: base + instance.
    pub vao: web_sys::WebGlVertexArrayObject,
    /// Базовая геометрия (треугольник): 3 вершины × 2 float.
    pub base_vbo: web_sys::WebGlBuffer,
    /// Per-instance данные: offset.x, offset.y, r, g, b, scale.
    pub instance_vbo: web_sys::WebGlBuffer,
    /// Сколько инстансов реально загружено в instance_vbo.
    pub loaded_instances: usize,
    /// Сколько vertices в базовой геометрии.
    pub vertex_count: i32,
}

impl InstancedRenderer {
    /// Создаёт рендерер с заданной базовой геометрией.
    ///
    /// `base_data` — base geometry (например, 3 вершины треугольника: 6 float).
    /// `vs_source`, `fs_source` — шейдеры. Вершинный шейдер должен
    /// объявить `a_position` (per-vertex, divisor 0) и per-instance
    /// атрибуты `a_instance`, `a_color`, `a_scale` (divisor 1).
    pub fn new(
        gl: &web_sys::WebGl2RenderingContext,
        vs_source: &str,
        fs_source: &str,
        base_data: &[f32],
    ) -> Result<Self, WebGl2Error> {
        let vs = shader::compile_vertex(gl, vs_source)?;
        let fs = shader::compile_fragment(gl, fs_source)?;
        let raw = shader::link(gl, &vs, &fs)?;
        let mut program = Program::new(raw);

        // === Базовая геометрия ===
        let base_vbo = gl
            .create_buffer()
            .ok_or_else(|| WebGl2Error::new("create_buffer failed".to_string()))?;
        gl.bind_buffer(web_sys::WebGl2RenderingContext::ARRAY_BUFFER, Some(&base_vbo));
        let base_array = js_sys::Float32Array::from(base_data);
        gl.buffer_data_with_array_buffer_view(
            web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
            &base_array,
            web_sys::WebGl2RenderingContext::STATIC_DRAW,
        );

        // === Per-instance буфер (пустой, заполним позже) ===
        let instance_vbo = gl
            .create_buffer()
            .ok_or_else(|| WebGl2Error::new("create_buffer failed".to_string()))?;
        gl.bind_buffer(web_sys::WebGl2RenderingContext::ARRAY_BUFFER, Some(&instance_vbo));
        gl.buffer_data_with_array_buffer_view(
            web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
            &js_sys::Float32Array::new_with_length(0),
            web_sys::WebGl2RenderingContext::DYNAMIC_DRAW,
        );

        // === VAO: сохраняем всё состояние атрибутов ===
        let vao = gl
            .create_vertex_array()
            .ok_or_else(|| WebGl2Error::new("create_vertex_array failed".to_string()))?;
        gl.bind_vertex_array(Some(&vao));

        // Атрибут 0: a_position — per-vertex, divisor 0 (по умолчанию).
        let pos_loc = program.attrib_location(gl, "a_position");
        gl.bind_buffer(web_sys::WebGl2RenderingContext::ARRAY_BUFFER, Some(&base_vbo));
        gl.enable_vertex_attrib_array(pos_loc);
        gl.vertex_attrib_pointer_with_i32(
            pos_loc, 2,
            web_sys::WebGl2RenderingContext::FLOAT,
            false, 2 * 4, 0,
        );

        // stride = 6 float: offset.x, offset.y, r, g, b, scale.
        let stride = 6 * 4;

        // Атрибут 1: a_instance — offset (vec2).
        let inst_loc = program.attrib_location(gl, "a_instance");
        gl.bind_buffer(web_sys::WebGl2RenderingContext::ARRAY_BUFFER, Some(&instance_vbo));
        gl.enable_vertex_attrib_array(inst_loc);
        gl.vertex_attrib_pointer_with_i32(
            inst_loc, 2,
            web_sys::WebGl2RenderingContext::FLOAT,
            false, stride, 0,
        );
        gl.vertex_attrib_divisor(inst_loc, 1);

        // Атрибут 2: a_color — vec3, смещение 2*4.
        let color_loc = program.attrib_location(gl, "a_color");
        gl.enable_vertex_attrib_array(color_loc);
        gl.vertex_attrib_pointer_with_i32(
            color_loc, 3,
            web_sys::WebGl2RenderingContext::FLOAT,
            false, stride, 2 * 4,
        );
        gl.vertex_attrib_divisor(color_loc, 1);

        // Атрибут 3: a_scale — float, смещение 5*4.
        let scale_loc = program.attrib_location(gl, "a_scale");
        gl.enable_vertex_attrib_array(scale_loc);
        gl.vertex_attrib_pointer_with_i32(
            scale_loc, 1,
            web_sys::WebGl2RenderingContext::FLOAT,
            false, stride, 5 * 4,
        );
        gl.vertex_attrib_divisor(scale_loc, 1);

        gl.bind_vertex_array(None);

        let vertex_count = (base_data.len() / 2) as i32;
        return Ok(Self {
            program,
            vao,
            base_vbo,
            instance_vbo,
            loaded_instances: 0,
            vertex_count,
        });
    }

    /// Загружает per-instance данные.
    ///
    /// Если буфер уже был создан под большее N — используем
    /// `bufferSubData` для инкрементального обновления. Иначе
    /// пересоздаём через `bufferData`.
    ///
    /// Возвращает количество загруженных инстансов.
    pub fn upload_instances(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
        data: &[f32],
    ) -> Result<usize, WebGl2Error> {
        // Каждый инстанс — 6 float.
        let count = data.len() / 6;
        gl.bind_buffer(
            web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
            Some(&self.instance_vbo),
        );
        let array = js_sys::Float32Array::from(data);

        if count <= self.loaded_instances && self.loaded_instances > 0 {
            // Partial update: буфер уже достаточного размера,
            // перезаписываем первые count инстансов.
            gl.buffer_sub_data_with_i32_and_array_buffer_view(
                web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
                0,
                &array,
            );
        } else {
            // Полная загрузка: расширяем буфер.
            gl.buffer_data_with_array_buffer_view(
                web_sys::WebGl2RenderingContext::ARRAY_BUFFER,
                &array,
                web_sys::WebGl2RenderingContext::DYNAMIC_DRAW,
            );
        }
        self.loaded_instances = count;
        return Ok(count);
    }

    /// Один draw call на N инстансов.
    ///
    /// Возвращает число draw calls (всегда 1).
    ///
    /// `&mut self` — чтобы использовать кэш `uniform_location` в `Program`.
    pub fn draw(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
        time: f32,
    ) -> u32 {
        if self.loaded_instances == 0 {
            return 0;
        }
        self.program.use_program(gl);
        gl.bind_vertex_array(Some(&self.vao));

        // Кэш uniform-локаций внутри Program: 60 кадров в секунду
        // обходятся без синхронных getUniformLocation.
        if let Some(loc) = self.program.uniform_location(gl, "u_time") {
            gl.uniform1f(Some(&loc), time);
        }

        gl.draw_arrays_instanced(
            web_sys::WebGl2RenderingContext::TRIANGLES,
            0,
            self.vertex_count,
            self.loaded_instances as i32,
        );

        gl.bind_vertex_array(None);
        let _ = check_error(gl);
        return 1;
    }
}