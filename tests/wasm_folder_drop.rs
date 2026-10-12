use wasm_bindgen_test::*;
use hello_wasm02::folder_drop::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_folder_entry_construction() {
    let entry = FolderEntry {
        path: "photos/2024/img001.png".to_string(),
        size: 12345.0,
        mime_type: "image/png".to_string(),
    };
    assert_eq!(entry.path, "photos/2024/img001.png");
    assert_eq!(entry.size, 12345.0);
    assert_eq!(entry.mime_type, "image/png");
}

#[wasm_bindgen_test]
fn test_folder_entry_equality() {
    let a = FolderEntry {
        path: "a.txt".to_string(),
        size: 100.0,
        mime_type: "text/plain".to_string(),
    };
    let b = FolderEntry {
        path: "a.txt".to_string(),
        size: 100.0,
        mime_type: "text/plain".to_string(),
    };
    let c = FolderEntry {
        path: "b.txt".to_string(),
        size: 100.0,
        mime_type: "text/plain".to_string(),
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[wasm_bindgen_test]
fn test_folder_entry_clone() {
    let original = FolderEntry {
        path: "dir/file.bin".to_string(),
        size: 4096.0,
        mime_type: String::new(),
    };
    let cloned = original.clone();
    assert_eq!(original, cloned);
}

#[wasm_bindgen_test]
fn test_entry_from_item_without_data_transfer() {
    // Синтетическое событие без DataTransfer — entry_from_item
    // не должен паниковать; для теста создаём пустой DataTransferItem
    // невозможно, поэтому проверяем через создание события.
    // Реальная проверка требует настоящего drop-события.
    // Здесь фиксируем, что сигнатура функции корректна и компилируется.
    let _fn: fn(&web_sys::DataTransferItem) -> Option<web_sys::FileSystemEntry> =
        entry_from_item;
}