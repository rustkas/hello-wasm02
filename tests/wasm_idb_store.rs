use wasm_bindgen_test::*;
use hello_wasm02::idb_store::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_put_get_roundtrip() {
    let entry = StoredEntry {
        id: "test-1".to_string(),
        name: "test.txt".to_string(),
        mime: "text/plain".to_string(),
        bytes: b"hello".to_vec(),
    };
    put_entry(&entry).await.expect("put_entry");

    let read = get_entry("test-1").await.expect("get_entry");
    assert_eq!(read, Some(entry.clone()));

    // Убираем.
    let _ = delete_entry("test-1").await;
}

#[wasm_bindgen_test]
async fn test_get_missing_returns_none() {
    let read = get_entry("no-such-id").await.expect("get_entry");
    assert!(read.is_none(), "не должно быть записи");
}

#[wasm_bindgen_test]
async fn test_delete_idempotent() {
    // Удаляем дважды — оба Ok.
    delete_entry("no-such-id").await.expect("first delete");
    delete_entry("no-such-id").await.expect("second delete");
}

#[wasm_bindgen_test]
async fn test_list_contains_inserted() {
    let id = format!("list-test-{}", js_sys::Date::now() as u64);
    let entry = StoredEntry {
        id: id.clone(),
        name: "list.txt".to_string(),
        mime: "text/plain".to_string(),
        bytes: vec![1, 2, 3],
    };
    put_entry(&entry).await.expect("put_entry");

    let list = list_entries().await.expect("list_entries");
    let found = list.iter().any(|e| e.id == id);
    assert!(found, "запись {id} должна быть в списке");

    let _ = delete_entry(&id).await;
}