//! hello-wasm02 — демонстрация Rust + web_sys.
//!
//! Публичные модули:
//! - `app` — практические проекты глав.
//! - `async_utils` — помощники для асинхронной работы.
//! - `callbacks` — управление временем жизни `Closure`.
//! - `dom` — помощники для работы с DOM.
//! - `events` — помощники для работы с событиями.
//! - `fetch` — работа с Fetch API.
//! - `value` — помощники для работы с `JsValue`.
//! - `js_objects` — помощники для `Array`, `Map`, `Set`, `Promise`, `Date`.
//! - `navigation` — работа с URL, Location, History, Navigation API.
//! - `storage` — работа с localStorage и sessionStorage.
//! - `notifier` — система уведомлений с Rust-callback API.

pub mod app;
pub mod api_client;
pub mod async_utils;
pub mod auth; 
pub mod callbacks;
pub mod dom;
pub mod events;
pub mod fetch;
pub mod js_objects;
pub mod navigation;
pub mod notifier;
pub mod storage;
pub mod streams;
pub mod streams_wasm;
pub mod value;

// … существующие модули …
pub mod file_api;
pub mod file_reader;
pub mod file_utils;
pub mod drag_drop;
pub mod file_upload;
pub mod object_url;
pub mod data_url; 
pub mod base64_utils;
pub mod folder_drop;
pub mod validation;
pub mod blob_slice;
pub mod blob_stream;
pub mod worker_file;
pub mod hashing;
pub mod upload_progress;
pub mod chunked_upload;
pub mod upload_strategy;
pub mod fsa_open;
pub mod fsa_save;

pub mod sharing;

pub mod cache_storage;
pub mod idb_store;
pub mod opfs;

pub mod converter;
pub mod converter_ui;
pub mod converter_worker;
pub mod converter_native;

pub mod converter_app; 

pub mod fsa_api_handle;
pub mod fsa_api_file;
pub mod fsa_api_directory;
pub mod fsa_api_writable;
pub mod fsa_api_pickers;
pub mod fsa_api_permissions;
pub mod fsa_api_reader;
pub mod fsa_api_writer;
pub mod fsa_api_large;
pub mod fsa_api_project;
pub mod fsa_lifecycle;
pub mod fsa_choose_options;
pub mod fsa_choose_exec;
pub mod fsa_choose_fallback;
pub mod fsa_grant_state;
pub mod fsa_grant_flow;
pub mod fsa_grant_store;
pub mod fsa_read_plan;
pub mod fsa_read_ops;
pub mod fsa_write_session;
pub mod fsa_write_ops;
pub mod fsa_large_chunk;
pub mod fsa_large_copy;
pub mod fsa_dir_tree;
pub mod fsa_dir_diff;
pub mod fsa_api_opfs;
pub mod fsa_api_quota;
pub mod fsa_api_observer;
pub mod fsa_api_wasm;
pub mod csv_editor;
pub mod blob_api;
pub mod blob_container;
pub mod object_url_lifecycle;
pub mod format_str;
pub mod download_paths;
pub mod blob_processor;
pub mod canvas_image_processor;
pub mod canvas_dpi;
pub mod canvas_drawing;
pub mod canvas_composition;
pub mod canvas_imagedata;
pub mod canvas_image_loading;
pub mod canvas_filters;
pub mod canvas_image_basic;
pub mod canvas_image_pipeline;
pub mod offscreen_worker;
pub mod offscreen_canvas;
pub mod dedicated_worker;
pub mod transferable;
pub mod wasm_in_worker;
pub mod worker_pool;
pub mod fractal;
pub mod codecs_demo;
pub mod video_frame;
pub mod video_decoder;
pub mod video_encoder;
pub mod video_encoder_tuning;
pub mod audio_codecs;
pub mod image_decoder;
pub mod pixel_ops;
pub mod media_stream;
pub mod worker_processor;
pub mod container_parser;
pub mod mp4_demuxer;
pub mod mp4_muxer;
pub mod video_filters;
pub mod pipeline_worker;
pub mod audio_api;
pub mod audio_dsp;
pub mod video_encoder_scalability;
pub mod idb_core;
pub mod idb_library;
pub mod cache_api;
pub mod cache_offline;
pub mod sw_core;
pub mod sw_offline;
pub mod offline_core;
pub mod offline_state;
pub mod offline_store;
pub mod offline_server;
pub mod offline_sync;
pub mod worker_core;
pub mod worker_pipeline;
pub mod sab_core;
pub mod sab_pipeline;
pub mod sched_core;
pub mod sched_pipeline;
pub mod perf_core;
pub mod perf_observer;
pub mod perf_timing;
pub mod perf_lab;
pub mod ls_probe;
pub mod ls_bloat;
pub mod ls_bench;
pub mod idb_factory;
pub mod idb_db;
pub mod idb_schema;
pub mod idb_records;
pub mod idb_upsert;
pub mod clip_core;
pub mod clip_perms;
pub mod paste_evt;
pub mod clip_async;
pub mod notif_core;
pub mod notif_sw;
pub mod geo_core;
pub mod media_devices;
pub mod ws_client;
pub mod wt_client;
pub mod rtc_peer;
pub mod rta_protocol;
pub mod rta_transport;
pub mod web_crypto;
pub mod webauthn; 
pub mod webgl;
pub mod webgl2;
pub mod webgpu;
pub mod bluetooth_gatt;
pub mod bluetooth_utils;
pub mod arch_js_bench;
pub mod arch_pure;
pub mod error_info;
pub mod error_scenarios;
pub mod panic_guard;
pub mod panic_scenarios;
pub mod file_convert;
pub mod file_io;


mod form;
mod logging;

use wasm_bindgen::prelude::*;




/// Точка входа. Вызывается автоматически при инициализации Wasm-модуля.
#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    if is_worker_context() {
        return Ok(());
    }
    logging::init();
    app::run()
}

/// true, если код исполняется в Worker-контексте (нет `window`).
fn is_worker_context() -> bool {
    let global = js_sys::global();
    let window = js_sys::Reflect::get(&global, &JsValue::from_str("window"))
        .unwrap_or(JsValue::UNDEFINED);
    window.is_undefined()
}