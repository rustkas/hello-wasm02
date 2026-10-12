//! Тесты `fsa_choose_*` — построение опций, валидация, классификация ошибок.
//!
//! Правило проекта: любой макрос или выражение, возвращающее значение,
//! в хвостовой позиции функции/`if`/`match`-arm оборачивается в явный
//! `return`.

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use hello_wasm02::fsa_choose_exec::{self, Capabilities, PickOutcome};
use hello_wasm02::fsa_choose_fallback;
use hello_wasm02::fsa_choose_options::{
    build_accept_array, build_directory_options, build_open_options,
    build_save_options, describe_directory_id, describe_filters, AcceptFilter,
    DirectoryMode, FilterPreset,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// AcceptFilter — валидация
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn accept_filter_valid_passes() {
    let f = AcceptFilter::images();
    assert!(f.validate().is_ok());
}

#[wasm_bindgen_test]
fn accept_filter_missing_dot_fails() {
    let f = AcceptFilter::new("Bad").add("image/png", &["png"]);
    assert!(f.validate().is_err());
}

#[wasm_bindgen_test]
fn accept_filter_bad_mime_fails() {
    let f = AcceptFilter::new("Bad").add("not-mime", &[".png"]);
    assert!(f.validate().is_err());
}

#[wasm_bindgen_test]
fn accept_filter_empty_fails() {
    let f = AcceptFilter::new("Empty");
    assert!(f.validate().is_err());
}

#[wasm_bindgen_test]
fn accept_filter_empty_description_fails() {
    let f = AcceptFilter::new("").add("image/png", &[".png"]);
    assert!(f.validate().is_err());
}

#[wasm_bindgen_test]
fn build_open_options_with_invalid_filter_returns_err() {
    let f = AcceptFilter::new("Bad").add("image/png", &["png"]);
    let r = build_open_options(false, false, &[f]);
    assert!(r.is_err());
}

#[wasm_bindgen_test]
fn accept_filter_images_has_png_and_jpeg() {
    let f = AcceptFilter::images();
    let mimetypes: Vec<&str> = f
        .mime_to_extensions
        .iter()
        .map(|(m, _)| m.as_str())
        .collect();
    assert!(mimetypes.contains(&"image/png"));
    assert!(mimetypes.contains(&"image/jpeg"));
}

#[wasm_bindgen_test]
fn accept_filter_plain_text_has_txt() {
    let f = AcceptFilter::plain_text();
    let exts: Vec<&str> = f
        .mime_to_extensions
        .iter()
        .flat_map(|(_, e)| e.iter().map(|s| s.as_str()))
        .collect();
    assert!(exts.contains(&".txt"));
}

// ---------------------------------------------------------------------------
// FilterPreset
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn filter_preset_images_builds_one_filter() {
    let p = FilterPreset::Images;
    let f = p.build();
    assert_eq!(f.len(), 1);
    assert!(f[0].validate().is_ok());
}

#[wasm_bindgen_test]
fn filter_preset_text_builds_one_filter() {
    let p = FilterPreset::Text;
    let f = p.build();
    assert_eq!(f.len(), 1);
    assert!(f[0].validate().is_ok());
}

#[wasm_bindgen_test]
fn filter_preset_none_is_empty() {
    let p = FilterPreset::None;
    let f = p.build();
    assert!(f.is_empty());
}

#[wasm_bindgen_test]
fn filter_preset_all_has_three() {
    assert_eq!(FilterPreset::all().len(), 3);
}

#[wasm_bindgen_test]
fn filter_preset_as_str_nonempty() {
    for p in FilterPreset::all() {
        assert!(!p.as_str().is_empty());
    }
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn capabilities_detect_returns_struct() {
    let caps = Capabilities::detect();
    let _ = caps.any();
    let _ = caps.all();
    let s = caps.summary();
    assert!(s.contains("open"));
    assert!(s.contains("save"));
    assert!(s.contains("directory"));
}

#[wasm_bindgen_test]
fn capabilities_summary_marks_available() {
    let caps = Capabilities {
        open: true,
        save: false,
        directory: true,
    };
    let s = caps.summary();
    assert!(s.contains("open ✓"));
    assert!(s.contains("save —"));
    assert!(s.contains("directory ✓"));
}

#[wasm_bindgen_test]
fn capabilities_any_all_none() {
    let none = Capabilities {
        open: false,
        save: false,
        directory: false,
    };
    assert!(!none.any());
    assert!(!none.all());
}

#[wasm_bindgen_test]
fn capabilities_any_all_full() {
    let all = Capabilities {
        open: true,
        save: true,
        directory: true,
    };
    assert!(all.any());
    assert!(all.all());
}

// ---------------------------------------------------------------------------
// Build options
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn build_accept_filter_array_length_one() {
    let f = AcceptFilter::images();
    let arr = build_accept_array(&[f]).unwrap();
    let arr: js_sys::Array = arr.dyn_into().unwrap();
    assert_eq!(arr.length(), 1);
}

#[wasm_bindgen_test]
fn build_open_options_multiple_and_exclude() {
    let f = AcceptFilter::plain_text();
    let opts = build_open_options(true, true, &[f]).unwrap();
    let multiple = js_sys::Reflect::get(&opts, &"multiple".into()).unwrap();
    assert_eq!(multiple.as_bool(), Some(true));
    let excl =
        js_sys::Reflect::get(&opts, &"excludeAcceptAllOption".into()).unwrap();
    assert_eq!(excl.as_bool(), Some(true));
}

#[wasm_bindgen_test]
fn build_open_options_without_filters_omits_types() {
    let opts = build_open_options(false, false, &[]).unwrap();
    let types = js_sys::Reflect::get(&opts, &"types".into()).unwrap();
    assert!(types.is_undefined());
}

#[wasm_bindgen_test]
fn build_save_options_has_suggested_name() {
    let opts = build_save_options("out.txt", &[]).unwrap();
    let name = js_sys::Reflect::get(&opts, &"suggestedName".into()).unwrap();
    assert_eq!(name.as_string().as_deref(), Some("out.txt"));
}

#[wasm_bindgen_test]
fn build_directory_options_readwrite_mode() {
    let opts =
        build_directory_options(DirectoryMode::ReadWrite, None, None).unwrap();
    let mode = js_sys::Reflect::get(&opts, &"mode".into()).unwrap();
    assert_eq!(mode.as_string().as_deref(), Some("readwrite"));
}

#[wasm_bindgen_test]
fn build_directory_options_read_mode_default() {
    let opts = build_directory_options(DirectoryMode::Read, None, None).unwrap();
    let mode = js_sys::Reflect::get(&opts, &"mode".into()).unwrap();
    assert_eq!(mode.as_string().as_deref(), Some("read"));
}

#[wasm_bindgen_test]
fn build_directory_options_with_id_and_start_in() {
    let opts = build_directory_options(
        DirectoryMode::Read,
        Some("myapp"),
        Some("documents"),
    )
    .unwrap();
    let id = js_sys::Reflect::get(&opts, &"id".into()).unwrap();
    assert_eq!(id.as_string().as_deref(), Some("myapp"));
    let start = js_sys::Reflect::get(&opts, &"startIn".into()).unwrap();
    assert_eq!(start.as_string().as_deref(), Some("documents"));
}

// ---------------------------------------------------------------------------
// Описания
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn describe_filters_empty_mentions_no_filters() {
    let s = describe_filters(&[]);
    assert!(s.contains("фильтры не заданы"));
}

#[wasm_bindgen_test]
fn describe_filters_images_shows_extensions() {
    let s = describe_filters(&[AcceptFilter::images()]);
    assert!(s.contains("Images"));
    assert!(s.contains(".png"));
    assert!(s.contains(".jpg"));
}

#[wasm_bindgen_test]
fn describe_filters_warns_about_invalid() {
    let bad = AcceptFilter::new("Bad").add("image/png", &["png"]);
    let s = describe_filters(&[bad]);
    assert!(s.contains("⚠"));
}

#[wasm_bindgen_test]
fn describe_directory_id_contains_id() {
    let s = describe_directory_id("fsa-choose-demo");
    assert!(s.contains("fsa-choose-demo"));
    assert!(s.contains("id"));
}

// ---------------------------------------------------------------------------
// Exec — feature detection
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn is_fsa_available_returns_bool() {
    let b: bool = fsa_choose_exec::is_fsa_available();
    let _ = b;
}

#[wasm_bindgen_test]
fn ensure_directory_available_consistent_with_caps() {
    let caps = Capabilities::detect();
    let ensured = fsa_choose_exec::ensure_directory_available();
    if caps.directory {
        assert!(ensured.is_none());
    } else {
        assert!(ensured.is_some());
    }
}

// ---------------------------------------------------------------------------
// PickOutcome — классификация без user gesture
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn open_files_without_gesture_is_not_picked() {
    let opts = build_open_options(false, false, &[]).unwrap();
    let outcome = fsa_choose_exec::open_files(opts).await;
    assert!(!outcome.is_picked(), "без gesture выбор невозможен");
    if let PickOutcome::Cancelled = outcome {
        return;
    }
    // Остальные варианты (Blocked / Failed) тоже допустимы.
}

#[wasm_bindgen_test]
async fn save_file_without_gesture_is_not_picked() {
    let opts = build_save_options("t.txt", &[]).unwrap();
    let outcome = fsa_choose_exec::save_file(opts).await;
    assert!(!outcome.is_picked(), "без gesture выбор невозможен");
    if let PickOutcome::Cancelled = outcome {
        return;
    }
}

#[wasm_bindgen_test]
async fn pick_directory_without_gesture_is_not_picked() {
    let opts =
        build_directory_options(DirectoryMode::Read, None, None).unwrap();
    let outcome = fsa_choose_exec::pick_directory(opts).await;
    assert!(!outcome.is_picked(), "без gesture выбор невозможен");
    if let PickOutcome::Cancelled = outcome {
        return;
    }
}

// ---------------------------------------------------------------------------
// PickOutcome — kind_label и предикаты
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn pick_outcome_kind_labels() {
    let ok: PickOutcome<()> = PickOutcome::Picked(());
    assert_eq!(ok.kind_label(), "OK");
    let c: PickOutcome<()> = PickOutcome::Cancelled;
    assert_eq!(c.kind_label(), "отменено");
    let b: PickOutcome<()> = PickOutcome::Blocked("x".into());
    assert_eq!(b.kind_label(), "заблокировано");
    let f: PickOutcome<()> = PickOutcome::Failed("x".into());
    assert_eq!(f.kind_label(), "ошибка");
}

#[wasm_bindgen_test]
fn pick_outcome_is_picked() {
    let ok: PickOutcome<()> = PickOutcome::Picked(());
    assert!(ok.is_picked());
    let c: PickOutcome<()> = PickOutcome::Cancelled;
    assert!(!c.is_picked());
    let b: PickOutcome<()> = PickOutcome::Blocked("x".into());
    assert!(!b.is_picked());
    let f: PickOutcome<()> = PickOutcome::Failed("x".into());
    assert!(!f.is_picked());
}

#[wasm_bindgen_test]
fn pick_outcome_is_cancelled() {
    let ok: PickOutcome<()> = PickOutcome::Picked(());
    assert!(!ok.is_cancelled());
    let c: PickOutcome<()> = PickOutcome::Cancelled;
    assert!(c.is_cancelled());
}

// ---------------------------------------------------------------------------
// Fallback — download_blob и DownloadInfo
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn download_blob_returns_info() {
    let blob = fsa_choose_fallback::make_text_blob("hi").unwrap();
    let info = fsa_choose_fallback::download_blob(&blob, "t.txt").unwrap();
    assert_eq!(info.suggested_name, "t.txt");
    assert!(info.blob_size > 0.0);
}

#[wasm_bindgen_test]
fn describe_download_includes_name_and_size() {
    let info = fsa_choose_fallback::DownloadInfo {
        suggested_name: "t.txt".into(),
        blob_size: 4.0,
    };
    let s = fsa_choose_fallback::describe_download(&info);
    assert!(s.contains("t.txt"));
    assert!(s.contains("4"));
}

#[wasm_bindgen_test]
fn make_text_blob_has_utf8_type() {
    let blob = fsa_choose_fallback::make_text_blob("тест").unwrap();
    assert!(blob.type_().starts_with("text/plain"));
}

#[wasm_bindgen_test]
fn make_text_blob_size_matches_content() {
    let text = "Hello, fallback!";
    let blob = fsa_choose_fallback::make_text_blob(text).unwrap();
    assert_eq!(blob.size() as usize, text.len());
}

#[wasm_bindgen_test]
fn directory_unsupported_message_mentions_browsers() {
    let s = fsa_choose_fallback::directory_unsupported_message();
    assert!(s.contains("Chrome"));
    assert!(s.contains("Edge"));
    assert!(s.contains("<input type=file>"));
}

// ---------------------------------------------------------------------------
// Fallback — интерактивный тест, требует user gesture
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
#[ignore]
async fn pick_via_input_requires_gesture() {
    let out = fsa_choose_fallback::pick_via_input("text/plain").await;
    match out {
        PickOutcome::Cancelled => {}
        PickOutcome::Picked(_) => {}
        PickOutcome::Blocked(_) => {}
        PickOutcome::Failed(_) => {}
    }
}