//! 45. WebRTC peer connection wrapper.
//!
//! Обёртка над RTCPeerConnection:
//!   • создание peer с конфигурацией ICE-серверов;
//!   • offer/answer через SDP;
//!   • ICE candidates — обработка и обмен;
//!   • data channels — создание, приём, отправка;
//!   • lifecycle — connectionState и iceConnectionState.
//!
//! # Особенности web-sys
//!
//! • `create_offer`, `create_answer`, `set_*_description`,
//!   `add_ice_candidate*` возвращают `Promise` напрямую (без
//!   `Result<Promise>`). Знак `?` к ним не применяется.
//! • `RtcConfiguration`, `RtcSessionDescriptionInit`,
//!   `RtcIceCandidateInit` — dictionary-типы. Создаём их через
//!   `js_sys::Object` + `Reflect::set`, конвертируем через
//!   `unchecked_into`. `dyn_into` для dictionary не работает.
//! • Поле `readyState` читаем через `Reflect`, потому что
//!   фича `RtcDataChannelState` не подключена в Cargo.toml.

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    RtcDataChannel, RtcDataChannelEvent, RtcIceCandidate,
    RtcPeerConnection, RtcPeerConnectionIceEvent,
};

// ============================================================
//                    СОСТОЯНИЯ
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtcState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
    Unknown,
}

impl RtcState {
    pub fn from_str(s: &str) -> Self {
        return match s {
            "new" => RtcState::New,
            "connecting" => RtcState::Connecting,
            "connected" => RtcState::Connected,
            "disconnected" => RtcState::Disconnected,
            "failed" => RtcState::Failed,
            "closed" => RtcState::Closed,
            _ => RtcState::Unknown,
        };
    }
    pub fn as_str(self) -> &'static str {
        return match self {
            RtcState::New => "new",
            RtcState::Connecting => "connecting",
            RtcState::Connected => "connected",
            RtcState::Disconnected => "disconnected",
            RtcState::Failed => "failed",
            RtcState::Closed => "closed",
            RtcState::Unknown => "unknown",
        };
    }
    pub fn is_terminal(self) -> bool {
        return matches!(self, RtcState::Closed | RtcState::Failed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IceState {
    New,
    Checking,
    Connected,
    Completed,
    Disconnected,
    Failed,
    Closed,
    Unknown,
}

impl IceState {
    pub fn from_str(s: &str) -> Self {
        return match s {
            "new" => IceState::New,
            "checking" => IceState::Checking,
            "connected" => IceState::Connected,
            "completed" => IceState::Completed,
            "disconnected" => IceState::Disconnected,
            "failed" => IceState::Failed,
            "closed" => IceState::Closed,
            _ => IceState::Unknown,
        };
    }
    pub fn as_str(self) -> &'static str {
        return match self {
            IceState::New => "new",
            IceState::Checking => "checking",
            IceState::Connected => "connected",
            IceState::Completed => "completed",
            IceState::Disconnected => "disconnected",
            IceState::Failed => "failed",
            IceState::Closed => "closed",
            IceState::Unknown => "unknown",
        };
    }
}

// ============================================================
//                    КОНФИГУРАЦИЯ
// ============================================================

#[derive(Clone, Debug)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PeerConfig {
    pub ice_servers: Vec<IceServer>,
}

impl PeerConfig {
    /// Для loopback (одна вкладка, два peer'а) — без STUN.
    pub fn loopback() -> Self {
        return Self { ice_servers: vec![] };
    }
    /// С публичным Google STUN. Для соединения через интернет.
    pub fn with_stun() -> Self {
        return Self {
            ice_servers: vec![IceServer {
                urls: vec!["stun:stun.l.google.com:19302".into()],
                username: None,
                credential: None,
            }],
        };
    }
}

fn build_configuration(cfg: &PeerConfig) -> Result<JsValue, JsValue> {
    let config = Object::new();
    let servers = Array::new();
    for s in &cfg.ice_servers {
        let server = Object::new();
        let urls = Array::new();
        for u in &s.urls {
            urls.push(&JsValue::from_str(u));
        }
        Reflect::set(&server, &JsValue::from_str("urls"), &urls)?;
        if let Some(u) = &s.username {
            Reflect::set(
                &server,
                &JsValue::from_str("username"),
                &JsValue::from_str(u),
            )?;
        }
        if let Some(c) = &s.credential {
            Reflect::set(
                &server,
                &JsValue::from_str("credential"),
                &JsValue::from_str(c),
            )?;
        }
        servers.push(&server);
    }
    Reflect::set(&config, &JsValue::from_str("iceServers"), &servers)?;
    Reflect::set(
        &config,
        &JsValue::from_str("iceTransportPolicy"),
        &JsValue::from_str("all"),
    )?;
    return Ok(config.into());
}

pub fn create_peer_connection(cfg: &PeerConfig) -> Result<RtcPeerConnection, JsValue> {
    let config_js = build_configuration(cfg)?;
    // RtcConfiguration — dictionary. dyn_into не работает,
    // используем unchecked_into.
    let config: web_sys::RtcConfiguration = config_js.unchecked_into();
    return RtcPeerConnection::new_with_configuration(&config);
}

// ============================================================
//                    SDP: OFFER / ANSWER
// ============================================================

pub fn get_local_description(pc: &RtcPeerConnection) -> Result<JsValue, JsValue> {
    let desc = pc
        .local_description()
        .ok_or_else(|| JsValue::from_str("no local description"))?;
    let desc_js: &JsValue = desc.unchecked_ref();
    let sdp = Reflect::get(desc_js, &JsValue::from_str("sdp"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("no sdp"))?;
    let typ = Reflect::get(desc_js, &JsValue::from_str("type"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("no type"))?;
    let init = Object::new();
    Reflect::set(&init, &JsValue::from_str("type"), &JsValue::from_str(&typ))?;
    Reflect::set(&init, &JsValue::from_str("sdp"), &JsValue::from_str(&sdp))?;
    return Ok(init.into());
}

pub async fn create_offer(pc: &RtcPeerConnection) -> Result<JsValue, JsValue> {
    // create_offer возвращает Promise напрямую.
    let promise = pc.create_offer();
    let result = JsFuture::from(promise).await?;
    return Ok(result);
}

pub async fn create_answer(pc: &RtcPeerConnection) -> Result<JsValue, JsValue> {
    let promise = pc.create_answer();
    let result = JsFuture::from(promise).await?;
    return Ok(result);
}

pub async fn set_local_description(
    pc: &RtcPeerConnection,
    desc: &JsValue,
) -> Result<(), JsValue> {
    // RtcSessionDescriptionInit — dictionary. unchecked_into.
    let init: web_sys::RtcSessionDescriptionInit = desc.clone().unchecked_into();
    // Метод возвращает Promise напрямую.
    let promise = pc.set_local_description(&init);
    JsFuture::from(promise).await?;
    return Ok(());
}

pub async fn set_remote_description(
    pc: &RtcPeerConnection,
    desc: &JsValue,
) -> Result<(), JsValue> {
    let init: web_sys::RtcSessionDescriptionInit = desc.clone().unchecked_into();
    let promise = pc.set_remote_description(&init);
    JsFuture::from(promise).await?;
    return Ok(());
}

// ============================================================
//                    ICE CANDIDATES
// ============================================================

pub fn candidate_to_init(c: &RtcIceCandidate) -> Result<JsValue, JsValue> {
    let init = Object::new();
    let c_js: &JsValue = c.unchecked_ref();
    for key in ["candidate", "sdpMid", "usernameFragment"] {
        if let Ok(v) = Reflect::get(c_js, &JsValue::from_str(key)) {
            Reflect::set(&init, &JsValue::from_str(key), &v)?;
        }
    }
    if let Ok(v) = Reflect::get(c_js, &JsValue::from_str("sdpMLineIndex")) {
        Reflect::set(&init, &JsValue::from_str("sdpMLineIndex"), &v)?;
    }
    return Ok(init.into());
}

/// Форматирует кандидата для лога: `typ/protocol`.
pub fn candidate_summary(c: &RtcIceCandidate) -> String {
    let c_js: &JsValue = c.unchecked_ref();
    let s = Reflect::get(c_js, &JsValue::from_str("candidate"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let typ = extract_token(&s, "typ").unwrap_or_else(|| "?".into());
    let proto = extract_token(&s, "protocol").unwrap_or_else(|| "?".into());
    return format!("{typ}/{proto}");
}

fn extract_token(s: &str, key: &str) -> Option<String> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    for i in 0..parts.len().saturating_sub(1) {
        if parts[i] == key {
            return Some(parts[i + 1].to_string());
        }
    }
    return None;
}

pub fn add_ice_candidate(pc: &RtcPeerConnection, init: &JsValue) -> Result<(), JsValue> {
    let cand_init: web_sys::RtcIceCandidateInit = init.clone().unchecked_into();
    // Метод возвращает Promise напрямую.
    let promise = pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&cand_init));
    // Fire-and-forget.
    wasm_bindgen_futures::spawn_local(async move {
        let _ = JsFuture::from(promise).await;
    });
    return Ok(());
}

// ============================================================
//                    DATA CHANNELS
// ============================================================

pub fn create_data_channel(
    pc: &RtcPeerConnection,
    label: &str,
) -> Result<RtcDataChannel, JsValue> {
    // create_data_channel возвращает RtcDataChannel напрямую.
    return Ok(pc.create_data_channel(label));
}

pub fn data_channel_state(dc: &RtcDataChannel) -> &'static str {
    // RtcDataChannelState не подключён в Cargo.toml —
    // читаем строку через Reflect.
    let s = Reflect::get(
        dc.unchecked_ref::<JsValue>(),
        &JsValue::from_str("readyState"),
    )
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_default();
    return match s.as_str() {
        "connecting" => "connecting",
        "open" => "open",
        "closing" => "closing",
        "closed" => "closed",
        _ => "unknown",
    };
}

pub fn data_channel_send(dc: &RtcDataChannel, text: &str) -> Result<(), JsValue> {
    if data_channel_state(dc) != "open" {
        return Err(JsValue::from_str("data channel not open"));
    }
    return dc.send_with_str(text);
}

pub fn data_channel_label(dc: &RtcDataChannel) -> String {
    return dc.label();
}

// ============================================================
//                    EVENT LISTENERS
// ============================================================

pub fn on_ice_candidate<F>(pc: &RtcPeerConnection, mut cb: F) -> Result<(), JsValue>
where
    F: FnMut(Option<RtcIceCandidate>) + 'static,
{
    let closure = Closure::<dyn FnMut(RtcPeerConnectionIceEvent)>::new(
        move |ev: RtcPeerConnectionIceEvent| {
            cb(ev.candidate());
        },
    );
    pc.set_onicecandidate(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    return Ok(());
}

pub fn on_data_channel<F>(pc: &RtcPeerConnection, mut cb: F) -> Result<(), JsValue>
where
    F: FnMut(RtcDataChannel) + 'static,
{
    let closure = Closure::<dyn FnMut(RtcDataChannelEvent)>::new(
        move |ev: RtcDataChannelEvent| {
            cb(ev.channel());
        },
    );
    pc.set_ondatachannel(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    return Ok(());
}

pub fn on_connection_state_change<F>(
    pc: &RtcPeerConnection,
    mut cb: F,
) -> Result<(), JsValue>
where
    F: FnMut(RtcState) + 'static,
{
    let pc_clone = pc.clone();
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(
        move |_ev: web_sys::Event| {
            let s = Reflect::get(
                pc_clone.unchecked_ref::<JsValue>(),
                &JsValue::from_str("connectionState"),
            )
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "unknown".into());
            cb(RtcState::from_str(&s));
        },
    );
    pc.set_onconnectionstatechange(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    return Ok(());
}

pub fn on_ice_connection_state_change<F>(
    pc: &RtcPeerConnection,
    mut cb: F,
) -> Result<(), JsValue>
where
    F: FnMut(IceState) + 'static,
{
    let pc_clone = pc.clone();
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(
        move |_ev: web_sys::Event| {
            let s = Reflect::get(
                pc_clone.unchecked_ref::<JsValue>(),
                &JsValue::from_str("iceConnectionState"),
            )
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "unknown".into());
            cb(IceState::from_str(&s));
        },
    );
    pc.set_oniceconnectionstatechange(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    return Ok(());
}

pub fn on_data_channel_message<F>(
    dc: &RtcDataChannel,
    mut cb: F,
) -> Result<(), JsValue>
where
    F: FnMut(String) + 'static,
{
    let closure = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
        move |ev: web_sys::MessageEvent| {
            if let Some(text) = ev.data().as_string() {
                cb(text);
            }
        },
    );
    dc.set_onmessage(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    return Ok(());
}

pub fn on_data_channel_open<F>(
    dc: &RtcDataChannel,
    mut cb: F,
) -> Result<(), JsValue>
where
    F: FnMut() + 'static,
{
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(
        move |_ev: web_sys::Event| {
            cb();
        },
    );
    dc.set_onopen(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    return Ok(());
}

// ============================================================
//                    CLOSE
// ============================================================

pub fn close_peer_connection(pc: &RtcPeerConnection) {
    let _ = pc.close();
}

pub fn close_data_channel(dc: &RtcDataChannel) {
    let _ = dc.close();
}

// ============================================================
//                    FEATURE DETECTION
// ============================================================

pub fn is_supported() -> bool {
    let global = js_sys::global();
    return Reflect::get(&global, &JsValue::from_str("RTCPeerConnection"))
        .map(|v| v.is_function())
        .unwrap_or(false);
}

use gloo_timers::future::TimeoutFuture;

/// Ждёт, пока iceGatheringState станет "complete".
///
/// Используется в vanilla-ICE-подходе: не отправляем ICE-кандидатов
/// по одному через signaling, а дожидаемся, пока все они окажутся
/// в SDP. После этого весь SDP передаётся одним куском.
pub async fn wait_ice_gathering_complete(
    pc: &RtcPeerConnection,
) -> Result<(), JsValue> {
    for _ in 0..200 {
        let state = Reflect::get(
            pc.unchecked_ref::<JsValue>(),
            &JsValue::from_str("iceGatheringState"),
        )
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
        if state == "complete" {
            return Ok(());
        }
        TimeoutFuture::new(50).await;
    }
    return Err(JsValue::from_str("ICE gathering timeout (10s)"));
}

/// SDP → строка JSON. Формат: `{"type":"offer","sdp":"..."}`.
pub fn sdp_to_json_string(desc: &JsValue) -> Result<String, JsValue> {
    let js_str = js_sys::JSON::stringify(desc)?;
    return Ok(js_str.as_string().unwrap_or_default());
}

/// Строка JSON → SDP-объект `{type, sdp}`.
pub fn sdp_from_json_string(s: &str) -> Result<JsValue, JsValue> {
    return js_sys::JSON::parse(s);
}

/// Читает `type` из SDP-объекта: "offer" | "answer" | "".
pub fn sdp_type(desc: &JsValue) -> String {
    return Reflect::get(desc, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
}