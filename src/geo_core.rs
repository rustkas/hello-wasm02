//! Геолокация: getCurrentPosition, watchPosition, clearWatch.
//!
//! ПРАВИЛО: callbacks — единственный способ получить результат.
//! getCurrentPosition возвращает void, не Promise.
//!
//! ПРАВИЛО: Closure должны жить, пока идёт запрос. Для одноразового
//! запроса — forget(), для трекинга — храним в GeoHandle.
//!
//! ПРАВИЛО: timeout = 0 означает «бесконечно» — почти всегда ошибка.
//!
//! ПРАВИЛО: web-sys не генерирует `*_with_error_callback_and_options`
//! для Geolocation. Вызываем через Reflect + Function::call3.
//!
//! ПРАВИЛО: в замыкании для Closure нельзя перемещать захваченный
//! FnMut наружу, иначе замыкание станет FnOnce.
//!
//! ТРЕБУЕТСЯ В Cargo.toml (features web-sys):
//!   "Geolocation", "Position", "PositionError",
//!   "PositionOptions", "Coordinates"

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

// ============================================================
//                    ОШИБКИ
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoErrorKind {
    PermissionDenied,
    PositionUnavailable,
    Timeout,
    Unknown,
}

impl GeoErrorKind {
    /// Маппинг кода PositionError в наш enum.
    pub fn from_code(code: u16) -> Self {
        return match code {
            1 => GeoErrorKind::PermissionDenied,
            2 => GeoErrorKind::PositionUnavailable,
            3 => GeoErrorKind::Timeout,
            _ => GeoErrorKind::Unknown,
        };
    }

    /// Может ли ошибка исчезнуть при следующей попытке.
    ///
    /// - `Timeout` — браузер продолжит пытаться получить позицию.
    /// - `PositionUnavailable` — временное отсутствие сигнала.
    /// - `PermissionDenied` — sticky, без действий пользователя не исправить.
    /// - `Unknown` — неизвестно, считаем фатальной.
    pub fn is_recoverable(self) -> bool {
        return matches!(
            self,
            GeoErrorKind::Timeout | GeoErrorKind::PositionUnavailable
        );
    }
}

#[derive(Clone, Debug)]
pub struct GeoError {
    pub kind: GeoErrorKind,
    pub message: String,
}

impl GeoError {
    pub fn from_js(err: &web_sys::PositionError) -> Self {
        return Self {
            kind: GeoErrorKind::from_code(err.code()),
            message: err.message(),
        };
    }

    pub fn user_hint(&self) -> &'static str {
        return match self.kind {
            GeoErrorKind::PermissionDenied => {
                "Доступ к геолокации запрещён. Разрешите его в настройках браузера \
                 (значок замка в адресной строке → Разрешения)."
            }
            GeoErrorKind::PositionUnavailable => {
                "Не удалось определить позицию: нет GPS-сигнала и недоступна \
                 геолокация по сети. Попробуйте отключить высокую точность \
                 или ввести координаты вручную."
            }
            GeoErrorKind::Timeout => {
                "Истекло время ожидания. Увеличьте timeout или отключите \
                 высокую точность."
            }
            GeoErrorKind::Unknown => {
                "Неизвестная ошибка геолокации. Проверьте консоль браузера."
            }
        };
    }
}

// ============================================================
//                    ПОЗИЦИЯ
// ============================================================

#[derive(Clone, Debug)]
pub struct GeoPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_m: f64,
    pub altitude_m: Option<f64>,
    pub heading_deg: Option<f64>,
    pub speed_mps: Option<f64>,
    pub timestamp_ms: f64,
}

impl GeoPosition {
    pub fn from_js(pos: &web_sys::Position) -> Self {
        let c = pos.coords();
        return Self {
            latitude: c.latitude(),
            longitude: c.longitude(),
            accuracy_m: c.accuracy(),
            altitude_m: c.altitude(),
            heading_deg: c.heading(),
            speed_mps: c.speed(),
            timestamp_ms: pos.timestamp(),
        };
    }

    pub fn accuracy_label(&self) -> &'static str {
        return if self.accuracy_m < 20.0 {
            "высокая (GPS)"
        } else if self.accuracy_m < 200.0 {
            "средняя (сеть)"
        } else {
            "низкая (IP)"
        };
    }
}

// ============================================================
//                    ГЕОМЕТРИЯ
// ============================================================

/// Расстояние между двумя точками по большому кругу (haversine), метры.
///
/// Радиус Земли — средний, 6 371 000 м. Для последовательных точек
/// GPS-трекинга (десятки метров друг от друга) ошибка < 0.5%.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos()
            * (dlon / 2.0).sin().powi(2);
    return 2.0 * R * a.sqrt().asin();
}

/// Начальный азимут из точки 1 в точку 2, градусы [0, 360).
///
/// 0° — север, 90° — восток. Используется, когда GPS не отдаёт
/// `heading` (на десктопе почти всегда `null`).
pub fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let x = dlon.sin() * phi2.cos();
    let y = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlon.cos();
    let deg = x.atan2(y).to_degrees();
    return if deg < 0.0 { deg + 360.0 } else { deg };
}

/// EMA-сглаживание: `alpha` — вес нового значения (0..1).
///
/// `alpha = 0.3` — заметное сглаживание, но реакция на движение
/// сохраняется. `alpha = 0.05` — почти статика. `alpha = 1.0` —
/// отключение сглаживания.
pub fn smooth_ema(prev: f64, current: f64, alpha: f64) -> f64 {
    return alpha * current + (1.0 - alpha) * prev;
}

// ============================================================
//                    ФОРМАТ КООРДИНАТ
// ============================================================

/// Формат отображения координат.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoFormat {
    /// Десятичные градусы: `12.871196° N`.
    Decimal,
    /// Градусы-минуты-секунды: `12° 52' 16.3" N`.
    Dms,
}

impl GeoFormat {
    pub fn format_lat(&self, lat: f64) -> String {
        let dir = if lat >= 0.0 { "N" } else { "S" };
        return self.format(lat.abs(), dir);
    }

    pub fn format_lon(&self, lon: f64) -> String {
        let dir = if lon >= 0.0 { "E" } else { "W" };
        return self.format(lon.abs(), dir);
    }

    fn format(&self, value: f64, dir: &str) -> String {
        return match self {
            GeoFormat::Decimal => {
                crate::format_str::format_str(format_args!("{:.6}° {dir}", value))
            }
            GeoFormat::Dms => {
                let deg = value.floor();
                let min_total = (value - deg) * 60.0;
                let min = min_total.floor();
                let sec = (min_total - min) * 60.0;
                crate::format_str::format_str(format_args!(
                    "{:.0}° {:.0}' {:.1}\" {dir}",
                    deg, min, sec,
                ))
            }
        };
    }
}

// ============================================================
//                    ОПЦИИ
// ============================================================

#[derive(Clone, Copy, Debug)]
pub struct GeoOptions {
    pub high_accuracy: bool,
    pub timeout_ms: u32,
    pub max_age_ms: u32,
}

impl Default for GeoOptions {
    fn default() -> Self {
        return Self {
            high_accuracy: false,
            timeout_ms: 12000,
            max_age_ms: 0,
        };
    }
}

impl GeoOptions {
    pub fn to_web_sys(&self) -> Result<web_sys::PositionOptions, JsValue> {
        let o = web_sys::PositionOptions::new();
        o.set_enable_high_accuracy(self.high_accuracy);
        o.set_timeout(self.timeout_ms);
        o.set_maximum_age(self.max_age_ms);
        return Ok(o);
    }
}

// ============================================================
//                    WATCH HANDLE
// ============================================================

pub struct GeoHandle {
    watch_id: i32,
    _success: Closure<dyn FnMut(web_sys::Position)>,
    _error: Closure<dyn FnMut(web_sys::PositionError)>,
}

impl GeoHandle {
    pub fn watch_id(&self) -> i32 {
        return self.watch_id;
    }
}

// ============================================================
//                    PUBLIC API
// ============================================================

pub fn is_supported() -> bool {
    return web_sys::window()
        .and_then(|w| w.navigator().geolocation().ok())
        .is_some();
}

fn call_geo_method(
    geo: &web_sys::Geolocation,
    method_name: &str,
    success: &js_sys::Function,
    error: Option<&js_sys::Function>,
    options: &web_sys::PositionOptions,
) -> Result<JsValue, JsValue> {
    let method: js_sys::Function =
        js_sys::Reflect::get(geo, &JsValue::from_str(method_name))?
            .dyn_into()
            .map_err(|_| JsValue::from_str("Geolocation method is not a function"))?;

    let geo_val: &JsValue = geo.unchecked_ref();
    let success_val: &JsValue = success.unchecked_ref();
    let error_val: JsValue = match error {
        Some(e) => e.unchecked_ref::<JsValue>().clone(),
        None => JsValue::UNDEFINED,
    };
    let options_val: &JsValue = options.unchecked_ref();

    return method.call3(geo_val, success_val, &error_val, options_val);
}

pub fn get_current_position<FS, FE>(
    opts: &GeoOptions,
    mut on_success: FS,
    mut on_error: FE,
) -> Result<(), JsValue>
where
    FS: FnMut(GeoPosition) + 'static,
    FE: FnMut(GeoError) + 'static,
{
    let geo = web_sys::window()
        .ok_or("no window")?
        .navigator()
        .geolocation()?;

    let success = Closure::<dyn FnMut(web_sys::Position)>::new(
        move |pos: web_sys::Position| {
            on_success(GeoPosition::from_js(&pos));
        },
    );
    let error = Closure::<dyn FnMut(web_sys::PositionError)>::new(
        move |err: web_sys::PositionError| {
            on_error(GeoError::from_js(&err));
        },
    );

    let options = opts.to_web_sys()?;
    call_geo_method(
        &geo,
        "getCurrentPosition",
        success.as_ref().unchecked_ref(),
        Some(error.as_ref().unchecked_ref()),
        &options,
    )?;

    success.forget();
    error.forget();
    return Ok(());
}

pub fn watch_position<FS, FE>(
    opts: &GeoOptions,
    mut on_success: FS,
    mut on_error: FE,
) -> Result<GeoHandle, JsValue>
where
    FS: FnMut(GeoPosition) + 'static,
    FE: FnMut(GeoError) + 'static,
{
    let geo = web_sys::window()
        .ok_or("no window")?
        .navigator()
        .geolocation()?;

    let success = Closure::<dyn FnMut(web_sys::Position)>::new(
        move |pos: web_sys::Position| {
            on_success(GeoPosition::from_js(&pos));
        },
    );
    let error = Closure::<dyn FnMut(web_sys::PositionError)>::new(
        move |err: web_sys::PositionError| {
            on_error(GeoError::from_js(&err));
        },
    );

    let options = opts.to_web_sys()?;
    let result = call_geo_method(
        &geo,
        "watchPosition",
        success.as_ref().unchecked_ref(),
        Some(error.as_ref().unchecked_ref()),
        &options,
    )?;

    let watch_id = result
        .as_f64()
        .ok_or_else(|| JsValue::from_str("watchPosition did not return a number"))?
        as i32;

    return Ok(GeoHandle {
        watch_id,
        _success: success,
        _error: error,
    });
}

pub fn clear_watch(watch_id: i32) -> Result<(), JsValue> {
    let geo = web_sys::window()
        .ok_or("no window")?
        .navigator()
        .geolocation()?;
    geo.clear_watch(watch_id);
    return Ok(());
}

/// Читает статус разрешения через Permissions API.
pub async fn permission_state() -> Option<String> {
    use wasm_bindgen_futures::JsFuture;

    let global = js_sys::global();
    let nav = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")).ok()?;
    let permissions = js_sys::Reflect::get(&nav, &JsValue::from_str("permissions")).ok()?;
    if permissions.is_undefined() || permissions.is_null() {
        return None;
    }
    let query: js_sys::Function =
        js_sys::Reflect::get(&permissions, &JsValue::from_str("query"))
            .ok()?
            .dyn_into()
            .ok()?;
    let desc = js_sys::Object::new();
    js_sys::Reflect::set(
        &desc,
        &JsValue::from_str("name"),
        &JsValue::from_str("geolocation"),
    )
    .ok()?;
    let promise = query.call1(&permissions, &desc).ok()?;
    let promise: js_sys::Promise = promise.dyn_into().ok()?;
    let result = JsFuture::from(promise).await.ok()?;
    return js_sys::Reflect::get(&result, &JsValue::from_str("state"))
        .ok()?
        .as_string();
}