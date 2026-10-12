//! Resource Timing и Navigation Timing.
//!
//! ВАЖНО: `get_entries_by_type` возвращает `js_sys::Array` напрямую,
//! без Result. Ошибок на этом шаге быть не может.

use wasm_bindgen::prelude::*;

use crate::perf_core;

/// Одна запись Resource Timing в удобном виде.
#[derive(Debug, Clone)]
pub struct ResourceInfo {
    pub name: String,
    pub duration: f64,
    pub transfer_size: f64,
    pub initiator_type: String,
}

/// Возвращает все ресурсы, загруженные страницей.
pub fn resources() -> Result<Vec<ResourceInfo>, JsValue> {
    let perf = perf_core::performance()?;
    let entries: js_sys::Array = perf.get_entries_by_type("resource");

    let mut out = Vec::with_capacity(entries.length() as usize);
    for i in 0..entries.length() {
        let e: web_sys::PerformanceResourceTiming = entries.get(i).unchecked_into();
        out.push(ResourceInfo {
            name: e.name(),
            duration: e.duration(),
            transfer_size: e.transfer_size(),
            initiator_type: e.initiator_type(),
        });
    }
    Ok(out)
}

/// Только медленные ресурсы (duration > threshold_ms).
pub fn slow_resources(threshold_ms: f64) -> Result<Vec<ResourceInfo>, JsValue> {
    Ok(resources()?
        .into_iter()
        .filter(|r| r.duration > threshold_ms)
        .collect())
}

/// Фазы Navigation Timing для текущей страницы.
#[derive(Debug, Clone)]
pub struct NavPhases {
    pub redirect: f64,
    pub dns: f64,
    pub tcp_tls: f64,
    pub request: f64,
    pub response: f64,
    pub dom_interactive: f64,
    pub dom_complete: f64,
    pub load_event: f64,
}

impl NavPhases {
    pub fn total(&self) -> f64 {
        self.redirect
            + self.dns
            + self.tcp_tls
            + self.request
            + self.response
            + self.dom_interactive
            + self.dom_complete
            + self.load_event
    }
}

fn phase(end: f64, start: f64) -> f64 {
    (end - start).max(0.0)
}

/// Читает Navigation Timing. Возвращает None, если запись отсутствует.
pub fn navigation() -> Result<Option<NavPhases>, JsValue> {
    let perf = perf_core::performance()?;
    let entries: js_sys::Array = perf.get_entries_by_type("navigation");
    if entries.length() == 0 {
        return Ok(None);
    }
    let n: web_sys::PerformanceNavigationTiming = entries.get(0).unchecked_into();
    Ok(Some(NavPhases {
        redirect: phase(n.redirect_end(), n.redirect_start()),
        dns: phase(n.domain_lookup_end(), n.domain_lookup_start()),
        tcp_tls: phase(n.connect_end(), n.connect_start()),
        request: phase(n.response_start(), n.request_start()),
        response: phase(n.response_end(), n.response_start()),
        dom_interactive: phase(n.dom_interactive(), n.response_end()),
        dom_complete: phase(n.dom_complete(), n.dom_interactive()),
        load_event: phase(n.load_event_start(), n.dom_complete()),
    }))
}