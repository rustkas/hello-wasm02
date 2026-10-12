//! Инициализация GpuAdapter и GpuDevice.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::{WebGpuError, call_method0, get_property};
use crate::format_str::format_str;

pub struct AdapterInfo {
    pub vendor: String,
    pub architecture: String,
    pub device: String,
    pub description: String,
}

pub struct GpuContext {
    pub adapter: web_sys::GpuAdapter,
    pub device: web_sys::GpuDevice,
    pub info: AdapterInfo,
}

pub async fn init(_preference: &str) -> Result<GpuContext, WebGpuError> {
    let gpu = super::gpu()?;

    // === requestAdapter() — без аргументов в web-sys 0.3.105 ===
    let adapter_promise: js_sys::Promise = call_method0(gpu.as_ref(), "requestAdapter")?
        .dyn_into()
        .map_err(|_| WebGpuError::new("requestAdapter: not a Promise".to_string()))?;
    let adapter_value: JsValue = JsFuture::from(adapter_promise).await
        .map_err(WebGpuError::from)?;
    if adapter_value.is_null() || adapter_value.is_undefined() {
        return Err(WebGpuError::new("Не удалось получить адаптер".to_string()));
    }
    let adapter: web_sys::GpuAdapter = adapter_value.dyn_into()
        .map_err(|_| WebGpuError::new("adapter: unexpected type".to_string()))?;

    let info = read_adapter_info(&adapter);

    // === requestDevice() ===
    let device_promise: js_sys::Promise = call_method0(adapter.as_ref(), "requestDevice")?
        .dyn_into()
        .map_err(|_| WebGpuError::new("requestDevice: not a Promise".to_string()))?;
    let device_value: JsValue = JsFuture::from(device_promise).await
        .map_err(WebGpuError::from)?;
    let device: web_sys::GpuDevice = device_value.dyn_into()
        .map_err(|_| WebGpuError::new("device: unexpected type".to_string()))?;

    return Ok(GpuContext { adapter, device, info });
}

fn read_adapter_info(adapter: &web_sys::GpuAdapter) -> AdapterInfo {
    let info_js = get_property(adapter.as_ref(), "info").unwrap_or(JsValue::UNDEFINED);
    let get_str = |key: &str| -> String {
        get_property(&info_js, key).ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "(unknown)".to_string())
    };
    return AdapterInfo {
        vendor: get_str("vendor"),
        architecture: get_str("architecture"),
        device: get_str("device"),
        description: get_str("description"),
    };
}

pub fn read_limits(device: &web_sys::GpuDevice) -> Vec<(String, f64)> {
    let limits_js = get_property(device.as_ref(), "limits").unwrap_or(JsValue::UNDEFINED);
    let keys = [
        "maxBufferSize", "maxStorageBufferBindingSize",
        "maxComputeWorkgroupSizeX", "maxComputeInvocationsPerWorkgroup",
        "maxComputeWorkgroupsPerDimension", "maxBindGroups",
    ];
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let v = get_property(&limits_js, key).ok().and_then(|x| x.as_f64()).unwrap_or(0.0);
        out.push((key.to_string(), v));
    }
    return out;
}

pub fn format_info(info: &AdapterInfo) -> String {
    let mut out = String::with_capacity(256);
    out.push_str(&format_str(format_args!("vendor:       {}\n", info.vendor)));
    out.push_str(&format_str(format_args!("architecture: {}\n", info.architecture)));
    out.push_str(&format_str(format_args!("device:       {}\n", info.device)));
    out.push_str(&format_str(format_args!("description:  {}", info.description)));
    return out;
}