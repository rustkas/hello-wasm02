//! WASM-тесты главы 44: WebGPU.
//!
//! Запуск:
//!   wasm-pack test --chrome --headless -- --test wasm_webgpu
//!
//! WebGPU в headless Chrome работает (SwiftShader).
//! Требует `--cfg=web_sys_unstable_apis` — уже в `.cargo/config.toml`.

use wasm_bindgen_test::*;

use hello_wasm02::webgpu::{self, buffer, compute, device};

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// 1. Availability
// ============================================================

#[wasm_bindgen_test]
fn wasm_webgpu_is_available() {
    assert!(
        webgpu::is_available(),
        "WebGPU должен быть доступен в Chrome",
    );
}

#[wasm_bindgen_test]
fn wasm_gpu_object_created() {
    use wasm_bindgen::JsCast;
    let gpu = webgpu::gpu().expect("navigator.gpu доступен");
    // `unchecked_ref` даёт явную ссылку на JsValue —
    // .as_ref() неоднозначен, потому что у Gpu несколько AsRef.
    let v: &wasm_bindgen::JsValue = gpu.unchecked_ref();
    assert!(!v.is_undefined());
}

// ============================================================
// 2. Device initialization
// ============================================================

#[wasm_bindgen_test]
async fn wasm_device_init() {
    use wasm_bindgen::JsCast;
    let ctx = device::init("high-performance").await
        .expect("init должен вернуть GpuContext");
    let adapter_v: &wasm_bindgen::JsValue = ctx.adapter.unchecked_ref();
    let device_v: &wasm_bindgen::JsValue = ctx.device.unchecked_ref();
    assert!(!adapter_v.is_undefined());
    assert!(!device_v.is_undefined());
    assert!(!ctx.info.vendor.is_empty());
}

#[wasm_bindgen_test]
async fn wasm_read_limits() {
    let ctx = device::init("high-performance").await.unwrap();
    let limits = device::read_limits(&ctx.device);
    let max_buf = limits.iter().find(|(k, _)| k == "maxBufferSize")
        .expect("maxBufferSize должен быть в списке");
    assert!(max_buf.1 > 0.0, "maxBufferSize > 0");
}

// ============================================================
// 3. Buffer create / write / read
// ============================================================

#[wasm_bindgen_test]
async fn wasm_buffer_create() {
    let ctx = device::init("high-performance").await.unwrap();
    // STORAGE | COPY_DST
    let usage = 0x0080u32 | 0x0008u32;
    let buf = buffer::create(&ctx.device, 256, usage);
    assert!(buf.is_ok(), "create_buffer должен пройти");
}

#[wasm_bindgen_test]
async fn wasm_buffer_write_and_read() {
    let ctx = device::init("high-performance").await.unwrap();
    // STORAGE | COPY_DST | COPY_SRC
    let usage = 0x0080u32 | 0x0008u32 | 0x0004u32;
    let buf = buffer::create(&ctx.device, 16, usage)
        .expect("create buffer");
    let data: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
    buffer::write_f32(&ctx.device, &buf, &data)
        .expect("write f32");
    let back = buffer::read_f32(&ctx.device, &buf, 4).await
        .expect("read f32");
    assert_eq!(back, vec![1.0, 2.0, 3.0, 4.0]);
}

// ============================================================
// 4. Compute pipeline
// ============================================================

#[wasm_bindgen_test]
async fn wasm_compute_double() {
    let ctx = device::init("high-performance").await.unwrap();
    let input: Vec<f32> = (1..=16).map(|i| i as f32).collect();
    let output = compute::run_f32(
        &ctx.device,
        compute::WGSL_DOUBLE,
        "main",
        &input,
        64,
    ).await.expect("compute должен выполниться");
    let expected: Vec<f32> = input.iter().map(|x| x * 2.0).collect();
    assert_eq!(output, expected);
}

#[wasm_bindgen_test]
async fn wasm_compute_add_one() {
    let ctx = device::init("high-performance").await.unwrap();
    let input: Vec<f32> = (0..32).map(|i| i as f32).collect();
    let output = compute::run_f32(
        &ctx.device,
        compute::WGSL_ADD,
        "main",
        &input,
        64,
    ).await.unwrap();
    let expected: Vec<f32> = input.iter().map(|x| x + 1.0).collect();
    assert_eq!(output, expected);
}

#[wasm_bindgen_test]
async fn wasm_compute_empty_input() {
    let ctx = device::init("high-performance").await.unwrap();
    let input: Vec<f32> = Vec::new();
    let output = compute::run_f32(
        &ctx.device,
        compute::WGSL_DOUBLE,
        "main",
        &input,
        64,
    ).await.unwrap();
    assert!(output.is_empty());
}

#[wasm_bindgen_test]
async fn wasm_compute_large_array() {
    let ctx = device::init("high-performance").await.unwrap();
    let input: Vec<f32> = (0..1024).map(|i| i as f32).collect();
    let output = compute::run_f32(
        &ctx.device,
        compute::WGSL_DOUBLE,
        "main",
        &input,
        64,
    ).await.unwrap();
    assert_eq!(output.len(), 1024);
    assert_eq!(output[0], 0.0);
    assert_eq!(output[1023], 2046.0);
}

// ============================================================
// 5. Разные workgroup_size
// ============================================================

#[wasm_bindgen_test]
async fn wasm_compute_wg32() {
    let ctx = device::init("high-performance").await.unwrap();
    // С wg=32 нужно больше workgroups на тот же N.
    let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
    let output = compute::run_f32(
        &ctx.device,
        compute::WGSL_DOUBLE,
        "main",
        &input,
        32,
    ).await.unwrap();
    assert_eq!(output.len(), 100);
    assert_eq!(output[99], 198.0);
}

// ============================================================
// 6. Проверка правильности округления workgroups
// ============================================================

#[wasm_bindgen_test]
async fn wasm_compute_non_multiple_of_wg() {
    let ctx = device::init("high-performance").await.unwrap();
    // 100 элементов, wg=64 → 2 workgroups (128 invocations).
    // Шейдер отсекает лишние через if (i < arrayLength(&input)).
    let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
    let output = compute::run_f32(
        &ctx.device,
        compute::WGSL_DOUBLE,
        "main",
        &input,
        64,
    ).await.unwrap();
    assert_eq!(output.len(), 100, "не должно быть лишних элементов");
    assert_eq!(output[0], 0.0);
    assert_eq!(output[99], 198.0);
}