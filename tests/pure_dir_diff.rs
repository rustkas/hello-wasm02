//! Host-тесты чистой логики `src/fsa_dir_diff.rs`.
//!
//! Работают без WASM, потому что `diff` не делает I/O — только
//! сравнивает два `Snapshot`.

use std::collections::BTreeMap;

use hello_wasm02::fsa_dir_diff::{diff, FileMeta, Snapshot};

fn snap(items: &[(&str, f64)]) -> Snapshot {
    let mut entries = BTreeMap::new();
    for (path, size) in items {
        entries.insert(
            (*path).to_string(),
            FileMeta {
                size: *size,
                mime: String::new(),
            },
        );
    }
    Snapshot { entries }
}

#[test]
fn diff_identical_snapshots() {
    let a = snap(&[("a.txt", 10.0), ("b.txt", 20.0)]);
    let b = snap(&[("a.txt", 10.0), ("b.txt", 20.0)]);
    let d = diff(&a, &b);
    assert!(d.added.is_empty());
    assert!(d.removed.is_empty());
    assert!(d.size_changed.is_empty());
    assert_eq!(d.unchanged_count, 2);
    assert_eq!(d.total_changes(), 0);
}

#[test]
fn diff_added_and_removed() {
    let a = snap(&[("keep.txt", 1.0), ("old.txt", 2.0)]);
    let b = snap(&[("keep.txt", 1.0), ("new.txt", 3.0)]);
    let d = diff(&a, &b);
    assert_eq!(d.added, vec!["new.txt".to_string()]);
    assert_eq!(d.removed, vec!["old.txt".to_string()]);
    assert!(d.size_changed.is_empty());
    assert_eq!(d.unchanged_count, 1);
    assert_eq!(d.total_changes(), 2);
}

#[test]
fn diff_size_changed() {
    let a = snap(&[("f.bin", 100.0)]);
    let b = snap(&[("f.bin", 250.0)]);
    let d = diff(&a, &b);
    assert_eq!(d.size_changed, vec![("f.bin".to_string(), 100.0, 250.0)]);
    assert_eq!(d.unchanged_count, 0);
}

#[test]
fn diff_empty_to_full() {
    let a = snap(&[]);
    let b = snap(&[("x", 1.0), ("y", 2.0)]);
    let d = diff(&a, &b);
    assert_eq!(d.added.len(), 2);
    assert!(d.removed.is_empty());
    assert_eq!(d.total_changes(), 2);
}

#[test]
fn diff_full_to_empty() {
    let a = snap(&[("x", 1.0), ("y", 2.0)]);
    let b = snap(&[]);
    let d = diff(&a, &b);
    assert!(d.added.is_empty());
    assert_eq!(d.removed.len(), 2);
    assert_eq!(d.total_changes(), 2);
}

#[test]
fn diff_describe_is_stable() {
    let a = snap(&[("keep", 1.0), ("gone", 2.0)]);
    let b = snap(&[("keep", 1.0), ("fresh", 3.0), ("grown", 4.0)]);
    let d = diff(&a, &b);
    // added — в алфавитном порядке (BTreeSet).
    assert_eq!(d.added, vec!["fresh".to_string(), "grown".to_string()]);
    let lines = d.describe();
    assert_eq!(lines[0], "diff: +2 / -1 / ~0 / =1");
    assert!(lines.iter().any(|l| l.starts_with("  + fresh")));
    assert!(lines.iter().any(|l| l.starts_with("  - gone")));
}

#[test]
fn diff_order_is_deterministic() {
    // Один и тот же diff, посчитанный много раз, даёт один и тот же
    // порядок — благодаря BTreeMap/BTreeSet, а не HashMap.
    let a = snap(&[("a", 1.0), ("c", 3.0), ("e", 5.0)]);
    let b = snap(&[("b", 2.0), ("d", 4.0)]);
    let first = diff(&a, &b);
    for _ in 0..20 {
        assert_eq!(diff(&a, &b), first);
    }
}