use wasm_bindgen_test::*;
use hello_wasm02::fsa_api_project::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_report_to_csv_empty() {
    let report = AuditReport { rows: vec![], truncated: false };
    let csv = report_to_csv(&report);
    assert_eq!(csv, "path,size_bytes,mime,sha256_prefix\n");
}

#[wasm_bindgen_test]
fn test_report_to_csv_with_rows() {
    let report = AuditReport {
        rows: vec![
            AuditRow {
                path: "a.txt".into(),
                size: 12.0,
                mime: "text/plain".into(),
                hash_prefix: "abcdef0123456789".into(),
            },
        ],
        truncated: false,
    };
    let csv = report_to_csv(&report);
    assert!(csv.contains("a.txt,12,text/plain,abcdef0123456789"));
}

#[wasm_bindgen_test]
fn test_report_to_csv_escapes_commas() {
    let report = AuditReport {
        rows: vec![AuditRow {
            path: "a,b.txt".into(),
            size: 0.0,
            mime: "text/plain".into(),
            hash_prefix: "0".repeat(16),
        }],
        truncated: false,
    };
    let csv = report_to_csv(&report);
    assert!(csv.contains("\"a,b.txt\""));
}

#[wasm_bindgen_test]
fn test_report_to_csv_marks_truncation() {
    let report = AuditReport { rows: vec![], truncated: true };
    let csv = report_to_csv(&report);
    assert!(csv.contains("# truncated"));
}