//! Тесты `fsa_large_*` — план чанков, EMA скорости, копирование OPFS → OPFS,
//! counterfactual без yield, SHA-256 источника и приёмника.

mod opfs {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::FileSystemDirectoryHandle;

    pub async fn root() -> Result<FileSystemDirectoryHandle, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let navigator_js = JsValue::from(window.navigator());
        let storage_js = js_sys::Reflect::get(&navigator_js, &JsValue::from_str("storage"))?;
        if storage_js.is_undefined() {
            return Err(JsValue::from_str("navigator.storage недоступен"));
        }
        let get_dir_js = js_sys::Reflect::get(&storage_js, &JsValue::from_str("getDirectory"))?;
        if get_dir_js.is_undefined() {
            return Err(JsValue::from_str("OPFS недоступен"));
        }
        let get_dir: js_sys::Function = get_dir_js
            .dyn_into()
            .map_err(|_| JsValue::from_str("getDirectory не функция"))?;
        let promise: js_sys::Promise = get_dir
            .call0(&storage_js)?
            .dyn_into()
            .map_err(|_| JsValue::from_str("не Promise"))?;
        let value = JsFuture::from(promise).await?;
        value
            .dyn_into::<FileSystemDirectoryHandle>()
            .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
    }
}

use hello_wasm02::fsa_large_chunk::{
    format_bytes, format_chunk_sizes, ChunkPlan, CopyStats,
    SpeedEma, MAX_CHUNK, MIN_CHUNK,
};
use hello_wasm02::fsa_large_copy::{
    copy_stream, copy_stream_with_options, format_hash_verdict, sha256_file,
    CopyOptions, CopyOutcome,
};
use hello_wasm02::fsa_write_ops::{read_bytes_now, WriteSession};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// ChunkPlan
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn plan_small_file_uses_min_chunk() {
    let plan = ChunkPlan::for_total(4 * 1024);
    assert_eq!(plan.chunk_size, MIN_CHUNK);
    assert_eq!(plan.expected_chunks, 1);
}

#[wasm_bindgen_test]
fn plan_mid_file_within_bounds() {
    let plan = ChunkPlan::for_total(200 * 1024 * 1024);
    assert!(plan.chunk_size >= MIN_CHUNK);
    assert!(plan.chunk_size <= MAX_CHUNK);
    assert_eq!(plan.expected_chunks, 200);
}

#[wasm_bindgen_test]
fn plan_huge_file_clamps_to_max() {
    let plan = ChunkPlan::for_total(10 * 1024 * 1024 * 1024);
    assert_eq!(plan.chunk_size, MAX_CHUNK);
}

#[wasm_bindgen_test]
fn plan_zero_size() {
    let plan = ChunkPlan::for_total(0);
    assert_eq!(plan.expected_chunks, 0);
    assert_eq!(plan.memory_saved(), 0);
}

#[wasm_bindgen_test]
fn plan_memory_saved_positive_for_large() {
    let plan = ChunkPlan::for_total(100 * 1024 * 1024);
    assert!(plan.memory_saved() > 0);
    assert_eq!(plan.memory_saved(), plan.total_bytes - plan.chunk_size as u64);
}

#[wasm_bindgen_test]
fn plan_describe_contains_all_parts() {
    let plan = ChunkPlan::for_total(100 * 1024 * 1024);
    let s = plan.describe();
    assert!(s.contains("План"));
    assert!(s.contains("Peak memory"));
    assert!(s.contains("экономия"));
}

// ---------------------------------------------------------------------------
// SpeedEma
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn ema_first_value_is_raw() {
    let mut ema = SpeedEma::new(0.25);
    ema.update(1_048_576, 1000.0);
    assert!((ema.value_mbps() - 1.0).abs() < 1e-6);
}

#[wasm_bindgen_test]
fn ema_smooths_subsequent_values() {
    let mut ema = SpeedEma::new(0.5);
    ema.update(1_048_576, 1000.0);
    ema.update(3 * 1_048_576, 1000.0);
    assert!((ema.value_mbps() - 2.0).abs() < 1e-6);
}

#[wasm_bindgen_test]
fn ema_ignores_zero_elapsed() {
    let mut ema = SpeedEma::new(0.25);
    ema.update(1_048_576, 1000.0);
    let before = ema.value_mbps();
    ema.update(1_048_576, 0.0);
    assert_eq!(ema.value_mbps(), before);
}

// ---------------------------------------------------------------------------
// CopyStats
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn stats_bottleneck_read() {
    let s = CopyStats {
        bytes_copied: 100, chunks: 1, yields: 0, progress_calls: 0,
        read_ms: 80.0, write_ms: 20.0, total_ms: 100.0,
        chunk_size: 100, cancelled: false, first_chunk_sizes: vec![100],
    };
    assert_eq!(s.bottleneck(), "чтение");
    assert!((s.read_share() - 0.8).abs() < 1e-6);
}

#[wasm_bindgen_test]
fn stats_bottleneck_write() {
    let s = CopyStats {
        bytes_copied: 100, chunks: 1, yields: 0, progress_calls: 0,
        read_ms: 20.0, write_ms: 80.0, total_ms: 100.0,
        chunk_size: 100, cancelled: false, first_chunk_sizes: vec![100],
    };
    assert_eq!(s.bottleneck(), "запись");
}

#[wasm_bindgen_test]
fn stats_matches_requires_not_cancelled() {
    let s = CopyStats {
        bytes_copied: 100, chunks: 1, yields: 0, progress_calls: 0,
        read_ms: 1.0, write_ms: 1.0, total_ms: 2.0,
        chunk_size: 100, cancelled: true, first_chunk_sizes: vec![100],
    };
    assert!(!s.matches(100), "cancel отменяет match");
}

// ---------------------------------------------------------------------------
// Форматтеры
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn format_bytes_units() {
    assert_eq!(format_bytes(0), "0 Б");
    assert_eq!(format_bytes(512), "512 Б");
    assert_eq!(format_bytes(2048), "2.0 КБ");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 МБ");
}

#[wasm_bindgen_test]
fn format_chunk_sizes_empty() {
    assert_eq!(format_chunk_sizes(&[]), "(нет)");
}

#[wasm_bindgen_test]
fn format_chunk_sizes_nonempty() {
    let s = format_chunk_sizes(&[1024, 2048]);
    assert!(s.contains("1.0 КБ"));
    assert!(s.contains("2.0 КБ"));
}

// ---------------------------------------------------------------------------
// Копирование OPFS → OPFS
// ---------------------------------------------------------------------------

async fn prepare_src(
    root: &web_sys::FileSystemDirectoryHandle,
    name: &str,
    size: usize,
) -> web_sys::FileSystemFileHandle {
    let _ = hello_wasm02::fsa_api_directory::remove_entry(root, name, true).await;
    let handle = hello_wasm02::fsa_api_directory::get_file_handle(root, name, true)
        .await
        .expect("create src");
    let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let mut s = WriteSession::open(&handle, false).await.expect("open src");
    s.write(&data).await.expect("write src");
    s.commit().await.expect("commit src");
    return handle;
}

async fn prepare_dst(
    root: &web_sys::FileSystemDirectoryHandle,
    name: &str,
) -> web_sys::FileSystemFileHandle {
    let _ = hello_wasm02::fsa_api_directory::remove_entry(root, name, true).await;
    hello_wasm02::fsa_api_directory::get_file_handle(root, name, true)
        .await
        .expect("create dst")
}

#[wasm_bindgen_test]
async fn copy_small_file_bitwise_equal() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let size = 256 * 1024;
    let src_handle = prepare_src(&root, "lg_src_small.bin", size).await;
    let dst_handle = prepare_dst(&root, "lg_dst_small.bin").await;

    let src_file = hello_wasm02::fsa_api_file::get_file(&src_handle)
        .await
        .expect("get src file");

    let outcome = copy_stream(&src_file, &dst_handle, |_, _| {}, || false)
        .await
        .expect("copy");

    match outcome {
        CopyOutcome::Done(stats) => {
            assert_eq!(stats.bytes_copied, size as u64);
            assert!(stats.matches(size as u64));
            assert!(!stats.cancelled);
        }
        CopyOutcome::Cancelled(_) => panic!("не должно быть отменено"),
    }

    let src_bytes = read_bytes_now(&src_handle).await.expect("read src");
    let dst_bytes = read_bytes_now(&dst_handle).await.expect("read dst");
    assert_eq!(src_bytes, dst_bytes, "побитовое совпадение");

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_src_small.bin", true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_dst_small.bin", true).await;
}

#[wasm_bindgen_test]
async fn copy_4mb_bitwise_equal() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let size = 4 * 1024 * 1024;
    let src_handle = prepare_src(&root, "lg_src_4mb.bin", size).await;
    let dst_handle = prepare_dst(&root, "lg_dst_4mb.bin").await;

    let src_file = hello_wasm02::fsa_api_file::get_file(&src_handle)
        .await
        .expect("get src file");

    let progress_calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let pc = progress_calls.clone();
    let outcome = copy_stream(
        &src_file,
        &dst_handle,
        move |_frac, _speed| pc.set(pc.get() + 1),
        || false,
    )
    .await
    .expect("copy");

    match outcome {
        CopyOutcome::Done(stats) => {
            assert!(stats.matches(size as u64));
            assert!(stats.yields >= 1, "yield должен сработать на 4 МБ");
            assert!(stats.chunks >= 1);
        }
        CopyOutcome::Cancelled(_) => panic!("не должно быть отменено"),
    }
    assert!(progress_calls.get() >= 1, "on_progress вызывался");

    let src_bytes = read_bytes_now(&src_handle).await.expect("read src");
    let dst_bytes = read_bytes_now(&dst_handle).await.expect("read dst");
    assert_eq!(src_bytes.len(), dst_bytes.len());
    assert_eq!(src_bytes, dst_bytes);

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_src_4mb.bin", true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_dst_4mb.bin", true).await;
}

#[wasm_bindgen_test]
async fn copy_cancel_discards_destination() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let size = 2 * 1024 * 1024;
    let src_handle = prepare_src(&root, "lg_src_cancel.bin", size).await;
    let dst_handle = prepare_dst(&root, "lg_dst_cancel.bin").await;

    let src_file = hello_wasm02::fsa_api_file::get_file(&src_handle)
        .await
        .expect("get src file");

    let cancel = std::rc::Rc::new(std::cell::Cell::new(true));
    let c = cancel.clone();
    let outcome = copy_stream(&src_file, &dst_handle, |_, _| {}, move || c.get())
        .await
        .expect("copy");

    match outcome {
        CopyOutcome::Cancelled(stats) => {
            assert!(stats.cancelled);
        }
        CopyOutcome::Done(_) => panic!("должно быть отменено"),
    }

    let src_bytes = read_bytes_now(&src_handle).await.expect("read src");
    assert_eq!(src_bytes.len(), size);

    let dst_size = match hello_wasm02::fsa_api_file::get_file(&dst_handle).await {
        Ok(f) => f.size() as u64,
        Err(_) => 0,
    };
    assert_eq!(dst_size, 0, "destination не остаётся с частичными данными");

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_src_cancel.bin", true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_dst_cancel.bin", true).await;
}

#[wasm_bindgen_test]
async fn copy_progress_monotonic() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let size = 512 * 1024;
    let src_handle = prepare_src(&root, "lg_src_prog.bin", size).await;
    let dst_handle = prepare_dst(&root, "lg_dst_prog.bin").await;

    let src_file = hello_wasm02::fsa_api_file::get_file(&src_handle)
        .await
        .expect("get src file");

    let values: std::rc::Rc<std::cell::RefCell<Vec<f64>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let v = values.clone();
    let _ = copy_stream(
        &src_file,
        &dst_handle,
        move |frac, _speed| v.borrow_mut().push(frac),
        || false,
    )
    .await
    .expect("copy");

    let vec = values.borrow();
    for w in vec.windows(2) {
        assert!(w[1] + 1e-9 >= w[0], "прогресс монотонный");
    }
    let last = *vec.last().unwrap_or(&0.0);
    assert!((last - 1.0).abs() < 1e-6, "финальный прогресс = 1.0");

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_src_prog.bin", true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_dst_prog.bin", true).await;
}

// ---------------------------------------------------------------------------
// no-yield vs with-yield
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn copy_no_yield_has_one_progress_call() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let size = 512 * 1024;
    let src_handle = prepare_src(&root, "lg_src_noyield.bin", size).await;
    let dst_handle = prepare_dst(&root, "lg_dst_noyield.bin").await;

    let src_file = hello_wasm02::fsa_api_file::get_file(&src_handle)
        .await
        .expect("get src file");

    let calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let c = calls.clone();
    let outcome = copy_stream_with_options(
        &src_file,
        &dst_handle,
        CopyOptions::without_yield(),
        move |_, _| c.set(c.get() + 1),
        || false,
    )
    .await
    .expect("copy");

    match outcome {
        CopyOutcome::Done(stats) => {
            assert_eq!(stats.yields, 0, "без yield не должно быть yield'ов");
            assert_eq!(stats.progress_calls, 1, "on_progress только в конце");
        }
        _ => panic!("должно быть Done"),
    }

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_src_noyield.bin", true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_dst_noyield.bin", true).await;
}

// ---------------------------------------------------------------------------
// sha256_file
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn sha256_matches_for_identical_files() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let size = 256 * 1024;
    let src_handle = prepare_src(&root, "lg_src_hash.bin", size).await;
    let dst_handle = prepare_dst(&root, "lg_dst_hash.bin").await;

    let src_file = hello_wasm02::fsa_api_file::get_file(&src_handle)
        .await
        .expect("get src file");

    let _ = copy_stream(
        &src_file,
        &dst_handle,
        |_, _| {},
        || false,
    )
    .await
    .expect("copy");

    let src_hash = sha256_file(&src_handle).await.expect("hash src");
    let dst_hash = sha256_file(&dst_handle).await.expect("hash dst");

    assert_eq!(src_hash, dst_hash, "хеши источника и приёмника совпадают");
    assert_eq!(src_hash.len(), 64);

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_src_hash.bin", true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "lg_dst_hash.bin", true).await;
}

#[wasm_bindgen_test]
fn hash_verdict_matches() {
    let h = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let s = format_hash_verdict(h, h);
    assert!(s.contains("✓"));
    assert!(s.contains("abcdef0123456789"));
}

#[wasm_bindgen_test]
fn hash_verdict_mismatch() {
    let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let s = format_hash_verdict(a, b);
    assert!(s.contains("✗"));
}