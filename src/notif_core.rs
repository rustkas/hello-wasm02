//! Page Notifications: permission, `Notification`, manager.
//!
//! `Notification` — попап поверх окна браузера. Три уровня:
//!
//!   1. Permission — `default` / `granted` / `denied`. Sticky per origin.
//!      Один раз `denied` — JS не может вызвать диалог снова.
//!
//!   2. `new Notification(title, options)` — попап, привязанный к
//!      вкладке. Умирает вместе с вкладкой. Это страничный уровень.
//!
//!   3. Service Worker `registration.showNotification()` — попап,
//!      который живёт вне вкладки. Разбирается в `notif_sw.rs`.
//!
//! ПРАВИЛО: работаем через `Reflect::get` — `web-sys::Notification`
//! требует фичи `Notification`, которой нет в Cargo.toml. Reflect
//! работает с любой версией.
//!
//! ПРАВИЛО: `Notification.permission` — статическое свойство
//! конструктора, не экземпляра. Читаем через Reflect от ctor.
//!
//! ПРАВИЛО: `requestPermission()` требует user gesture в Firefox
//! (SecurityError) и молча игнорируется в Chrome без жеста.
//! Демо вызывает только из обработчика клика.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::callbacks::EventClosures;

// ============================================================
//                    PERMISSION
// ============================================================

/// Состояние разрешения на показ уведомлений.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// Пользователь ещё не отвечал на запрос.
    Default,
    /// Разрешено.
    Granted,
    /// Запрещено. Sticky per origin — JS не может вызвать диалог снова.
    Denied,
    /// Значение не распознано или API недоступен.
    Unknown,
}

impl Permission {
    pub fn from_str(s: &str) -> Self {
        match s {
            "default" => Permission::Default,
            "granted" => Permission::Granted,
            "denied" => Permission::Denied,
            _ => Permission::Unknown,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::Default => "default",
            Permission::Granted => "granted",
            Permission::Denied => "denied",
            Permission::Unknown => "unknown",
        }
    }
    pub fn short(&self) -> &'static str {
        match self {
            Permission::Default => "не запрошено",
            Permission::Granted => "разрешено",
            Permission::Denied => "запрещено",
            Permission::Unknown => "неизвестно",
        }
    }
    /// Цветовая категория — для UI.
    pub fn tone(&self) -> &'static str {
        match self {
            Permission::Granted => "ok",
            Permission::Denied => "err",
            Permission::Default => "warn",
            Permission::Unknown => "muted",
        }
    }
}

/// Возвращает конструктор `Notification` — или ошибку, если API
/// недоступен (secure context отсутствует, браузер не поддерживает).
fn notification_ctor() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("Notification"))
        .map_err(|e| JsValue::from_str(&format!("Reflect Notification: {e:?}")))?;
    if ctor.is_undefined() || ctor.is_null() {
        return Err(JsValue::from_str(
            "Notification API недоступен (нужен secure context и поддержка браузера)",
        ));
    }
    Ok(ctor)
}

/// true, если `Notification` доступен.
pub fn is_supported() -> bool {
    return notification_ctor().is_ok();
}

/// Читает `Notification.permission` — статическое свойство конструктора.
pub fn permission() -> Permission {
    let ctor = match notification_ctor() {
        Ok(c) => c,
        Err(_) => return Permission::Unknown,
    };
    let p = match js_sys::Reflect::get(&ctor, &JsValue::from_str("permission")) {
        Ok(v) => v,
        Err(_) => return Permission::Unknown,
    };
    let s = p.as_string().unwrap_or_else(|| "unknown".to_string());
    return Permission::from_str(&s);
}

/// Запрашивает разрешение. Возвращает новое состояние.
///
/// **Требует user gesture.** В Firefox без жеста бросает `SecurityError`.
/// В Chrome без жеста промис не резолвится (молчание).
///
/// **Если permission === "denied"** — возвращает `Denied` без запроса.
/// JS не может повторно вызвать диалог. Пользователь должен зайти
/// в настройки сайта вручную.
pub async fn request_permission() -> Result<Permission, JsValue> {
    let ctor = notification_ctor()?;
    let f = js_sys::Reflect::get(&ctor, &JsValue::from_str("requestPermission"))
        .map_err(|e| JsValue::from_str(&format!("Reflect requestPermission: {e:?}")))?;
    let f: js_sys::Function = f
        .dyn_into()
        .map_err(|_| JsValue::from_str("requestPermission не функция"))?;
    let result = f
        .call0(&ctor)
        .map_err(|e| JsValue::from_str(&format!("requestPermission: {e:?}")))?;
    if let Some(p) = result.dyn_ref::<js_sys::Promise>() {
        let awaited = JsFuture::from(p.clone()).await?;
        let s = awaited.as_string().unwrap_or_else(|| "unknown".to_string());
        return Ok(Permission::from_str(&s));
    }
    if let Some(s) = result.as_string() {
        return Ok(Permission::from_str(&s));
    }
    Ok(Permission::Unknown)
}

// ============================================================
//                    NOTIFICATION HANDLE
// ============================================================

/// Дескриптор активного уведомления.
pub struct NotificationHandle {
    raw: JsValue,
    _closures: EventClosures,
}

impl NotificationHandle {
    pub fn raw(&self) -> &JsValue {
        return &self.raw;
    }

    /// Закрывает уведомление. Идемпотентно.
    pub fn close(&self) {
        if let Ok(f) = js_sys::Reflect::get(&self.raw, &JsValue::from_str("close"))
            .and_then(|v| v.dyn_into::<js_sys::Function>())
        {
            let _ = f.call0(&self.raw);
        }
    }
}

// ============================================================
//                    NOTIFY OPTIONS
// ============================================================

#[derive(Clone, Debug, Default)]
pub struct NotifyOptions {
    pub title: String,
    pub body: String,
    /// Идентификатор для группировки. Уведомление с тем же `tag`
    /// заменяет предыдущее, а не добавляется в очередь.
    pub tag: String,
    /// URL иконки. Может быть проигнорирован платформой.
    pub icon: Option<String>,
    /// Не закрывать автоматически — ждать действия пользователя.
    /// Поддерживается только на десктопе.
    pub require_interaction: bool,
}

// ============================================================
//                    NOTIFICATION MANAGER
// ============================================================

/// Менеджер активных page-уведомлений.
#[derive(Clone)]
pub struct NotificationManager {
    active: Rc<RefCell<Vec<NotificationHandle>>>,
}

impl NotificationManager {
    pub fn new() -> Self {
        return Self {
            active: Rc::new(RefCell::new(Vec::new())),
        };
    }

    pub fn count(&self) -> usize {
        return self.active.borrow().len();
    }

    pub fn close_all(&self) {
        let mut list = self.active.borrow_mut();
        for handle in list.iter() {
            handle.close();
        }
        list.clear();
    }

    /// Показывает уведомление. `on_event` вызывается на `"show"`,
    /// `"click"`, `"close"`, `"error"`.
    ///
    /// **Важно:** успешный возврат `Ok(())` означает только то, что
    /// конструктор не бросил исключение. Реальный показ ОС
    /// подтверждается событием `"show"` в `on_event`. Диагностируйте
    /// отсутствие события через таймер (см. `app.rs`).
    pub fn show<F>(
        &self,
        opts: NotifyOptions,
        on_event: F,
    ) -> Result<(), JsValue>
    where
        F: FnMut(&str) + 'static,
    {
        let ctor = notification_ctor()?;

        let js_opts = build_notify_options(&opts);

        let args = js_sys::Array::new();
        args.push(&JsValue::from_str(&opts.title));
        args.push(&js_opts);

        let notification = js_sys::Reflect::construct(
            &ctor.dyn_into::<js_sys::Function>().map_err(|_| {
                JsValue::from_str("Notification не конструктор")
            })?,
            &args,
        )
        .map_err(|e| JsValue::from_str(&format!("new Notification: {e:?}")))?;

        let mut closures = EventClosures::new();
        let on_event = Rc::new(RefCell::new(Box::new(on_event)
            as Box<dyn FnMut(&str)>));

        for (event_name, label) in [
            ("show", "show"),
            ("click", "click"),
            ("close", "close"),
            ("error", "error"),
        ] {
            let cb = on_event.clone();
            let label_s = label.to_string();
            let closure = Closure::<dyn FnMut(web_sys::Event)>::new(
                move |_e: web_sys::Event| {
                    let mut f = cb.borrow_mut();
                    f(&label_s);
                },
            );
            let _ = js_sys::Reflect::set(
                &notification,
                &JsValue::from_str(&format!("on{event_name}")),
                closure.as_ref().unchecked_ref(),
            );
            closures.push(closure);
        }

        self.active.borrow_mut().push(NotificationHandle {
            raw: notification,
            _closures: closures,
        });
        Ok(())
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        return Self::new();
    }
}

// ============================================================
//                    HELPERS
// ============================================================

fn build_notify_options(opts: &NotifyOptions) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("body"),
        &JsValue::from_str(&opts.body),
    );
    if !opts.tag.is_empty() {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("tag"),
            &JsValue::from_str(&opts.tag),
        );
    }
    if let Some(icon) = &opts.icon {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("icon"),
            &JsValue::from_str(icon),
        );
    }
    if opts.require_interaction {
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("requireInteraction"),
            &JsValue::TRUE,
        );
    }
    return obj.into();
}

// ============================================================
//                    ДИАГНОСТИКА
// ============================================================
//
// Зачем: `new Notification()` не бросает исключение, даже если попап
// фактически не отображается (DND, настройки ОС/браузера, headless,
// отсутствие доступа к каналу уведомлений). Этот блок собирает
// снимок окружения, по которому можно понять, где именно теряется
// уведомление.

fn js_get(obj: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(key)).ok()
}

fn js_get_str(obj: &JsValue, key: &str) -> Option<String> {
    js_get(obj, key).and_then(|v| v.as_string())
}

fn js_get_bool(obj: &JsValue, key: &str) -> Option<bool> {
    js_get(obj, key).and_then(|v| v.as_bool())
}

fn yes_no(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

/// `navigator.permissions.query({name: 'notifications'})` → `.state`.
///
/// Отличие от `Notification.permission`: Permissions API умеет
/// возвращать не только `granted`/`denied`, но и `prompt`, и умеет
/// подписываться на изменения через `.onchange`. Если API не
/// поддерживается — вернёт `None`.
pub async fn permission_via_permissions_api() -> Option<String> {
    let global = js_sys::global();
    let nav = js_get(&global, "navigator")?;
    let permissions = js_get(&nav, "permissions")?;
    if permissions.is_undefined() || permissions.is_null() {
        return None;
    }
    let query = js_get(&permissions, "query")?;
    let query: js_sys::Function = query.dyn_into().ok()?;
    let desc = js_sys::Object::new();
    js_sys::Reflect::set(
        &desc,
        &JsValue::from_str("name"),
        &JsValue::from_str("notifications"),
    )
    .ok()?;
    let promise = query.call1(&permissions, &desc).ok()?;
    let promise: js_sys::Promise = promise.dyn_into().ok()?;
    let result = JsFuture::from(promise).await.ok()?;
    js_get_str(&result, "state")
}

/// Собирает снимок окружения. Возвращает плоский список (ключ, значение)
/// — удобно для рендера таблицей.
pub async fn collect_diagnostics() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let global = js_sys::global();

    // --- secure context ---
    out.push((
        "isSecureContext".into(),
        yes_no(js_get_bool(&global, "isSecureContext").unwrap_or(false)),
    ));

    // --- Notification API ---
    out.push(("Notification API".into(), yes_no(is_supported())));
    out.push((
        "Notification.permission (legacy)".into(),
        permission().as_str().into(),
    ));

    // --- Permissions API (более надёжный источник) ---
    let perm_api = permission_via_permissions_api().await;
    out.push((
        "Permissions API (notifications)".into(),
        perm_api.clone().unwrap_or_else(|| "n/a".into()),
    ));

    // --- maxActions: если undefined — вероятно, очень старый движок ---
    let ma = notification_ctor()
        .ok()
        .and_then(|c| js_get(&c, "maxActions").and_then(|v| v.as_f64()));
    out.push((
        "Notification.maxActions".into(),
        ma.map(|n| (n as i64).to_string())
            .unwrap_or_else(|| "n/a".into()),
    ));

    // --- navigator ---
    if let Some(nav) = js_get(&global, "navigator") {
        let ua = js_get_str(&nav, "userAgent").unwrap_or_default();
        out.push(("navigator.userAgent".into(), ua.clone()));
        out.push((
            "navigator.platform".into(),
            js_get_str(&nav, "platform").unwrap_or_default(),
        ));
        out.push((
            "navigator.language".into(),
            js_get_str(&nav, "language").unwrap_or_default(),
        ));
        out.push((
            "navigator.webdriver".into(),
            yes_no(js_get_bool(&nav, "webdriver").unwrap_or(false)),
        ));
        let ua_lower = ua.to_lowercase();
        out.push((
            "UA содержит 'headless'".into(),
            yes_no(ua_lower.contains("headless")),
        ));
        out.push((
            "UA содержит 'mobile' или 'android'".into(),
            yes_no(ua_lower.contains("mobile") || ua_lower.contains("android")),
        ));
    }

    // --- document ---
    if let Some(doc) = js_get(&global, "document") {
        out.push((
            "document.visibilityState".into(),
            js_get_str(&doc, "visibilityState").unwrap_or_default(),
        ));
        let has_focus = js_get(&doc, "hasFocus")
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
            .and_then(|f| f.call0(&doc).ok())
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        out.push(("document.hasFocus()".into(), yes_no(has_focus)));
    }

    // --- Service Worker ---
    out.push((
        "Service Worker API".into(),
        yes_no(crate::notif_sw::is_supported()),
    ));

    match crate::notif_sw::get_registration().await {
        Ok(Some(reg)) => {
            out.push(("SW registration".into(), "present".into()));
            for key in ["active", "installing", "waiting"] {
                if let Some(v) = js_get(&reg, key) {
                    if !v.is_null() && !v.is_undefined() {
                        let state = js_get_str(&v, "state").unwrap_or_default();
                        out.push((format!("SW.{key}.state"), state));
                    }
                }
            }
            match crate::notif_sw::count_via_registration(&reg).await {
                Ok(n) => out.push((
                    "SW getNotifications().length".into(),
                    n.to_string(),
                )),
                Err(e) => out.push((
                    "SW getNotifications()".into(),
                    format!("error: {}", e.as_string().unwrap_or_default()),
                )),
            }
        }
        Ok(None) => out.push((
            "SW registration".into(),
            "none — нажмите «Регистрировать SW»".into(),
        )),
        Err(e) => out.push((
            "SW registration".into(),
            format!("error: {}", e.as_string().unwrap_or_default()),
        )),
    }

    // --- Итоговая подсказка ---
    let mut hints: Vec<&'static str> = Vec::new();
    if !js_get_bool(&global, "isSecureContext").unwrap_or(false) {
        hints.push("Нет secure context — Notification API не работает.");
    }
    if !is_supported() {
        hints.push("Notification API недоступен в этом браузере/режиме.");
    }
    if permission() != Permission::Granted {
        hints.push("Разрешение не granted — попап не покажется.");
    }
    if js_get_bool(&global, "isSecureContext").is_none() {
        hints.push("Нет isSecureContext — вероятно, очень старый движок.");
    }
    if let Some(nav) = js_get(&global, "navigator") {
        if js_get_bool(&nav, "webdriver").unwrap_or(false) {
            hints.push(
                "navigator.webdriver = true (автотест/Selenium). \
                 Некоторые ОС глушат уведомления в этом режиме.",
            );
        }
        let ua = js_get_str(&nav, "userAgent").unwrap_or_default().to_lowercase();
        if ua.contains("headless") {
            hints.push(
                "Headless-браузер — попап может не отображаться ОС.",
            );
        }
    }
    hints.push(
        "Проверьте ОС: режим «Не беспокоить» / Focus / уведомления \
         для браузера / канал звука.",
    );

    out.push(("— подсказки —".into(), String::new()));
    for h in hints {
        out.push(("•".into(), h.to_string()));
    }

    out
}