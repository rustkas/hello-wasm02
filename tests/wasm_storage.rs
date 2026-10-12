//! Тесты для Web Storage API: базовые операции + полный API.
//!
//! Покрытие:
//! - Доступность localStorage и sessionStorage.
//! - Три базовые операции: set, get, remove.
//! - Edge cases: отсутствующий ключ, пустая строка, Unicode.
//! - Итерация: length, key(i), keys, entries.
//! - has_key.
//! - clear.
use wasm_bindgen_test::*;

use hello_wasm02::storage;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// Доступность хранилищ
// ============================================================

#[wasm_bindgen_test]
fn test_local_storage_available() {
    let ls = storage::local_storage();
    assert!(ls.is_ok(), "localStorage should be available");
}

#[wasm_bindgen_test]
fn test_session_storage_available() {
    let ss = storage::session_storage();
    assert!(ss.is_ok(), "sessionStorage should be available");
}

// ============================================================
// Базовые операции
// ============================================================

#[wasm_bindgen_test]
fn test_set_and_get() {
    let ls = storage::local_storage().unwrap();

    storage::set(&ls, "test:set_get", "hello").unwrap();
    assert_eq!(storage::get(&ls, "test:set_get"), Some("hello".into()));

    storage::remove(&ls, "test:set_get").unwrap();
    assert_eq!(storage::get(&ls, "test:set_get"), None);
}

#[wasm_bindgen_test]
fn test_get_missing_key() {
    let ls = storage::local_storage().unwrap();

    storage::remove(&ls, "test:missing").ok();

    assert_eq!(storage::get(&ls, "test:missing"), None);
}

#[wasm_bindgen_test]
fn test_remove() {
    let ls = storage::local_storage().unwrap();

    storage::set(&ls, "test:remove", "value").unwrap();
    assert_eq!(storage::get(&ls, "test:remove"), Some("value".into()));

    storage::remove(&ls, "test:remove").unwrap();
    assert_eq!(storage::get(&ls, "test:remove"), None);
}

// ============================================================
// Edge cases
// ============================================================

#[wasm_bindgen_test]
fn test_set_empty_string() {
    let ls = storage::local_storage().unwrap();

    storage::set(&ls, "test:empty", "").unwrap();
    assert_eq!(storage::get(&ls, "test:empty"), Some("".into()));

    storage::remove(&ls, "test:empty").ok();
}

#[wasm_bindgen_test]
fn test_set_unicode_string() {
    let ls = storage::local_storage().unwrap();

    let unicode = "Привет, мир! 🦀 Rust + Wasm = ❤";
    storage::set(&ls, "test:unicode", unicode).unwrap();

    assert_eq!(
        storage::get(&ls, "test:unicode"),
        Some(unicode.to_string())
    );

    storage::remove(&ls, "test:unicode").ok();
}

// ============================================================
// Итерация: length, key(i), keys, entries
// ============================================================

#[wasm_bindgen_test]
fn test_length_empty_storage() {
    let ls = storage::local_storage().unwrap();

    // Сохраняем состояние, очищаем, проверяем
    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    assert_eq!(storage::length(&ls), 0);

    // Восстанавливаем
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_length_after_inserts() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    storage::set(&ls, "test:len1", "a").unwrap();
    storage::set(&ls, "test:len2", "b").unwrap();
    storage::set(&ls, "test:len3", "c").unwrap();

    assert_eq!(storage::length(&ls), 3);

    storage::clear(&ls).ok();
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_key_by_index() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    storage::set(&ls, "test:first", "1").unwrap();

    // Первый ключ доступен по индексу 0
    assert_eq!(storage::key(&ls, 0), Some("test:first".into()));

    // Индекс вне диапазона
    assert_eq!(storage::key(&ls, 99), None);

    storage::clear(&ls).ok();
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_keys_iteration() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    storage::set(&ls, "test:key1", "v1").unwrap();
    storage::set(&ls, "test:key2", "v2").unwrap();

    let keys = storage::keys(&ls);
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"test:key1".to_string()));
    assert!(keys.contains(&"test:key2".to_string()));

    storage::clear(&ls).ok();
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_keys_empty_storage() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    let keys = storage::keys(&ls);
    assert!(keys.is_empty());

    storage::clear(&ls).ok();
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_entries_consistency() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    storage::set(&ls, "test:a", "1").unwrap();
    storage::set(&ls, "test:b", "2").unwrap();
    storage::set(&ls, "test:c", "3").unwrap();

    let entries = storage::entries(&ls);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries.len() as u32, storage::length(&ls));

    // Каждая пара из entries доступна через get
    for (key, value) in &entries {
        assert_eq!(storage::get(&ls, key), Some(value.clone()));
    }

    storage::clear(&ls).ok();
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

// ============================================================
// has_key
// ============================================================

#[wasm_bindgen_test]
fn test_has_key() {
    let ls = storage::local_storage().unwrap();

    storage::set(&ls, "test:has", "value").unwrap();

    assert!(storage::has_key(&ls, "test:has"));
    assert!(!storage::has_key(&ls, "test:nonexistent"));

    storage::remove(&ls, "test:has").ok();
}

// ============================================================
// clear
// ============================================================

#[wasm_bindgen_test]
fn test_clear_removes_all() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);

    storage::set(&ls, "test:clear1", "a").unwrap();
    storage::set(&ls, "test:clear2", "b").unwrap();
    assert!(storage::length(&ls) >= 2);

    storage::clear(&ls).unwrap();
    assert_eq!(storage::length(&ls), 0);

    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_clear_empty_storage() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    // Повторный clear на пустом хранилище не должен вызывать ошибок
    let result = storage::clear(&ls);
    assert!(result.is_ok());
    assert_eq!(storage::length(&ls), 0);

    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

// ============================================================
// Изоляция localStorage и sessionStorage
// ============================================================

#[wasm_bindgen_test]
fn test_local_and_session_are_separate() {
    let ls = storage::local_storage().unwrap();
    let ss = storage::session_storage().unwrap();

    storage::set(&ls, "test:sep", "local").unwrap();
    storage::set(&ss, "test:sep", "session").unwrap();

    assert_eq!(storage::get(&ls, "test:sep"), Some("local".into()));
    assert_eq!(storage::get(&ss, "test:sep"), Some("session".into()));

    storage::remove(&ls, "test:sep").ok();
    storage::remove(&ss, "test:sep").ok();
}

#[wasm_bindgen_test]
fn test_clearing_local_does_not_affect_session() {
    let ls = storage::local_storage().unwrap();
    let ss = storage::session_storage().unwrap();

    storage::set(&ls, "test:clear_iso", "local_data").unwrap();
    storage::set(&ss, "test:clear_iso", "session_data").unwrap();

    storage::clear(&ls).ok();

    // sessionStorage не затронут очисткой localStorage
    assert_eq!(
        storage::get(&ss, "test:clear_iso"),
        Some("session_data".into())
    );

    storage::remove(&ss, "test:clear_iso").ok();
}

#[wasm_bindgen_test]
fn test_clearing_session_does_not_affect_local() {
    let ls = storage::local_storage().unwrap();
    let ss = storage::session_storage().unwrap();

    storage::set(&ls, "test:clear_iso2", "local_data").unwrap();
    storage::set(&ss, "test:clear_iso2", "session_data").unwrap();

    storage::clear(&ss).ok();

    // localStorage не затронут очисткой sessionStorage
    assert_eq!(
        storage::get(&ls, "test:clear_iso2"),
        Some("local_data".into())
    );

    storage::remove(&ls, "test:clear_iso2").ok();
}

#[wasm_bindgen_test]
fn test_local_and_session_length_independent() {
    let ls = storage::local_storage().unwrap();
    let ss = storage::session_storage().unwrap();

    let original_ls: Vec<_> = storage::entries(&ls);
    let original_ss: Vec<_> = storage::entries(&ss);

    storage::clear(&ls).ok();
    storage::clear(&ss).ok();

    storage::set(&ls, "ls1", "a").unwrap();
    storage::set(&ls, "ls2", "b").unwrap();
    storage::set(&ls, "ls3", "c").unwrap();
    storage::set(&ss, "ss1", "x").unwrap();

    assert_eq!(storage::length(&ls), 3);
    assert_eq!(storage::length(&ss), 1);

    storage::clear(&ls).ok();
    storage::clear(&ss).ok();

    for (k, v) in original_ls {
        storage::set(&ls, &k, &v).ok();
    }
    for (k, v) in original_ss {
        storage::set(&ss, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_local_and_session_keys_independent() {
    let ls = storage::local_storage().unwrap();
    let ss = storage::session_storage().unwrap();

    let original_ls: Vec<_> = storage::entries(&ls);
    let original_ss: Vec<_> = storage::entries(&ss);

    storage::clear(&ls).ok();
    storage::clear(&ss).ok();

    storage::set(&ls, "only_local", "a").unwrap();
    storage::set(&ss, "only_session", "b").unwrap();

    let ls_keys = storage::keys(&ls);
    let ss_keys = storage::keys(&ss);

    assert!(ls_keys.contains(&"only_local".to_string()));
    assert!(!ls_keys.contains(&"only_session".to_string()));

    assert!(ss_keys.contains(&"only_session".to_string()));
    assert!(!ss_keys.contains(&"only_local".to_string()));

    storage::clear(&ls).ok();
    storage::clear(&ss).ok();

    for (k, v) in original_ls {
        storage::set(&ls, &k, &v).ok();
    }
    for (k, v) in original_ss {
        storage::set(&ss, &k, &v).ok();
    }
}

#[wasm_bindgen_test]
fn test_local_and_session_entries_independent() {
    let ls = storage::local_storage().unwrap();
    let ss = storage::session_storage().unwrap();

    let original_ls: Vec<_> = storage::entries(&ls);
    let original_ss: Vec<_> = storage::entries(&ss);

    storage::clear(&ls).ok();
    storage::clear(&ss).ok();

    storage::set(&ls, "a", "1").unwrap();
    storage::set(&ls, "b", "2").unwrap();
    storage::set(&ss, "c", "3").unwrap();

    let ls_entries = storage::entries(&ls);
    let ss_entries = storage::entries(&ss);

    assert_eq!(ls_entries.len(), 2);
    assert_eq!(ss_entries.len(), 1);

    assert!(ls_entries.iter().any(|(k, _)| k == "a"));
    assert!(ls_entries.iter().any(|(k, _)| k == "b"));
    assert!(ss_entries.iter().any(|(k, _)| k == "c"));

    storage::clear(&ls).ok();
    storage::clear(&ss).ok();

    for (k, v) in original_ls {
        storage::set(&ls, &k, &v).ok();
    }
    for (k, v) in original_ss {
        storage::set(&ss, &k, &v).ok();
    }
}

// ============================================================
// Сериализация: числа
// ============================================================

#[wasm_bindgen_test]
fn test_i32_roundtrip() {
    let ls = storage::local_storage().unwrap();

    let original: i32 = 42;
    let serialized: String = original.to_string();
    storage::set(&ls, "test:i32", &serialized).unwrap();

    let retrieved = storage::get(&ls, "test:i32").unwrap();
    let parsed: i32 = retrieved.parse().unwrap();

    assert_eq!(parsed, original);
    assert_eq!(retrieved, "42");

    storage::remove(&ls, "test:i32").ok();
}

#[wasm_bindgen_test]
fn test_negative_i32_roundtrip() {
    let ls = storage::local_storage().unwrap();

    let original: i32 = -123;
    storage::set(&ls, "test:neg_i32", &original.to_string()).unwrap();

    let retrieved = storage::get(&ls, "test:neg_i32").unwrap();
    let parsed: i32 = retrieved.parse().unwrap();

    assert_eq!(parsed, original);
    assert_eq!(retrieved, "-123");

    storage::remove(&ls, "test:neg_i32").ok();
}

#[wasm_bindgen_test]
fn test_f64_roundtrip() {
    let ls = storage::local_storage().unwrap();

    let original: f64 = 3.14159;
    storage::set(&ls, "test:f64", &original.to_string()).unwrap();

    let retrieved = storage::get(&ls, "test:f64").unwrap();
    let parsed: f64 = retrieved.parse().unwrap();

    assert!((parsed - original).abs() < 1e-10);
    assert!(retrieved.starts_with("3.14"));

    storage::remove(&ls, "test:f64").ok();
}

// ============================================================
// Сериализация: булевы значения
// ============================================================

#[wasm_bindgen_test]
fn test_bool_true_roundtrip() {
    let ls = storage::local_storage().unwrap();

    let original: bool = true;
    storage::set(&ls, "test:bool_true", &original.to_string()).unwrap();

    let retrieved = storage::get(&ls, "test:bool_true").unwrap();
    let parsed: bool = retrieved.parse().unwrap();

    assert_eq!(parsed, original);
    assert_eq!(retrieved, "true");

    storage::remove(&ls, "test:bool_true").ok();
}

#[wasm_bindgen_test]
fn test_bool_false_roundtrip() {
    let ls = storage::local_storage().unwrap();

    let original: bool = false;
    storage::set(&ls, "test:bool_false", &original.to_string()).unwrap();

    let retrieved = storage::get(&ls, "test:bool_false").unwrap();
    let parsed: bool = retrieved.parse().unwrap();

    assert_eq!(parsed, original);
    assert_eq!(retrieved, "false");

    storage::remove(&ls, "test:bool_false").ok();
}

// ============================================================
// Ошибки парсинга
// ============================================================

#[wasm_bindgen_test]
fn test_parse_fails_on_invalid_input() {
    let ls = storage::local_storage().unwrap();

    // Кладём в хранилище строку, которая не является числом
    storage::set(&ls, "test:invalid_int", "hello").unwrap();

    let retrieved = storage::get(&ls, "test:invalid_int").unwrap();
    let parsed: Result<i32, _> = retrieved.parse::<i32>();

    assert!(parsed.is_err(), "parsing 'hello' as i32 should fail");

    storage::remove(&ls, "test:invalid_int").ok();
}

// ============================================================
// JSON: сериализация структур
// ============================================================

#[wasm_bindgen_test]
fn test_set_get_json_simple_struct() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct SimpleData {
        name: String,
        count: u32,
    }

    let ls = storage::local_storage().unwrap();

    let original = SimpleData {
        name: "Rust".into(),
        count: 42,
    };

    storage::set_json(&ls, "test:json_simple", &original).unwrap();

    let retrieved: SimpleData = storage::get_json(&ls, "test:json_simple").unwrap();
    assert_eq!(retrieved, original);

    storage::remove(&ls, "test:json_simple").ok();
}

#[wasm_bindgen_test]
fn test_set_get_json_vec() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Item {
        id: u32,
        label: String,
    }

    let ls = storage::local_storage().unwrap();

    let original = vec![
        Item { id: 1, label: "один".into() },
        Item { id: 2, label: "два".into() },
        Item { id: 3, label: "три".into() },
    ];

    storage::set_json(&ls, "test:json_vec", &original).unwrap();

    let retrieved: Vec<Item> = storage::get_json(&ls, "test:json_vec").unwrap();
    assert_eq!(retrieved, original);
    assert_eq!(retrieved.len(), 3);

    storage::remove(&ls, "test:json_vec").ok();
}

#[wasm_bindgen_test]
fn test_set_get_json_empty_vec() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Item {
        id: u32,
    }

    let ls = storage::local_storage().unwrap();

    let original: Vec<Item> = Vec::new();
    storage::set_json(&ls, "test:json_empty_vec", &original).unwrap();

    let retrieved: Vec<Item> = storage::get_json(&ls, "test:json_empty_vec").unwrap();
    assert!(retrieved.is_empty());

    storage::remove(&ls, "test:json_empty_vec").ok();
}

#[wasm_bindgen_test]
fn test_set_get_json_option_none() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct WithOption {
        name: String,
        description: Option<String>,
    }

    let ls = storage::local_storage().unwrap();

    let original = WithOption {
        name: "item".into(),
        description: None,
    };

    storage::set_json(&ls, "test:json_opt_none", &original).unwrap();

    let retrieved: WithOption = storage::get_json(&ls, "test:json_opt_none").unwrap();
    assert_eq!(retrieved, original);
    assert!(retrieved.description.is_none());

    storage::remove(&ls, "test:json_opt_none").ok();
}

#[wasm_bindgen_test]
fn test_set_get_json_option_some() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct WithOption {
        name: String,
        description: Option<String>,
    }

    let ls = storage::local_storage().unwrap();

    let original = WithOption {
        name: "item".into(),
        description: Some("описание".into()),
    };

    storage::set_json(&ls, "test:json_opt_some", &original).unwrap();

    let retrieved: WithOption = storage::get_json(&ls, "test:json_opt_some").unwrap();
    assert_eq!(retrieved, original);
    assert_eq!(retrieved.description, Some("описание".into()));

    storage::remove(&ls, "test:json_opt_some").ok();
}

#[wasm_bindgen_test]
fn test_set_get_json_nested_struct() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Inner {
        value: i32,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    let ls = storage::local_storage().unwrap();

    let original = Outer {
        name: "outer".into(),
        inner: Inner { value: 42 },
    };

    storage::set_json(&ls, "test:json_nested", &original).unwrap();

    let retrieved: Outer = storage::get_json(&ls, "test:json_nested").unwrap();
    assert_eq!(retrieved, original);
    assert_eq!(retrieved.inner.value, 42);

    storage::remove(&ls, "test:json_nested").ok();
}

#[wasm_bindgen_test]
fn test_get_json_missing_key() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct Unused {
        #[allow(dead_code)]
        field: String,
    }

    let ls = storage::local_storage().unwrap();
    storage::remove(&ls, "test:json_missing").ok();

    let result: Option<Unused> = storage::get_json(&ls, "test:json_missing");
    assert!(result.is_none());
}

#[wasm_bindgen_test]
fn test_get_json_invalid_syntax() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct Unused {
        #[allow(dead_code)]
        field: String,
    }

    let ls = storage::local_storage().unwrap();

    // Не валидный JSON вообще
    storage::set(&ls, "test:json_invalid", "not json at all").unwrap();

    let result: Option<Unused> = storage::get_json(&ls, "test:json_invalid");
    assert!(result.is_none(), "invalid JSON syntax should return None");

    storage::remove(&ls, "test:json_invalid").ok();
}

#[wasm_bindgen_test]
fn test_get_json_wrong_schema() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct Expected {
        #[allow(dead_code)]
        required_field: String,
    }

    let ls = storage::local_storage().unwrap();

    // Валидный JSON, но не подходит под схему
    storage::set(&ls, "test:json_schema", r#"{"other_field": 42}"#).unwrap();

    let result: Option<Expected> = storage::get_json(&ls, "test:json_schema");
    assert!(result.is_none(), "wrong schema should return None");

    storage::remove(&ls, "test:json_schema").ok();
}

#[wasm_bindgen_test]
fn test_json_roundtrip_preserves_unicode() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TextHolder {
        text: String,
    }

    let ls = storage::local_storage().unwrap();

    let original = TextHolder {
        text: "Привет, мир! 🦀 Rust + JSON = ❤".into(),
    };

    storage::set_json(&ls, "test:json_unicode", &original).unwrap();

    let retrieved: TextHolder = storage::get_json(&ls, "test:json_unicode").unwrap();
    assert_eq!(retrieved, original);
    assert!(retrieved.text.contains("🦀"));

    storage::remove(&ls, "test:json_unicode").ok();
}

#[wasm_bindgen_test]
fn test_json_stored_as_string_in_storage() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Data {
        value: i32,
    }

    let ls = storage::local_storage().unwrap();

    let original = Data { value: 42 };
    storage::set_json(&ls, "test:json_raw", &original).unwrap();

    // Читаем сырую строку — она должна быть валидным JSON,
    // а не Rust-представлением структуры
    let raw = storage::get(&ls, "test:json_raw").unwrap();
    assert!(raw.starts_with('{'));
    assert!(raw.contains("\"value\""));
    assert!(raw.contains("42"));

    storage::remove(&ls, "test:json_raw").ok();
}

// ============================================================
// StorageEvent: парсинг
// ============================================================

#[wasm_bindgen_test]
fn test_parse_storage_event_default() {
    let event = web_sys::StorageEvent::new("storage").unwrap();
    let change = storage::parse_storage_event(&event);

    // Синтетическое событие без init-данных
    assert!(change.key.is_none());
    assert!(change.old_value.is_none());
    assert!(change.new_value.is_none());
}

#[wasm_bindgen_test]
fn test_parse_storage_event_sets_area() {
    let event = web_sys::StorageEvent::new("storage").unwrap();
    let change = storage::parse_storage_event(&event);

    // storage_area всегда "localStorage" — событие генерируется
    // только для localStorage
    assert_eq!(change.storage_area, Some("localStorage".to_string()));
}

// ============================================================
// StorageEvent: форматирование
// ============================================================

#[wasm_bindgen_test]
fn test_format_storage_change_set() {
    let change = storage::StorageChange {
        key: Some("theme".into()),
        old_value: Some("light".into()),
        new_value: Some("dark".into()),
        url: None,
        storage_area: None,
    };
    let text = storage::format_storage_change(&change);

    assert!(text.contains("theme"));
    assert!(text.contains("dark"));
    assert!(text.contains("was: \"light\""));
}

#[wasm_bindgen_test]
fn test_format_storage_change_set_without_old() {
    let change = storage::StorageChange {
        key: Some("new_key".into()),
        old_value: None,
        new_value: Some("value".into()),
        url: None,
        storage_area: None,
    };
    let text = storage::format_storage_change(&change);

    assert!(text.contains("new_key"));
    assert!(text.contains("value"));
    // Для нового ключа old_value отсутствует — не должно быть "was:"
    assert!(!text.contains("was:"));
}

#[wasm_bindgen_test]
fn test_format_storage_change_remove() {
    let change = storage::StorageChange {
        key: Some("theme".into()),
        old_value: Some("dark".into()),
        new_value: None,
        url: None,
        storage_area: None,
    };
    let text = storage::format_storage_change(&change);

    assert!(text.contains("removed"));
    assert!(text.contains("theme"));
    // При remove показываем "removed", а не "was:"
    assert!(!text.contains("was:"));
}

#[wasm_bindgen_test]
fn test_format_storage_change_clear() {
    let change = storage::StorageChange {
        key: None,
        old_value: None,
        new_value: None,
        url: None,
        storage_area: None,
    };
    let text = storage::format_storage_change(&change);

    assert!(text.contains("cleared"));
}

#[wasm_bindgen_test]
fn test_format_storage_change_with_area_and_url() {
    let change = storage::StorageChange {
        key: Some("theme".into()),
        old_value: Some("light".into()),
        new_value: Some("dark".into()),
        url: Some("http://localhost:5173/".into()),
        storage_area: Some("localStorage".into()),
    };
    let text = storage::format_storage_change(&change);

    assert!(text.contains("theme"));
    assert!(text.contains("dark"));
    assert!(text.contains("area: localStorage"));
    assert!(text.contains("from: http://localhost:5173/"));
    assert!(text.contains("was: \"light\""));
}

#[wasm_bindgen_test]
fn test_format_storage_change_clear_with_area_and_url() {
    let change = storage::StorageChange {
        key: None,
        old_value: None,
        new_value: None,
        url: Some("http://example.com/".into()),
        storage_area: Some("localStorage".into()),
    };
    let text = storage::format_storage_change(&change);

    assert!(text.contains("cleared"));
    assert!(text.contains("area: localStorage"));
    assert!(text.contains("from: http://example.com/"));
    // При clear нет старого значения — не должно быть "was:"
    assert!(!text.contains("was:"));
}

// ============================================================
// StorageChange: конструктор по умолчанию
// ============================================================

#[wasm_bindgen_test]
fn test_storage_change_default() {
    let change = storage::StorageChange::default();

    assert!(change.key.is_none());
    assert!(change.old_value.is_none());
    assert!(change.new_value.is_none());
    assert!(change.url.is_none());
    assert!(change.storage_area.is_none());
}

// ============================================================
// current_origin
// ============================================================

#[wasm_bindgen_test]
fn test_current_origin_is_valid() {
    let origin = storage::current_origin();

    assert!(!origin.is_empty(), "origin should not be empty");
    assert!(
        origin.starts_with("http"),
        "origin should start with http: {}",
        origin
    );
    assert_ne!(origin, "unknown");
}

#[wasm_bindgen_test]
fn test_current_origin_contains_port() {
    let origin = storage::current_origin();
    // В тестовом окружении (127.0.0.1:порт) origin содержит порт
    assert!(
        origin.contains(':'),
        "origin should contain port separator: {}",
        origin
    );
}

// ============================================================
// clear_prefix
// ============================================================

#[wasm_bindgen_test]
fn test_clear_prefix_removes_matching() {
    let ls = storage::local_storage().unwrap();

    storage::set(&ls, "test_prefix:a", "1").unwrap();
    storage::set(&ls, "test_prefix:b", "2").unwrap();
    storage::set(&ls, "other_key", "3").unwrap();

    let removed = storage::clear_prefix(&ls, "test_prefix:");
    assert_eq!(removed, 2);

    assert!(!storage::has_key(&ls, "test_prefix:a"));
    assert!(!storage::has_key(&ls, "test_prefix:b"));
    assert!(storage::has_key(&ls, "other_key"));

    storage::remove(&ls, "other_key").ok();
}

#[wasm_bindgen_test]
fn test_clear_prefix_no_matches() {
    let ls = storage::local_storage().unwrap();

    let removed = storage::clear_prefix(&ls, "definitely_no_such_prefix_xyz:");
    assert_eq!(removed, 0);
}

#[wasm_bindgen_test]
fn test_clear_prefix_empty_storage() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    let removed = storage::clear_prefix(&ls, "anything:");
    assert_eq!(removed, 0);

    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}

// ============================================================
// fill_to_quota
// ============================================================

#[wasm_bindgen_test]
fn test_fill_to_quota_respects_max_keys() {
    let ls = storage::local_storage().unwrap();

    // Маленький лимит: 5 ключей по 1000 символов
    let result = storage::fill_to_quota(&ls, "test_quota_small", 1000, 5);

    assert_eq!(result.keys_written, 5);
    assert!(!result.hit_quota);
    assert!(result.total_chars >= 5 * 1000);
    assert!(result.error_message.is_some());

    storage::clear_prefix(&ls, "test_quota_small");
}

#[wasm_bindgen_test]
fn test_fill_to_quota_max_zero() {
    let ls = storage::local_storage().unwrap();

    // max_keys = 0 — цикл не должен выполниться ни разу
    let result = storage::fill_to_quota(&ls, "test_quota_zero", 1000, 0);

    assert_eq!(result.keys_written, 0);
    assert!(!result.hit_quota);
    assert_eq!(result.total_chars, 0);
}

#[wasm_bindgen_test]
fn test_fill_to_quota_hits_real_quota() {
    let ls = storage::local_storage().unwrap();

    // Записываем блоками по 1 МБ. Реальная квота сработает
    // задолго до 200 ключей (200 МБ — намного больше 5–10 МБ лимита).
    let result = storage::fill_to_quota(&ls, "test_quota_real", 1_000_000, 200);

    // Либо упёрлись в реальную квоту, либо прошли все 200 ключей
    // (маловероятно, но возможно в браузере с большим лимитом)
    assert!(
        result.hit_quota || result.keys_written == 200,
        "should hit quota or reach max, got keys={} hit_quota={}",
        result.keys_written,
        result.hit_quota
    );

    assert!(result.keys_written > 0);

    storage::clear_prefix(&ls, "test_quota_real");
}

// ============================================================
// measure_sync_block
// ============================================================

#[wasm_bindgen_test]
fn test_measure_sync_block_writes_data() {
    let ls = storage::local_storage().unwrap();

    storage::clear_prefix(&ls, "test_sync");

    // Записываем 100 КБ — маленький объём, чтобы тест был быстрым
    let result = storage::measure_sync_block(&ls, 100, "test_sync");

    assert_eq!(result.keys_written, 100);
    assert!(result.bytes_written >= 100 * 1024);
    assert!(result.duration_ms >= 0.0);

    let removed = storage::clear_prefix(&ls, "test_sync");
    assert_eq!(removed, 100);
}

#[wasm_bindgen_test]
fn test_measure_sync_block_zero_size() {
    let ls = storage::local_storage().unwrap();

    let result = storage::measure_sync_block(&ls, 0, "test_sync_zero");

    assert_eq!(result.keys_written, 0);
    assert_eq!(result.bytes_written, 0);
    // Даже нулевая операция занимает какое-то время (0 или больше)
    assert!(result.duration_ms >= 0.0);
}

// ============================================================
// Демонстрация не-атомарности
// ============================================================

#[wasm_bindgen_test]
fn test_operations_are_not_atomic() {
    let ls = storage::local_storage().unwrap();

    let original: Vec<_> = storage::entries(&ls);
    storage::clear(&ls).ok();

    // Имитируем "транзакцию" из двух записей
    storage::set(&ls, "tx:a", "1").unwrap();

    // Между двумя set можно прочитать промежуточное состояние
    assert!(storage::has_key(&ls, "tx:a"));
    assert!(!storage::has_key(&ls, "tx:b"));

    storage::set(&ls, "tx:b", "2").unwrap();

    // Теперь видны обе
    assert!(storage::has_key(&ls, "tx:a"));
    assert!(storage::has_key(&ls, "tx:b"));

    storage::clear(&ls).ok();
    for (k, v) in original {
        storage::set(&ls, &k, &v).ok();
    }
}