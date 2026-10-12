//! 35. MediaDevices: камера и микрофон.
//!
//! Обёртки вокруг enumerateDevices / getUserMedia / getCapabilities.
//!
//! # Ключевое отличие от 23.8
//!
//! Глава 23.8 работает с **кадрами**: MediaStreamTrackProcessor →
//! WASM → MediaStreamTrackGenerator. Это пиксельный уровень.
//!
//! Глава 35 работает с **устройствами**: какие камеры доступны,
//! какие constraints применить, как переключать камеру на лету.
//! Это уровень device management.

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MediaStream, MediaStreamTrack};

use crate::format_str::format_str;

// ============================================================
//              FEATURE DETECTION
// ============================================================

pub fn has_media_devices() -> bool {
    return web_sys::window()
        .map(|w| {
            js_sys::Reflect::get(&w.navigator(), &JsValue::from_str("mediaDevices"))
                .map(|v| !v.is_undefined() && !v.is_null())
                .unwrap_or(false)
        })
        .unwrap_or(false);
}

pub fn has_get_user_media() -> bool {
    if !has_media_devices() {
        return false;
    }
    return web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w.navigator(), &JsValue::from_str("mediaDevices")).ok())
        .and_then(|md| js_sys::Reflect::get(&md, &JsValue::from_str("getUserMedia")).ok())
        .map(|v| v.is_function())
        .unwrap_or(false);
}

pub fn has_enumerate_devices() -> bool {
    if !has_media_devices() {
        return false;
    }
    return web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w.navigator(), &JsValue::from_str("mediaDevices")).ok())
        .and_then(|md| js_sys::Reflect::get(&md, &JsValue::from_str("enumerateDevices")).ok())
        .map(|v| v.is_function())
        .unwrap_or(false);
}

// ============================================================
//              DEVICE INFO
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    VideoInput,
    AudioInput,
    AudioOutput,
    Unknown,
}

impl DeviceKind {
    pub fn from_str(s: &str) -> Self {
        return match s {
            "videoinput" => DeviceKind::VideoInput,
            "audioinput" => DeviceKind::AudioInput,
            "audiooutput" => DeviceKind::AudioOutput,
            _ => DeviceKind::Unknown,
        };
    }
    pub fn as_str(self) -> &'static str {
        return match self {
            DeviceKind::VideoInput => "videoinput",
            DeviceKind::AudioInput => "audioinput",
            DeviceKind::AudioOutput => "audiooutput",
            DeviceKind::Unknown => "unknown",
        };
    }
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub kind: DeviceKind,
    pub device_id: String,
    pub group_id: String,
    pub label: String,
}

impl DeviceInfo {
    pub fn from_js(v: &JsValue) -> Self {
        return Self {
            kind: DeviceKind::from_str(
                &js_sys::Reflect::get(v, &JsValue::from_str("kind"))
                    .ok()
                    .and_then(|k| k.as_string())
                    .unwrap_or_default(),
            ),
            device_id: js_sys::Reflect::get(v, &JsValue::from_str("deviceId"))
                .ok()
                .and_then(|x| x.as_string())
                .unwrap_or_default(),
            group_id: js_sys::Reflect::get(v, &JsValue::from_str("groupId"))
                .ok()
                .and_then(|x| x.as_string())
                .unwrap_or_default(),
            label: js_sys::Reflect::get(v, &JsValue::from_str("label"))
                .ok()
                .and_then(|x| x.as_string())
                .unwrap_or_default(),
        };
    }

    pub fn has_label(&self) -> bool {
        return !self.label.is_empty();
    }

    /// Показывает label, либо «Камера N» для случая без разрешения.
    pub fn display_label(&self, index: usize) -> String {
        if self.has_label() {
            return self.label.clone();
        }
        return match self.kind {
            DeviceKind::VideoInput => format_str(format_args!("Камера {}", index + 1)),
            DeviceKind::AudioInput => format_str(format_args!("Микрофон {}", index + 1)),
            DeviceKind::AudioOutput => format_str(format_args!("Динамик {}", index + 1)),
            DeviceKind::Unknown => format_str(format_args!("Устройство {}", index + 1)),
        };
    }
}

/// Обёртка над `enumerateDevices()`.
pub async fn enumerate_devices() -> Result<Vec<DeviceInfo>, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let md = js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("mediaDevices"))?;
    let f = js_sys::Reflect::get(&md, &JsValue::from_str("enumerateDevices"))?;
    let f: js_sys::Function = f
        .dyn_into()
        .map_err(|_| JsValue::from_str("enumerateDevices не функция"))?;
    let promise = f.call0(&md)?.dyn_into::<Promise>()?;
    let result = JsFuture::from(promise).await?;
    let array: Array = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("enumerateDevices не вернул Array"))?;
    let mut out = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        out.push(DeviceInfo::from_js(&array.get(i)));
    }
    return Ok(out);
}

// ============================================================
//              CONSTRAINTS
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacingMode {
    /// Любое лицо (по умолчанию).
    Any,
    /// Фронтальная камера (селфи).
    User,
    /// Основная камера (тыл).
    Environment,
}

impl FacingMode {
    pub fn as_str(self) -> &'static str {
        return match self {
            FacingMode::Any => "",
            FacingMode::User => "user",
            FacingMode::Environment => "environment",
        };
    }
}

#[derive(Clone, Debug)]
pub struct StreamConstraints {
    /// Запрашивать видео? Если false — параметры ниже игнорируются.
    pub video: bool,
    /// Запрашивать аудио?
    pub audio: bool,
    /// Идеальная ширина, пиксели.
    pub width: u32,
    /// Идеальная высота, пиксели.
    pub height: u32,
    /// Идеальный fps.
    pub frame_rate: u32,
    /// `facingMode` (только для мобильных).
    pub facing: FacingMode,
    /// Конкретный deviceId (exact). Пустая строка — не указывать.
    pub video_device_id: String,
    pub audio_device_id: String,
    /// Эхо-подавление (для аудио).
    pub echo_cancellation: bool,
}

impl Default for StreamConstraints {
    fn default() -> Self {
        return Self {
            video: true,
            audio: false,
            width: 1280,
            height: 720,
            frame_rate: 30,
            facing: FacingMode::Any,
            video_device_id: String::new(),
            audio_device_id: String::new(),
            echo_cancellation: true,
        };
    }
}

/// Собирает `MediaStreamConstraints` как JS-объект.
///
/// Для каждой характеристики используется `{ ideal: value }` —
/// браузер вернёт ближайшее доступное, не выбросит
/// `OverconstrainedError` при невыполнении.
pub fn build_constraints(c: &StreamConstraints) -> Result<JsValue, JsValue> {
    let root = Object::new();

    // --- video ---
    if c.video {
        let video = Object::new();
        Reflect::set(&video, &JsValue::from_str("width"), &ideal(c.width as f64)?)?;
        Reflect::set(
            &video,
            &JsValue::from_str("height"),
            &ideal(c.height as f64)?,
        )?;
        Reflect::set(
            &video,
            &JsValue::from_str("frameRate"),
            &ideal(c.frame_rate as f64)?,
        )?;
        if !c.video_device_id.is_empty() {
            let dev = Object::new();
            Reflect::set(
                &dev,
                &JsValue::from_str("exact"),
                &JsValue::from_str(&c.video_device_id),
            )?;
            Reflect::set(&video, &JsValue::from_str("deviceId"), &dev)?;
        }
        if c.facing != FacingMode::Any {
            let fm = Object::new();
            Reflect::set(
                &fm,
                &JsValue::from_str("ideal"),
                &JsValue::from_str(c.facing.as_str()),
            )?;
            Reflect::set(&video, &JsValue::from_str("facingMode"), &fm)?;
        }
        Reflect::set(&root, &JsValue::from_str("video"), &video)?;
    } else {
        Reflect::set(&root, &JsValue::from_str("video"), &JsValue::FALSE)?;
    }

    // --- audio ---
    if c.audio {
        let audio = Object::new();
        Reflect::set(
            &audio,
            &JsValue::from_str("echoCancellation"),
            &JsValue::from_bool(c.echo_cancellation),
        )?;
        Reflect::set(
            &audio,
            &JsValue::from_str("noiseSuppression"),
            &JsValue::TRUE,
        )?;
        if !c.audio_device_id.is_empty() {
            let dev = Object::new();
            Reflect::set(
                &dev,
                &JsValue::from_str("exact"),
                &JsValue::from_str(&c.audio_device_id),
            )?;
            Reflect::set(&audio, &JsValue::from_str("deviceId"), &dev)?;
        }
        Reflect::set(&root, &JsValue::from_str("audio"), &audio)?;
    } else {
        Reflect::set(&root, &JsValue::from_str("audio"), &JsValue::FALSE)?;
    }

    return Ok(root.into());
}

fn ideal(value: f64) -> Result<JsValue, JsValue> {
    let obj = Object::new();
    Reflect::set(&obj, &JsValue::from_str("ideal"), &JsValue::from_f64(value))?;
    return Ok(obj.into());
}

// ============================================================
//              GET USER MEDIA
// ============================================================

pub async fn request_stream(c: &StreamConstraints) -> Result<MediaStream, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let md = js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("mediaDevices"))?;
    let f = js_sys::Reflect::get(&md, &JsValue::from_str("getUserMedia"))?;
    let f: js_sys::Function = f
        .dyn_into()
        .map_err(|_| JsValue::from_str("getUserMedia не функция"))?;
    let constraints = build_constraints(c)?;
    let promise = f.call1(&md, &constraints)?.dyn_into::<Promise>()?;
    let stream = JsFuture::from(promise).await?;
    return stream.dyn_into::<MediaStream>();
}

// ============================================================
//              TRACK INSPECTION
// ============================================================

#[derive(Clone, Debug, Default)]
pub struct TrackSettings {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub device_id: Option<String>,
    pub facing_mode: Option<String>,
}

impl TrackSettings {
    pub fn from_js(v: &JsValue) -> Self {
        let read_num = |key: &str| -> Option<f64> {
            js_sys::Reflect::get(v, &JsValue::from_str(key))
                .ok()
                .and_then(|x| x.as_f64())
        };
        let read_str = |key: &str| -> Option<String> {
            js_sys::Reflect::get(v, &JsValue::from_str(key))
                .ok()
                .and_then(|x| x.as_string())
        };
        return Self {
            width: read_num("width").map(|n| n as u32),
            height: read_num("height").map(|n| n as u32),
            frame_rate: read_num("frameRate"),
            device_id: read_str("deviceId"),
            facing_mode: read_str("facingMode"),
        };
    }
}

pub fn get_settings(track: &MediaStreamTrack) -> TrackSettings {
    let f = js_sys::Reflect::get(track, &JsValue::from_str("getSettings"))
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
    let Some(f) = f else {
        return TrackSettings::default();
    };
    let Ok(result) = f.call0(track.as_ref()) else {
        return TrackSettings::default();
    };
    return TrackSettings::from_js(&result);
}

/// Простой трек-статус для UI.
#[derive(Clone, Debug)]
pub struct TrackState {
    pub kind: String,
    pub label: String,
    pub enabled: bool,
    pub muted: bool,
    pub ready_state: String,
}

pub fn read_track_state(track: &MediaStreamTrack) -> TrackState {
    let read_bool = |key: &str| -> bool {
        js_sys::Reflect::get(track, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let read_str = |key: &str| -> String {
        js_sys::Reflect::get(track, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    };
    return TrackState {
        kind: read_str("kind"),
        label: read_str("label"),
        enabled: read_bool("enabled"),
        muted: read_bool("muted"),
        ready_state: read_str("readyState"),
    };
}

// ============================================================
//              ERRORS
// ============================================================

pub fn format_media_error(e: &JsValue) -> String {
    let name = js_sys::Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    return match name.as_str() {
        "NotAllowedError" | "PermissionDeniedError" => "Доступ к камере/микрофону отклонён. \
             Разрешите его в настройках браузера (иконка замка \
             в адресной строке) и попробуйте снова."
            .to_string(),
        "NotFoundError" | "DevicesNotFoundError" => {
            "Устройство не найдено: камера или микрофон отсутствуют. \
             Подключите устройство и повторите."
                .to_string()
        }
        "NotReadableError" | "TrackStartError" => {
            "Устройство занято другим приложением (Zoom, Teams, OBS). \
             Закройте приложения, использующие камеру, и попробуйте снова."
                .to_string()
        }
        "OverconstrainedError" | "ConstraintNotSatisfiedError" => {
            let constraint = js_sys::Reflect::get(e, &JsValue::from_str("constraint"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            format_str(format_args!(
                "Запрошенные параметры невыполнимы: {constraint}. \
                 Уменьшите разрешение или fps.",
            ))
        }
        "SecurityError" => "Нарушение политики безопасности. Проверьте, что страница \
             на HTTPS или localhost."
            .to_string(),
        "AbortError" => "Запрос прерван. Попробуйте снова.".to_string(),
        _ => {
            let msg = js_sys::Reflect::get(e, &JsValue::from_str("message"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| String::from("неизвестная ошибка"));
            format_str(format_args!("getUserMedia: {name} — {msg}."))
        }
    };
}

// ============================================================
//              STOP
// ============================================================

/// Освобождает все треки потока. Идемпотентно.
pub fn stop_stream(stream: &MediaStream) {
    let tracks = stream.get_tracks();
    for i in 0..tracks.length() {
        if let Ok(track) = tracks.get(i).dyn_into::<MediaStreamTrack>() {
            track.stop();
        }
    }
}
