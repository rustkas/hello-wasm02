//! GpuBuffer: создание, запись, чтение.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::{WebGpuError, call_method0, call_method1, call_method_apply, get_property, set_prop};

const USAGE_MAP_READ: u32 = 0x0001;
const USAGE_COPY_DST: u32 = 0x0008;

pub fn create(device: &web_sys::GpuDevice, size_bytes: u32, usage: u32) -> Result<web_sys::GpuBuffer, WebGpuError> {
    let desc = js_sys::Object::new();
    set_prop(&desc, "size", &JsValue::from_f64(size_bytes as f64))?;
    set_prop(&desc, "usage", &JsValue::from_f64(usage as f64))?;
    let buf_js = call_method1(device.as_ref(), "createBuffer", &JsValue::from(desc))?;
    let buf: web_sys::GpuBuffer = buf_js.dyn_into()
        .map_err(|_| WebGpuError::new("createBuffer: unexpected type".to_string()))?;
    return Ok(buf);
}

pub fn write_f32(device: &web_sys::GpuDevice, buffer: &web_sys::GpuBuffer, data: &[f32]) -> Result<(), WebGpuError> {
    let queue_js = get_property(device.as_ref(), "queue")?;
    let arr = js_sys::Float32Array::from(data);
    let args = js_sys::Array::new();
    args.push(buffer.as_ref());
    args.push(&JsValue::from_f64(0.0));
    args.push(&arr.into());
    call_method_apply(&queue_js, "writeBuffer", &args)?;
    return Ok(());
}

pub async fn read_f32(device: &web_sys::GpuDevice, source: &web_sys::GpuBuffer, count: u32) -> Result<Vec<f32>, WebGpuError> {
    let size_bytes = count * 4;
    let staging = create(device, size_bytes, USAGE_MAP_READ | USAGE_COPY_DST)?;

    // encoder.copyBufferToBuffer(source, 0, staging, 0, size_bytes)
    let encoder_js = call_method0(device.as_ref(), "createCommandEncoder")?;
    let copy_args = js_sys::Array::new();
    copy_args.push(source.as_ref());
    copy_args.push(&JsValue::from_f64(0.0));
    copy_args.push(staging.as_ref());
    copy_args.push(&JsValue::from_f64(0.0));
    copy_args.push(&JsValue::from_f64(size_bytes as f64));
    call_method_apply(&encoder_js, "copyBufferToBuffer", &copy_args)?;

    // cmd = encoder.finish()
    let cmd = call_method0(&encoder_js, "finish")?;
    // queue.submit([cmd])
    let queue_js = get_property(device.as_ref(), "queue")?;
    let submit_args = js_sys::Array::new();
    let cmds = js_sys::Array::new();
    cmds.push(&cmd);
    submit_args.push(&cmds);
    call_method_apply(&queue_js, "submit", &submit_args)?;

    // staging.mapAsync(READ) → Promise
    let map_promise: js_sys::Promise = call_method1(staging.as_ref(), "mapAsync", &JsValue::from_f64(USAGE_MAP_READ as f64))?
        .dyn_into()
        .map_err(|_| WebGpuError::new("mapAsync: not a Promise".to_string()))?;
    JsFuture::from(map_promise).await.map_err(WebGpuError::from)?;

    // ArrayBuffer = staging.getMappedRange()
    let mapped_js = call_method0(staging.as_ref(), "getMappedRange")?;
    let array_buffer: js_sys::ArrayBuffer = mapped_js.dyn_into()
        .map_err(|_| WebGpuError::new("getMappedRange: not ArrayBuffer".to_string()))?;
    let f32_view = js_sys::Float32Array::new(&array_buffer);
    let result = f32_view.to_vec();

    // staging.unmap()
    call_method0(staging.as_ref(), "unmap")?;

    return Ok(result);
}