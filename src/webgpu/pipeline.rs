//! Shader module, compute pipeline, bind group.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::{WebGpuError, call_method1, set_prop};

pub fn create_shader_module(device: &web_sys::GpuDevice, wgsl: &str) -> Result<web_sys::GpuShaderModule, WebGpuError> {
    let desc = js_sys::Object::new();
    set_prop(&desc, "code", &JsValue::from_str(wgsl))?;
    let m_js = call_method1(device.as_ref(), "createShaderModule", &JsValue::from(desc))?;
    let m: web_sys::GpuShaderModule = m_js.dyn_into()
        .map_err(|_| WebGpuError::new("createShaderModule: unexpected type".to_string()))?;
    return Ok(m);
}

/// Асинхронная версия. `createComputePipelineAsync` возвращает Promise,
/// который reject'ится с человекочитаемым сообщением, если WGSL-шейдер
/// невалиден. Синхронный `createComputePipeline` в этом случае молча
/// возвращает pipeline, который ничего не делает.
pub async fn create_compute_pipeline_async(
    device: &web_sys::GpuDevice,
    shader: &web_sys::GpuShaderModule,
    entry_point: &str,
) -> Result<web_sys::GpuComputePipeline, WebGpuError> {
    let compute = js_sys::Object::new();
    set_prop(&compute, "module", shader.as_ref())?;
    set_prop(&compute, "entryPoint", &JsValue::from_str(entry_point))?;

    let desc = js_sys::Object::new();
    set_prop(&desc, "layout", &JsValue::from_str("auto"))?;
    set_prop(&desc, "compute", &JsValue::from(compute))?;

    let promise_js = call_method1(device.as_ref(), "createComputePipelineAsync", &JsValue::from(desc))?;
    let promise: js_sys::Promise = promise_js.dyn_into()
        .map_err(|_| WebGpuError::new("createComputePipelineAsync: not a Promise".to_string()))?;
    let result = JsFuture::from(promise).await.map_err(WebGpuError::from)?;
    let p: web_sys::GpuComputePipeline = result.dyn_into()
        .map_err(|_| WebGpuError::new("createComputePipelineAsync: unexpected type".to_string()))?;
    return Ok(p);
}

/// `size` — реальный размер буфера в байтах. Передаём явно:
/// SwiftShader в некоторых версиях не понимает `size: 0`.
pub fn create_bind_group(
    device: &web_sys::GpuDevice,
    pipeline: &web_sys::GpuComputePipeline,
    input: &web_sys::GpuBuffer,
    input_size: u32,
    output: &web_sys::GpuBuffer,
    output_size: u32,
) -> Result<web_sys::GpuBindGroup, WebGpuError> {
    let layout = call_method1(pipeline.as_ref(), "getBindGroupLayout", &JsValue::from_f64(0.0))?;

    let entries = js_sys::Array::new();
    for (binding, buffer, size) in [
        (0.0, input,  input_size),
        (1.0, output, output_size),
    ] {
        let e = js_sys::Object::new();
        set_prop(&e, "binding", &JsValue::from_f64(binding))?;
        let resource = js_sys::Object::new();
        set_prop(&resource, "buffer", buffer.as_ref())?;
        set_prop(&resource, "offset", &JsValue::from_f64(0.0))?;
        set_prop(&resource, "size", &JsValue::from_f64(size as f64))?;
        set_prop(&e, "resource", &JsValue::from(resource))?;
        entries.push(&e);
    }

    let desc = js_sys::Object::new();
    set_prop(&desc, "layout", &layout)?;
    set_prop(&desc, "entries", &JsValue::from(entries))?;

    let bg_js = call_method1(device.as_ref(), "createBindGroup", &JsValue::from(desc))?;
    let bg: web_sys::GpuBindGroup = bg_js.dyn_into()
        .map_err(|_| WebGpuError::new("createBindGroup: unexpected type".to_string()))?;
    return Ok(bg);
}