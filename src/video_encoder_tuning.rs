//! 23.4. Codec negotiation и тюнинг VideoEncoder.
//!
//! Обёртки для negotiation и tuning:
//!
//! - `isConfigSupported` для списка кодеков;
//! - `configure_tuned` — configure с полным набором подсказок:
//!   `latencyMode`, `bitrateMode`, `contentHint`,
//!   `hardwareAcceleration`, `scalabilityMode`, `alpha`.
//!
//! ## Что нового по сравнению с 23.3
//!
//! 23.3 (`video_encoder`) — базовые обёртки: конструктор, configure,
//! encode, flush, close. Здесь — **tuning**: подсказки кодеку,
//! проверка поддержки, выбор лучшего кодека.
//!
//! ## Переиспользование
//!
//! `configure_tuned` строит `VideoEncoderConfig` через
//! `Object::new` + `set_prop` и вызывает метод `configure` напрямую
//! через `get_fn`. Дополнительных зависимостей от `video_encoder`
//! не требуется — только базовые хелперы из `video_frame`.

use js_sys::{Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::{format_str, get_fn, num, set_prop};

// =============================================================================
// Справочники допустимых значений
// =============================================================================

/// Кодеки для проверки. `avc1.42001E` — H.264 Baseline.
/// `vp8` — VP8. `vp09.00.10.08` — VP9 Profile 0.
/// `av01.0.04M.08` — AV1 Main Profile.
pub const CODECS: &[(&str, &str)] = &[
    ("avc1.42001E", "H.264 Baseline"),
    ("vp8", "VP8"),
    ("vp09.00.10.08", "VP9"),
    ("av01.0.04M.08", "AV1"),
];

pub const LATENCY_MODES: &[&str] = &["quality", "realtime"];
pub const BITRATE_MODES: &[&str] = &["variable", "constant"];
pub const CONTENT_HINTS: &[&str] = &["motion", "detail", "text"];
pub const HW_ACCELS: &[&str] = &["no-preference", "prefer-hardware", "prefer-software"];

/// Значения `scalabilityMode`. Первое — «выключено».
/// Полный список в спецификации WebCodecs больше, здесь — основные.
pub const SCALABILITY_MODES: &[&str] = &[
    "(off)", "L1T1", "L1T2", "L1T3", "L2T1", "L2T2", "L3T1",
];

/// Значения `alpha`. Первое — «выключено».
pub const ALPHA_MODES: &[&str] = &["(off)", "discard", "keep"];

// =============================================================================
// Конфигурация tuning
// =============================================================================

/// Полная конфигурация для `configure` / `isConfigSupported`.
/// Все поля, которые могут отсутствовать в конфиге WebCodecs,
/// — `Option`. Пустые строки из UI (например `"(off)"`)
/// конвертируются в `None` на стороне демо-примера.
pub struct TuningConfig<'a> {
    pub codec: &'a str,
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub framerate: f64,
    pub latency_mode: Option<&'a str>,
    pub bitrate_mode: Option<&'a str>,
    pub content_hint: Option<&'a str>,
    pub hardware_acceleration: Option<&'a str>,
    pub scalability_mode: Option<&'a str>,
    pub alpha: Option<&'a str>,
}

impl<'a> TuningConfig<'a> {
    /// Собирает JS-объект `VideoEncoderConfig`.
    fn to_js_object(&self) -> Result<Object, JsValue> {
        let cfg = Object::new();
        set_prop(&cfg, "codec", &JsValue::from_str(self.codec))?;
        set_prop(&cfg, "width", &num(self.width as f64))?;
        set_prop(&cfg, "height", &num(self.height as f64))?;
        set_prop(&cfg, "bitrate", &num(self.bitrate as f64))?;
        set_prop(&cfg, "framerate", &num(self.framerate))?;
        if let Some(v) = self.latency_mode {
            set_prop(&cfg, "latencyMode", &JsValue::from_str(v))?;
        }
        if let Some(v) = self.bitrate_mode {
            set_prop(&cfg, "bitrateMode", &JsValue::from_str(v))?;
        }
        if let Some(v) = self.content_hint {
            set_prop(&cfg, "contentHint", &JsValue::from_str(v))?;
        }
        if let Some(v) = self.hardware_acceleration {
            set_prop(&cfg, "hardwareAcceleration", &JsValue::from_str(v))?;
        }
        if let Some(v) = self.scalability_mode {
            set_prop(&cfg, "scalabilityMode", &JsValue::from_str(v))?;
        }
        if let Some(v) = self.alpha {
            set_prop(&cfg, "alpha", &JsValue::from_str(v))?;
        }
        Ok(cfg)
    }
}

// =============================================================================
// Расширенный configure
// =============================================================================

/// `encoder.configure(...)` с полным набором полей.
pub fn configure_tuned(encoder: &JsValue, cfg: &TuningConfig) -> Result<(), JsValue> {
    let cfg_obj = cfg.to_js_object()?;
    let f = get_fn(encoder, "configure")?;
    let _ = f.call1(encoder, &cfg_obj)?;
    Ok(())
}

// =============================================================================
// isConfigSupported
// =============================================================================

/// Результат проверки одного кодека.
#[derive(Clone)]
pub struct CodecSupport {
    pub codec: String,
    pub label: String,
    pub supported: bool,
}

/// Проверяет `isConfigSupported` для каждого кодека из `CODECS`.
/// Поле `codec` в переданной конфигурации игнорируется —
/// он подставляется из списка `CODECS` на каждой итерации.
pub async fn check_codecs(cfg: &TuningConfig<'_>) -> Vec<CodecSupport> {
    let mut out = Vec::with_capacity(CODECS.len());
    for (codec, label) in CODECS {
        let supported = check_one_codec(codec, cfg).await.unwrap_or(false);
        out.push(CodecSupport {
            codec: codec.to_string(),
            label: label.to_string(),
            supported,
        });
    }
    out
}

/// Проверяет один кодек с заданной конфигурацией.
/// Поле `codec` из конфигурации игнорируется — используется `codec` из аргумента.
pub async fn check_one_codec(
    codec: &str,
    cfg: &TuningConfig<'_>,
) -> Result<bool, JsValue> {
    let cfg_obj = cfg.to_js_object()?;
    set_prop(&cfg_obj, "codec", &JsValue::from_str(codec))?;

    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("VideoEncoder"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("VideoEncoder not available"));
    }
    let f = Reflect::get(&ctor, &JsValue::from_str("isConfigSupported"))?;
    if !f.is_function() {
        return Err(JsValue::from_str(
            "VideoEncoder.isConfigSupported not available",
        ));
    }
    let f = f.dyn_into::<js_sys::Function>()?;
    let promise = f.call1(&ctor, &cfg_obj)?.dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;
    let supported = Reflect::get(&result, &JsValue::from_str("supported"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(supported)
}

// =============================================================================
// Форматирование результатов
// =============================================================================

/// Форматирует результат `check_codecs` в строку для UI.
pub fn format_codec_support(results: &[CodecSupport]) -> String {
    let mut html = String::with_capacity(results.len() * 64);
    for r in results {
        let mark = if r.supported { "✅" } else { "❌" };
        let cls = if r.supported { "ok" } else { "err" };
        html.push_str(&format_str(format_args!(
            r#"<span class="{cls}">{mark} {label}</span> "#,
            cls = cls,
            mark = mark,
            label = r.label,
        )));
    }
    html
}