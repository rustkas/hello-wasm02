//! Запуск compute shader на массиве f32.

use wasm_bindgen::JsValue;

use super::{WebGpuError, buffer, pipeline, call_method0, call_method1, call_method_apply, get_property};

const USAGE_STORAGE: u32 = 0x0080;
const USAGE_COPY_DST: u32 = 0x0008;
const USAGE_COPY_SRC: u32 = 0x0004;

pub async fn run_f32(
    device: &web_sys::GpuDevice,
    wgsl: &str,
    entry: &str,
    input: &[f32],
    workgroup_size: u32,
) -> Result<Vec<f32>, WebGpuError> {
    let count = input.len() as u32;
    if count == 0 { return Ok(Vec::new()); }
    let size_bytes = count * 4;

    let input_buf = buffer::create(device, size_bytes, USAGE_STORAGE | USAGE_COPY_DST)?;
    buffer::write_f32(device, &input_buf, input)?;

    let output_buf = buffer::create(device, size_bytes, USAGE_STORAGE | USAGE_COPY_SRC)?;

    let shader = pipeline::create_shader_module(device, wgsl)?;
    // Асинхронная версия — реально сообщает об ошибках WGSL.
    let cp = pipeline::create_compute_pipeline_async(device, &shader, entry).await?;
    // Явные размеры в bind group.
    let bg = pipeline::create_bind_group(
        device, &cp,
        &input_buf, size_bytes,
        &output_buf, size_bytes,
    )?;

    let encoder_js = call_method0(device.as_ref(), "createCommandEncoder")?;
    let pass_js = call_method0(&encoder_js, "beginComputePass")?;

    call_method1(&pass_js, "setPipeline", cp.as_ref())?;
    let args = js_sys::Array::new();
    args.push(&JsValue::from_f64(0.0));
    args.push(bg.as_ref());
    call_method_apply(&pass_js, "setBindGroup", &args)?;

    let n_wg = (count + workgroup_size - 1) / workgroup_size;
    let dargs = js_sys::Array::new();
    dargs.push(&JsValue::from_f64(n_wg as f64));
    dargs.push(&JsValue::from_f64(1.0));
    dargs.push(&JsValue::from_f64(1.0));
    call_method_apply(&pass_js, "dispatchWorkgroups", &dargs)?;
    call_method0(&pass_js, "end")?;

    let cmd = call_method0(&encoder_js, "finish")?;

    let queue_js = get_property(device.as_ref(), "queue")?;
    let submit_args = js_sys::Array::new();
    let cmds = js_sys::Array::new();
    cmds.push(&cmd);
    submit_args.push(&cmds);
    call_method_apply(&queue_js, "submit", &submit_args)?;

    return buffer::read_f32(device, &output_buf, count).await;
}

pub const WGSL_DOUBLE: &str = r#"
@group(0) @binding(0) var<storage, read>       input:  array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < arrayLength(&input)) {
        output[i] = input[i] * 2.0;
    }
}
"#;

pub const WGSL_ADD: &str = r#"
@group(0) @binding(0) var<storage, read>       input:  array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < arrayLength(&input)) {
        output[i] = input[i] + 1.0;
    }
}
"#;