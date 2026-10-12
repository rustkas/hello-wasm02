//! 23.12-W. Pipeline worker: decode → filter → encode для группы chunks.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{ImageBitmap, OffscreenCanvas, OffscreenCanvasRenderingContext2d};

use crate::video_filters::{apply_filter, FILTER_GRAYSCALE, FILTER_INVERT, FILTER_SEPIA};
use crate::video_frame::{format_str, get_fn, invoke, num, set_prop};

const QUEUE_HIGH_WATER: u32 = 8;
const YIELD_AFTER_MS: f64 = 8.0;
const PROGRESS_INTERVAL_MS: f64 = 120.0;

// =============================================================================
// Shared state
// =============================================================================

struct WorkerSharedState { session: u64 }
impl WorkerSharedState { fn new() -> Self { return Self { session: 0 }; } }

// =============================================================================
// Точка входа
// =============================================================================

#[wasm_bindgen]
pub fn worker_run() -> Result<(), JsValue> {
    let global = js_sys::global();
    web_sys::console::log_1(&JsValue::from_str("[pipeline-worker] registering onmessage"));

    let state = Rc::new(RefCell::new(WorkerSharedState::new()));
    let onmessage: Closure<dyn FnMut(JsValue)> = Closure::new({
        let state = state.clone();
        move |event: JsValue| {
            let data = match Reflect::get(&event, &JsValue::from_str("data")) {
                Ok(v) => v, Err(_) => return,
            };
            if data.is_undefined() || data.is_null() { return; }
            if get_str(&data, "type") == "run" {
                handle_run(data, state.clone());
            }
        }
    });
    Reflect::set(&global, &JsValue::from_str("onmessage"), onmessage.as_ref())?;
    onmessage.forget();
    return Ok(());
}

fn handle_run(data: JsValue, state: Rc<RefCell<WorkerSharedState>>) {
    let session = get_f64(&data, "session") as u64;
    state.borrow_mut().session = session;
    spawn_local(async move {
        let result = run_bucket(data, session, state.clone()).await;
        if let Err(e) = result {
            let s = e.as_string().unwrap_or_else(|| format_str(format_args!("{e:?}")));
            if !s.is_empty() && s != "cancelled" { post_error(&s, session); }
        }
    });
}

// =============================================================================
// Post messages
// =============================================================================

fn post_message(msg: &JsValue) {
    let global = js_sys::global();
    let Ok(post) = Reflect::get(&global, &JsValue::from_str("postMessage")) else { return; };
    let Ok(post) = post.dyn_into::<Function>() else { return; };
    let _ = post.call1(&global, msg);
}

#[allow(clippy::too_many_arguments)]
fn post_progress(
    session: u64, frames: u64, total: u64, fps: f64, ms_per_frame: f64,
    filter_ms_avg: f64, encode_ms_avg: f64, gpu_path: bool,
) {
    let msg = Object::new();
    let _ = set_prop(&msg, "type", &JsValue::from_str("progress"));
    let _ = set_prop(&msg, "session", &JsValue::from_f64(session as f64));
    let _ = set_prop(&msg, "frames", &JsValue::from_f64(frames as f64));
    let _ = set_prop(&msg, "total", &JsValue::from_f64(total as f64));
    let _ = set_prop(&msg, "fps", &JsValue::from_f64(fps));
    let _ = set_prop(&msg, "ms_per_frame", &JsValue::from_f64(ms_per_frame));
    let _ = set_prop(&msg, "filter_ms_avg", &JsValue::from_f64(filter_ms_avg));
    let _ = set_prop(&msg, "encode_ms_avg", &JsValue::from_f64(encode_ms_avg));
    let _ = set_prop(&msg, "gpu_path", &JsValue::from_bool(gpu_path));
    post_message(&msg);
}

fn post_error(message: &str, session: u64) {
    let msg = Object::new();
    let _ = set_prop(&msg, "type", &JsValue::from_str("error"));
    let _ = set_prop(&msg, "session", &JsValue::from_f64(session as f64));
    let _ = set_prop(&msg, "message", &JsValue::from_str(message));
    post_message(&msg);
}

/// **FIX**: теперь передаём `avcc` (SPS/PPS) и `codec` от нашего encoder'а.
/// Main thread использует их для mux — иначе плеер видит чужие SPS/PPS
/// и декодирует неправильно → видео дёргается.
fn post_chunks(session: u64, chunks: &[EncodedOut], avcc: &[u8], codec: &str) {
    let arr = Array::new();
    let transfer = Array::new();

    for c in chunks {
        let u8 = Uint8Array::from(c.data.as_slice());
        let buffer = u8.buffer();
        let obj = Object::new();
        let _ = set_prop(&obj, "is_key", &JsValue::from_bool(c.is_key));
        let _ = set_prop(&obj, "ts", &JsValue::from_f64(c.timestamp_us as f64));
        let _ = set_prop(&obj, "dur", &JsValue::from_f64(c.duration_us as f64));
        let _ = set_prop(&obj, "data", buffer.as_ref());
        arr.push(&obj);
        transfer.push(buffer.as_ref());
    }

    let avcc_u8 = Uint8Array::from(avcc);
    let avcc_buffer = avcc_u8.buffer();

    let msg = Object::new();
    let _ = set_prop(&msg, "type", &JsValue::from_str("chunks"));
    let _ = set_prop(&msg, "session", &JsValue::from_f64(session as f64));
    let _ = set_prop(&msg, "chunks", &arr);
    let _ = set_prop(&msg, "avcc", avcc_buffer.as_ref());
    let _ = set_prop(&msg, "codec", &JsValue::from_str(codec));
    transfer.push(avcc_buffer.as_ref());

    let global = js_sys::global();
    let Ok(post) = Reflect::get(&global, &JsValue::from_str("postMessage")) else { return; };
    let Ok(post) = post.dyn_into::<Function>() else { return; };
    let _ = post.call2(&global, &msg, &transfer);
}

// =============================================================================
// Хелперы
// =============================================================================

fn get_str(target: &JsValue, key: &str) -> String {
    return Reflect::get(target, &JsValue::from_str(key))
        .ok().and_then(|v| v.as_string()).unwrap_or_default();
}
fn get_f64(target: &JsValue, key: &str) -> f64 {
    return Reflect::get(target, &JsValue::from_str(key))
        .ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
}
fn vf_timestamp(frame: &JsValue) -> f64 { return get_f64(frame, "timestamp"); }
fn vf_duration(frame: &JsValue) -> f64 { return get_f64(frame, "duration"); }
fn read_u32(obj: &JsValue, name: &str) -> u32 { return get_f64(obj, name) as u32; }

fn close_video_frame(frame: &JsValue) {
    if frame.is_undefined() || frame.is_null() { return; }
    if let Ok(f) = get_fn(frame, "close") { let _ = f.call0(frame); }
}
fn close_all_frames(queue: &Rc<RefCell<VecDeque<JsValue>>>) {
    let mut q = queue.borrow_mut();
    while let Some(f) = q.pop_front() { close_video_frame(&f); }
}
fn try_call(obj: &JsValue, method: &str) {
    if let Ok(f) = get_fn(obj, method) { let _ = f.call0(obj); }
}

async fn wait_for_queue(
    codec_obj: &JsValue, queue_prop: &str,
    state: &Rc<RefCell<WorkerSharedState>>, session: u64,
) -> Result<(), JsValue> {
    loop {
        if state.borrow().session != session { return Err(JsValue::from_str("cancelled")); }
        if read_u32(codec_obj, queue_prop) < QUEUE_HIGH_WATER { return Ok(()); }
        TimeoutFuture::new(0).await;
    }
}

// =============================================================================
// Chunks
// =============================================================================

#[derive(Clone)]
struct EncodedIn {
    is_key: bool,
    timestamp_us: u64,
    duration_us: u32,
    data: Vec<u8>,
}

#[derive(Clone)]
struct EncodedOut {
    is_key: bool,
    timestamp_us: u64,
    duration_us: u32,
    data: Vec<u8>,
}

// =============================================================================
// WebCodecs обёртки
// =============================================================================

fn make_encoded_chunk(c: &EncodedIn) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("EncodedVideoChunk"))?
        .dyn_into::<Function>()?;
    let init = Object::new();
    set_prop(&init, "type", &JsValue::from_str(if c.is_key { "key" } else { "delta" }))?;
    set_prop(&init, "timestamp", &num(c.timestamp_us as f64))?;
    set_prop(&init, "duration", &num(c.duration_us as f64))?;
    let u8 = Uint8Array::from(c.data.as_slice());
    let u8_js: JsValue = u8.into();
    set_prop(&init, "data", &u8_js)?;
    let args = Array::new();
    args.push(&init);
    return Reflect::construct(&ctor, &args);
}

fn make_video_frame_from_bitmap(
    bitmap: &ImageBitmap, timestamp_us: f64, duration_us: f64,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("VideoFrame"))?
        .dyn_into::<Function>()?;
    let init = Object::new();
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    if duration_us > 0.0 { set_prop(&init, "duration", &num(duration_us))?; }
    let args = Array::new();
    args.push(bitmap.as_ref());
    args.push(&init);
    return Reflect::construct(&ctor, &args);
}

fn make_video_frame_from_rgba(
    rgba: &[u8], width: u32, height: u32, timestamp_us: f64, duration_us: f64,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor = Reflect::get(&global, &JsValue::from_str("VideoFrame"))?
        .dyn_into::<Function>()?;
    let u8 = Uint8Array::from(rgba);
    let u8_js: JsValue = u8.into();
    let init = Object::new();
    set_prop(&init, "format", &JsValue::from_str("RGBA"))?;
    set_prop(&init, "codedWidth", &num(width as f64))?;
    set_prop(&init, "codedHeight", &num(height as f64))?;
    set_prop(&init, "timestamp", &num(timestamp_us))?;
    if duration_us > 0.0 { set_prop(&init, "duration", &num(duration_us))?; }
    let args = Array::new();
    args.push(&u8_js);
    args.push(&init);
    return Reflect::construct(&ctor, &args);
}

fn make_decoder_config(codec: &str, description: &[u8]) -> Result<JsValue, JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    if !description.is_empty() {
        let u8 = Uint8Array::from(description);
        let u8_js: JsValue = u8.into();
        set_prop(&cfg, "description", &u8_js)?;
    }
    return Ok(cfg.into());
}

fn make_encoder_config(
    codec: &str, width: u32, height: u32, bitrate: u32,
    framerate: u32, latency_mode: &str,
) -> Result<JsValue, JsValue> {
    let cfg = Object::new();
    set_prop(&cfg, "codec", &JsValue::from_str(codec))?;
    set_prop(&cfg, "width", &num(width as f64))?;
    set_prop(&cfg, "height", &num(height as f64))?;
    set_prop(&cfg, "bitrate", &num(bitrate as f64))?;
    set_prop(&cfg, "framerate", &num(framerate as f64))?;
    if !latency_mode.is_empty() {
        set_prop(&cfg, "latencyMode", &JsValue::from_str(latency_mode))?;
    }
    let avc = Object::new();
    set_prop(&avc, "format", &JsValue::from_str("avc"))?;
    set_prop(&cfg, "avc", &avc)?;
    return Ok(cfg.into());
}

// =============================================================================
// Filter paths
// =============================================================================

async fn process_frame_gpu(
    frame: &JsValue, canvas: &OffscreenCanvas,
    ctx: &OffscreenCanvasRenderingContext2d,
    css_filter: &str, width: u32, height: u32,
) -> Result<JsValue, JsValue> {
    let ts = vf_timestamp(frame);
    let dur = vf_duration(frame);
    set_prop(ctx.as_ref(), "filter", &JsValue::from_str(css_filter))?;
    invoke(ctx.as_ref(), "drawImage", &[
        frame.clone(), num(0.0), num(0.0),
        num(width as f64), num(height as f64),
    ])?;
    let bitmap = canvas.transfer_to_image_bitmap()?;
    let new_frame = make_video_frame_from_bitmap(&bitmap, ts, dur)?;
    let _ = bitmap.close();
    return Ok(new_frame);
}

async fn process_frame_cpu(
    frame: &JsValue, width: u32, height: u32, filter_id: u32,
) -> Result<JsValue, JsValue> {
    let ts = vf_timestamp(frame);
    let dur = vf_duration(frame);
    let bytes_len = (width as usize) * (height as usize) * 4;
    let u8 = Uint8Array::new_with_length(bytes_len as u32);
    let opts = Object::new();
    set_prop(&opts, "format", &JsValue::from_str("RGBA"))?;
    let copy_fn = get_fn(frame, "copyTo")?;
    let copy_ret = copy_fn.call2(frame, u8.as_ref(), opts.as_ref())?;
    if let Ok(promise) = copy_ret.clone().dyn_into::<Promise>() {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
    let rgba = u8.to_vec();
    let filtered = apply_filter(&rgba, width, height, filter_id);
    let new_frame = make_video_frame_from_rgba(&filtered, width, height, ts, dur)?;
    return Ok(new_frame);
}

// =============================================================================
// Bucket processing
// =============================================================================

async fn run_bucket(
    data: JsValue, session: u64, state: Rc<RefCell<WorkerSharedState>>,
) -> Result<(), JsValue> {
    let codec_string = get_str(&data, "codec_string");
    let width = get_f64(&data, "width") as u32;
    let height = get_f64(&data, "height") as u32;
    let latency_mode = get_str(&data, "latency_mode");
    let filter_id = get_f64(&data, "filter_id") as u32;
    // **NEW**: bitrate передаётся из main thread (вычислен из исходного).
    let bitrate = get_f64(&data, "bitrate") as u32;
    let bitrate = if bitrate < 100_000 { 2_000_000 } else { bitrate };

    let avcc_js = Reflect::get(&data, &JsValue::from_str("avcc"))?;
    let source_avcc: Vec<u8> = if let Ok(ab) = avcc_js.dyn_into::<js_sys::ArrayBuffer>() {
        Uint8Array::new(&ab).to_vec()
    } else { Vec::new() };

    let chunks_js = Reflect::get(&data, &JsValue::from_str("chunks"))?;
    let chunks_arr: Array = chunks_js.dyn_into()?;
    let n = chunks_arr.length();

    let mut input_chunks: Vec<EncodedIn> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let obj = chunks_arr.get(i);
        let is_key = Reflect::get(&obj, &JsValue::from_str("is_key"))
            .ok().and_then(|v| v.as_bool()).unwrap_or(false);
        let ts = get_f64(&obj, "ts") as u64;
        let dur = get_f64(&obj, "dur") as u32;
        let data_js = Reflect::get(&obj, &JsValue::from_str("data"))?;
        let bytes: Vec<u8> = if let Ok(ab) = data_js.dyn_into::<js_sys::ArrayBuffer>() {
            Uint8Array::new(&ab).to_vec()
        } else { Vec::new() };
        input_chunks.push(EncodedIn {
            is_key, timestamp_us: ts, duration_us: dur, data: bytes,
        });
    }

    let total = input_chunks.len() as u64;
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[pipeline-worker] bucket: {} chunks, {}x{}, codec={}, latency={}, bitrate={}",
        total, width, height, codec_string, latency_mode, bitrate,
    )));

    if input_chunks.is_empty() {
        post_chunks(session, &[], &source_avcc, &codec_string);
        return Ok(());
    }

    // Сохраняем duration и is_key из входных chunks.
    let mut duration_map: HashMap<u64, u32> = HashMap::with_capacity(input_chunks.len());
    for c in &input_chunks { duration_map.insert(c.timestamp_us, c.duration_us); }
    let duration_map = Rc::new(duration_map);
    let key_flags: Rc<Vec<bool>> = Rc::new(input_chunks.iter().map(|c| c.is_key).collect());

    // OffscreenCanvas.
    let canvas = OffscreenCanvas::new(width, height)?;
    let ctx_obj = canvas.get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d on OffscreenCanvas"))?;
    let ctx: OffscreenCanvasRenderingContext2d = ctx_obj.dyn_into()?;

    let gpu_path = {
        let _ = set_prop(ctx.as_ref(), "filter", &JsValue::from_str("grayscale(100%)"));
        let back = Reflect::get(ctx.as_ref(), &JsValue::from_str("filter"))
            .ok().and_then(|v| v.as_string()).unwrap_or_default();
        back.contains("grayscale")
    };

    let css_filter = match filter_id {
        FILTER_GRAYSCALE => "grayscale(100%)",
        FILTER_INVERT => "invert(100%)",
        FILTER_SEPIA => "sepia(100%)",
        _ => "none",
    };

    // Decoder.
    let pending: Rc<RefCell<VecDeque<JsValue>>> = Rc::new(RefCell::new(VecDeque::new()));
    let decode_err: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let pending_cb = pending.clone();
    let dec_output_cb: Closure<dyn FnMut(JsValue)> = Closure::new(move |frame: JsValue| {
        pending_cb.borrow_mut().push_back(frame);
    });
    let decode_err_cb = decode_err.clone();
    let dec_err_cb: Closure<dyn FnMut(JsValue)> = Closure::new(move |e: JsValue| {
        let msg = Reflect::get(&e, &JsValue::from_str("message"))
            .ok().and_then(|v| v.as_string())
            .unwrap_or_else(|| String::from("unknown decoder error"));
        *decode_err_cb.borrow_mut() = Some(msg);
    });

    let global = js_sys::global();
    let dec_ctor = Reflect::get(&global, &JsValue::from_str("VideoDecoder"))?;
    if !dec_ctor.is_function() { return Err(JsValue::from_str("VideoDecoder недоступен")); }
    let dec_ctor = dec_ctor.dyn_into::<Function>()?;
    let dec_init = Object::new();
    set_prop(&dec_init, "output", dec_output_cb.as_ref())?;
    set_prop(&dec_init, "error", dec_err_cb.as_ref())?;
    let dec_args = Array::new();
    dec_args.push(&dec_init);
    let decoder = Reflect::construct(&dec_ctor, &dec_args)?;

    let dec_cfg = make_decoder_config(&codec_string, &source_avcc)?;
    let dec_configure = get_fn(&decoder, "configure")?;
    dec_configure.call1(&decoder, &dec_cfg)?;
    let dec_decode = get_fn(&decoder, "decode")?;
    let dec_flush = get_fn(&decoder, "flush")?;

    // Encoder.
    let enc_output: Rc<RefCell<Vec<EncodedOut>>> = Rc::new(RefCell::new(Vec::new()));
    let enc_err: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    // **FIX**: захватываем avcc и codec от нашего encoder'а.
    let enc_avcc: Rc<RefCell<Option<Vec<u8>>>> = Rc::new(RefCell::new(None));
    let enc_codec: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let enc_output_cb = enc_output.clone();
    let duration_map_cb = duration_map.clone();
    let enc_avcc_cb = enc_avcc.clone();
    let enc_codec_cb = enc_codec.clone();
    let output_cb: Closure<dyn FnMut(JsValue, JsValue)> = Closure::new(move |chunk: JsValue, meta: JsValue| {
        // Захватываем SPS/PPS и codec только один раз.
        if enc_avcc_cb.borrow().is_none() {
            if let Ok(cfg) = Reflect::get(&meta, &JsValue::from_str("decoderConfig")) {
                if !cfg.is_undefined() && !cfg.is_null() {
                    if let Ok(codec_val) = Reflect::get(&cfg, &JsValue::from_str("codec")) {
                        if let Some(s) = codec_val.as_string() {
                            *enc_codec_cb.borrow_mut() = s;
                        }
                    }
                    if let Ok(desc) = Reflect::get(&cfg, &JsValue::from_str("description")) {
                        if !desc.is_undefined() && !desc.is_null() {
                            let bytes: Vec<u8> = if let Ok(u8v) = desc.clone().dyn_into::<Uint8Array>() {
                                u8v.to_vec()
                            } else if let Ok(ab) = desc.clone().dyn_into::<js_sys::ArrayBuffer>() {
                                Uint8Array::new(&ab).to_vec()
                            } else { Vec::new() };
                            if !bytes.is_empty() { *enc_avcc_cb.borrow_mut() = Some(bytes); }
                        }
                    }
                }
            }
        }

        let is_key = Reflect::get(&chunk, &JsValue::from_str("type"))
            .ok().and_then(|v| v.as_string()).map(|s| s == "key").unwrap_or(false);
        let ts = get_f64(&chunk, "timestamp") as u64;

        let dur = duration_map_cb.get(&ts).copied()
            .filter(|&d| d > 0)
            .or_else(|| {
                let d = get_f64(&chunk, "duration") as u32;
                if d > 0 { Some(d) } else { None }
            })
            .unwrap_or(33_333);

        let bl = get_f64(&chunk, "byteLength") as usize;
        let mut data: Vec<u8> = Vec::new();
        if let Ok(copy_to) = Reflect::get(&chunk, &JsValue::from_str("copyTo")) {
            if let Ok(copy_fn) = copy_to.dyn_into::<Function>() {
                let u8_buf = Uint8Array::new_with_length(bl as u32);
                if copy_fn.call1(&chunk, u8_buf.as_ref()).is_ok() {
                    data = u8_buf.to_vec();
                }
            }
        }
        enc_output_cb.borrow_mut().push(EncodedOut {
            is_key, timestamp_us: ts, duration_us: dur, data,
        });
    });
    let enc_err_cb_out = enc_err.clone();
    let err_cb: Closure<dyn FnMut(JsValue)> = Closure::new(move |e: JsValue| {
        let msg = Reflect::get(&e, &JsValue::from_str("message"))
            .ok().and_then(|v| v.as_string())
            .unwrap_or_else(|| String::from("unknown encoder error"));
        *enc_err_cb_out.borrow_mut() = Some(msg);
    });

    let enc_ctor = Reflect::get(&global, &JsValue::from_str("VideoEncoder"))?;
    if !enc_ctor.is_function() {
        try_call(&decoder, "close");
        return Err(JsValue::from_str("VideoEncoder недоступен"));
    }
    let enc_ctor = enc_ctor.dyn_into::<Function>()?;
    let enc_init = Object::new();
    set_prop(&enc_init, "output", output_cb.as_ref())?;
    set_prop(&enc_init, "error", err_cb.as_ref())?;
    let enc_args = Array::new();
    enc_args.push(&enc_init);
    let encoder = Reflect::construct(&enc_ctor, &enc_args)?;

    let framerate = 30u32;

    // **FIX дёрганья**: принудительно Baseline Profile для encoder'а.
    // High/Main могут использовать B-frames, для которых нужен ctts-box
    // в MP4 (PTS ≠ DTS). У нашего muxer'а ctts нет, поэтому плеер видит
    // не-монотонный PTS → рывки. Baseline не использует B-frames —
    // PTS == DTS, ctts не нужен.
    const ENCODER_CODEC: &str = "avc1.42E01E"; // Baseline 3.1
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[pipeline-worker] encoder forced to {} (Baseline, no B-frames)",
        ENCODER_CODEC,
    )));

    let enc_cfg = make_encoder_config(
        ENCODER_CODEC, width, height, bitrate, framerate, &latency_mode,
    )?;
    let enc_configure = get_fn(&encoder, "configure")?;
    enc_configure.call1(&encoder, &enc_cfg)?;
    let enc_encode = get_fn(&encoder, "encode")?;
    let enc_flush = get_fn(&encoder, "flush")?;

    // Feed + drain.
    let t_start = js_sys::Date::now();
    let mut frames_processed: u64 = 0;
    let mut filter_ms_accum: f64 = 0.0;
    let mut encode_ms_accum: f64 = 0.0;
    let mut last_progress_ms = t_start;
    let mut last_yield_ms = t_start;

    macro_rules! drain_one {
        ($frame:expr) => {{
            let frame: JsValue = $frame;
            let t_filter_start = js_sys::Date::now();
            let new_frame = if gpu_path {
                match process_frame_gpu(frame.as_ref(), &canvas, &ctx, css_filter, width, height).await {
                    Ok(f) => f,
                    Err(_) => {
                        close_video_frame(&frame);
                        process_frame_cpu(frame.as_ref(), width, height, filter_id).await?
                    }
                }
            } else {
                process_frame_cpu(frame.as_ref(), width, height, filter_id).await?
            };
            close_video_frame(&frame);
            let t_filter_end = js_sys::Date::now();

            if wait_for_queue(&encoder, "encodeQueueSize", &state, session).await.is_err() {
                close_video_frame(&new_frame);
                return Err(JsValue::from_str("cancelled"));
            }

            let t_encode_start = js_sys::Date::now();
            let is_key = key_flags
                .get(frames_processed as usize)
                .copied()
                .unwrap_or(frames_processed == 0);
            let opts = Object::new();
            set_prop(&opts, "keyFrame", &JsValue::from_bool(is_key))?;
            enc_encode.call2(&encoder, &new_frame, &opts)?;
            close_video_frame(&new_frame);
            let t_encode_end = js_sys::Date::now();

            filter_ms_accum += t_filter_end - t_filter_start;
            encode_ms_accum += t_encode_end - t_encode_start;
            frames_processed += 1;

            let now = js_sys::Date::now();
            if now - last_progress_ms >= PROGRESS_INTERVAL_MS {
                last_progress_ms = now;
                let elapsed_ms = now - t_start;
                let fps = if elapsed_ms > 0.0 { frames_processed as f64 * 1000.0 / elapsed_ms } else { 0.0 };
                let ms_per_frame = if frames_processed > 0 { elapsed_ms / frames_processed as f64 } else { 0.0 };
                let nf = frames_processed as f64;
                post_progress(session, frames_processed, total, fps, ms_per_frame,
                    if nf > 0.0 { filter_ms_accum / nf } else { 0.0 },
                    if nf > 0.0 { encode_ms_accum / nf } else { 0.0 },
                    gpu_path);
            }

            if now - last_yield_ms >= YIELD_AFTER_MS {
                last_yield_ms = now;
                TimeoutFuture::new(0).await;
            }
        }};
    }

    for input in input_chunks.iter() {
        if state.borrow().session != session {
            close_all_frames(&pending);
            try_call(&decoder, "close");
            try_call(&encoder, "close");
            return Ok(());
        }
        if let Some(msg) = decode_err.borrow().as_ref() {
            close_all_frames(&pending);
            try_call(&decoder, "close");
            try_call(&encoder, "close");
            return Err(JsValue::from_str(&format_str(format_args!("decode: {msg}"))));
        }
        if wait_for_queue(&decoder, "decodeQueueSize", &state, session).await.is_err() {
            close_all_frames(&pending);
            try_call(&decoder, "close");
            try_call(&encoder, "close");
            return Ok(());
        }
        let chunk_js = make_encoded_chunk(input)?;
        if let Err(e) = dec_decode.call1(&decoder, &chunk_js) {
            close_all_frames(&pending);
            try_call(&decoder, "close");
            try_call(&encoder, "close");
            return Err(JsValue::from_str(&format_str(format_args!("decode: {e:?}"))));
        }
        loop {
            let frame = match pending.borrow_mut().pop_front() {
                Some(f) => f, None => break,
            };
            if state.borrow().session != session {
                close_video_frame(&frame);
                close_all_frames(&pending);
                try_call(&decoder, "close");
                try_call(&encoder, "close");
                return Ok(());
            }
            drain_one!(frame);
        }
    }

    let flush_p = dec_flush.call0(&decoder)?.dyn_into::<Promise>()?;
    let _ = wasm_bindgen_futures::JsFuture::from(flush_p).await;

    if let Some(msg) = decode_err.borrow().as_ref() {
        close_all_frames(&pending);
        try_call(&decoder, "close");
        try_call(&encoder, "close");
        return Err(JsValue::from_str(&format_str(format_args!("decode: {msg}"))));
    }

    loop {
        let frame = match pending.borrow_mut().pop_front() {
            Some(f) => f, None => break,
        };
        if state.borrow().session != session {
            close_video_frame(&frame);
            close_all_frames(&pending);
            try_call(&decoder, "close");
            try_call(&encoder, "close");
            return Ok(());
        }
        drain_one!(frame);
    }

    let enc_flush_p = enc_flush.call0(&encoder)?.dyn_into::<Promise>()?;
    wasm_bindgen_futures::JsFuture::from(enc_flush_p).await?;

    try_call(&decoder, "close");
    try_call(&encoder, "close");

    if let Some(msg) = enc_err.borrow().as_ref() {
        return Err(JsValue::from_str(&format_str(format_args!("encode: {msg}"))));
    }

    let out: Vec<EncodedOut> = enc_output.borrow().clone();
    let final_avcc = enc_avcc.borrow().clone().unwrap_or(source_avcc);
    let final_codec = {
        let c = enc_codec.borrow().clone();
        if c.is_empty() { codec_string.clone() } else { c }
    };

    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[pipeline-worker] bucket done: {} chunks, avcc={} B, codec={}, time={} ms",
        out.len(), final_avcc.len(), final_codec, (js_sys::Date::now() - t_start) as u64,
    )));
    post_chunks(session, &out, &final_avcc, &final_codec);
    return Ok(());
}