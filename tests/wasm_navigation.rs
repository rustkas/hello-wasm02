//! Тесты для навигации и URL.
use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use hello_wasm02::navigation;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// Location
// ============================================================

#[wasm_bindgen_test]
fn test_location_href_is_valid() {
    let href = navigation::current_href();
    assert!(!href.is_empty(), "href should not be empty");
    assert!(
        href.starts_with("http"),
        "href should start with http: {}",
        href
    );
}

#[wasm_bindgen_test]
fn test_location_pathname_starts_with_slash() {
    let pathname = navigation::current_pathname();
    assert!(!pathname.is_empty(), "pathname should not be empty");
    assert!(
        pathname.starts_with('/'),
        "pathname should start with /: {}",
        pathname
    );
}

#[wasm_bindgen_test]
fn test_location_search_is_valid_string() {
    let search = navigation::current_search();
    // Search пустой (нет query) или начинается с "?"
    assert!(
        search.is_empty() || search.starts_with('?'),
        "search should be empty or start with ?: {}",
        search
    );
}

#[wasm_bindgen_test]
fn test_location_hash_is_valid_string() {
    let hash = navigation::current_hash();
    // Hash пустой (нет hash) или начинается с "#"
    assert!(
        hash.is_empty() || hash.starts_with('#'),
        "hash should be empty or start with #: {}",
        hash
    );
}

#[wasm_bindgen_test]
fn test_location_components_consistency() {
    let href = navigation::current_href();
    let pathname = navigation::current_pathname();
    let search = navigation::current_search();
    let hash = navigation::current_hash();

    assert!(
        href.contains(&pathname),
        "href should contain pathname: {} vs {}",
        href,
        pathname
    );

    if !search.is_empty() {
        assert!(
            href.contains(&search),
            "href should contain search: {} vs {}",
            href,
            search
        );
    }

    if !hash.is_empty() {
        assert!(
            href.contains(&hash),
            "href should contain hash: {} vs {}",
            href,
            hash
        );
    }
}

#[wasm_bindgen_test]
fn test_location_set_hash() {
    let original_url = navigation::current_href();

    navigation::set_hash("#test-location-hash").unwrap();
    assert_eq!(
        navigation::current_hash(),
        "#test-location-hash",
        "hash should be set"
    );

    let href = navigation::current_href();
    assert!(
        href.ends_with("#test-location-hash"),
        "href should end with hash: {}",
        href
    );

    // Восстанавливаем оригинальный URL
    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

#[wasm_bindgen_test]
fn test_location_set_hash_empty() {
    let original_url = navigation::current_href();

    navigation::set_hash("#something").unwrap();
    assert_eq!(navigation::current_hash(), "#something");

    navigation::set_hash("").unwrap();
    assert_eq!(
        navigation::current_hash(),
        "",
        "hash should be empty after clearing"
    );

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

#[wasm_bindgen_test]
fn test_location_replace_state_changes_url() {
    let original_url = navigation::current_href();

    navigation::replace_state(&JsValue::NULL, "", Some("/new-location-path"))
        .unwrap();
    assert_eq!(navigation::current_pathname(), "/new-location-path");

    navigation::replace_state(&JsValue::NULL, "", Some("/another-path"))
        .unwrap();
    assert_eq!(navigation::current_pathname(), "/another-path");

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

#[wasm_bindgen_test]
fn test_location_all_components_via_replace_state() {
    let original_url = navigation::current_href();

    navigation::replace_state(
        &JsValue::NULL,
        "",
        Some("/test-location-path?foo=bar&baz=qux#myhash"),
    )
    .unwrap();

    assert_eq!(navigation::current_pathname(), "/test-location-path");
    assert_eq!(navigation::current_search(), "?foo=bar&baz=qux");
    assert_eq!(navigation::current_hash(), "#myhash");

    let href = navigation::current_href();
    assert!(href.contains("/test-location-path"));
    assert!(href.contains("?foo=bar&baz=qux"));
    assert!(href.contains("#myhash"));

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

#[wasm_bindgen_test]
fn test_location_search_with_empty_query() {
    let original_url = navigation::current_href();

    navigation::replace_state(&JsValue::NULL, "", Some("/path-without-query"))
        .unwrap();
    assert_eq!(
        navigation::current_search(),
        "",
        "search should be empty"
    );

    navigation::replace_state(&JsValue::NULL, "", Some("/with?x=1")).unwrap();
    assert_eq!(navigation::current_search(), "?x=1");

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

#[wasm_bindgen_test]
fn test_location_hash_with_empty_hash() {
    let original_url = navigation::current_href();

    navigation::replace_state(&JsValue::NULL, "", Some("/path")).unwrap();
    assert_eq!(navigation::current_hash(), "", "hash should be empty");

    navigation::replace_state(&JsValue::NULL, "", Some("/path#h")).unwrap();
    assert_eq!(navigation::current_hash(), "#h");

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

// ============================================================
// Url
// ============================================================

#[wasm_bindgen_test]
fn test_url_parse() {
    let url = navigation::parse_url("https://example.com/path?q=1#hash").unwrap();

    assert_eq!(navigation::url_origin(&url), "https://example.com");
    assert_eq!(navigation::url_pathname(&url), "/path");
    assert_eq!(navigation::url_search(&url), "?q=1");
    assert_eq!(navigation::url_hash(&url), "#hash");
}

#[wasm_bindgen_test]
fn test_url_relative() {
    let url = navigation::parse_url("https://example.com").unwrap();
    assert_eq!(navigation::url_pathname(&url), "/");
}

#[wasm_bindgen_test]
fn test_url_with_port() {
    let url = navigation::parse_url("https://example.com:8080/api").unwrap();
    assert_eq!(navigation::url_origin(&url), "https://example.com:8080");
    assert_eq!(navigation::url_pathname(&url), "/api");
}

// ============================================================
// UrlSearchParams
// ============================================================

#[wasm_bindgen_test]
fn test_search_params_from_str() {
    let params = navigation::search_params_from_str("a=1&b=2").unwrap();

    assert_eq!(navigation::param_get(&params, "a"), Some("1".into()));
    assert_eq!(navigation::param_get(&params, "b"), Some("2".into()));
    assert_eq!(navigation::param_get(&params, "c"), None);
}

#[wasm_bindgen_test]
fn test_search_params_set_append_delete() {
    let params = navigation::new_search_params().unwrap();

    navigation::param_set(&params, "a", "1");
    assert_eq!(navigation::param_get(&params, "a"), Some("1".into()));

    navigation::param_append(&params, "a", "2");
    assert_eq!(navigation::param_get(&params, "a"), Some("1".into()));

    navigation::param_delete(&params, "a");
    assert_eq!(navigation::param_get(&params, "a"), None);
}

#[wasm_bindgen_test]
fn test_search_params_has() {
    let params = navigation::search_params_from_str("x=1").unwrap();

    assert!(navigation::param_has(&params, "x"));
    assert!(!navigation::param_has(&params, "y"));
}

#[wasm_bindgen_test]
fn test_search_params_to_string() {
    let params = navigation::new_search_params().unwrap();
    navigation::param_set(&params, "a", "1");
    navigation::param_set(&params, "b", "2");

    let s = navigation::params_to_string(&params);
    assert!(s.contains("a=1"));
    assert!(s.contains("b=2"));
}

#[wasm_bindgen_test]
fn test_parse_url_with_query_params() {
    let url = navigation::parse_url(
        "https://example.com:8080/path?name=Rust&version=1.75#section",
    )
    .unwrap();

    assert_eq!(navigation::url_origin(&url), "https://example.com:8080");
    assert_eq!(navigation::url_pathname(&url), "/path");

    let params = navigation::url_search_params(&url);
    assert_eq!(navigation::param_get(&params, "name"), Some("Rust".into()));
    assert_eq!(
        navigation::param_get(&params, "version"),
        Some("1.75".into())
    );
    assert!(!navigation::param_has(&params, "unknown"));
}

// ============================================================
// History
// ============================================================

#[wasm_bindgen_test]
fn test_history_length() {
    let len = navigation::history_length();
    assert!(len >= 1);
}

#[wasm_bindgen_test]
fn test_push_and_replace_state() {
    let original_url = navigation::current_href();

    navigation::push_state(&JsValue::NULL, "", Some("/test-page")).unwrap();
    assert_eq!(navigation::current_pathname(), "/test-page");

    navigation::replace_state(&JsValue::NULL, "", Some("/test-replaced")).unwrap();
    assert_eq!(navigation::current_pathname(), "/test-replaced");

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

// ============================================================
// Hash
// ============================================================

#[wasm_bindgen_test]
fn test_set_hash_and_read() {
    let original_url = navigation::current_href();

    navigation::set_hash("#section-a").unwrap();
    assert_eq!(navigation::current_hash(), "#section-a");

    navigation::set_hash("#section-b").unwrap();
    assert_eq!(navigation::current_hash(), "#section-b");

    navigation::set_hash("").unwrap();
    assert_eq!(navigation::current_hash(), "");

    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

#[wasm_bindgen_test]
fn test_hash_change_event() {
    let original_url = navigation::current_href();

    let fired = Rc::new(RefCell::new(false));
    let fired_clone = fired.clone();

    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        *fired_clone.borrow_mut() = true;
    });

    let window = web_sys::window().unwrap();
    window
        .add_event_listener_with_callback("hashchange", closure.as_ref().unchecked_ref())
        .unwrap();

    let event = web_sys::Event::new("hashchange").unwrap();
    window.dispatch_event(&event).unwrap();

    assert!(*fired.borrow(), "hashchange handler was not called");

    closure.forget();
    navigation::replace_state(&JsValue::NULL, "", Some(&original_url)).ok();
}

// ============================================================
// Navigation events
// ============================================================

#[wasm_bindgen_test]
fn test_popstate_event_dispatch() {
    let fired = Rc::new(RefCell::new(false));
    let fired_clone = fired.clone();

    let closure = Closure::<dyn FnMut(web_sys::PopStateEvent)>::new(move |_event| {
        *fired_clone.borrow_mut() = true;
    });

    let window = web_sys::window().unwrap();
    window
        .add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref())
        .unwrap();

    let event = web_sys::PopStateEvent::new("popstate").unwrap();
    window.dispatch_event(&event).unwrap();

    assert!(*fired.borrow(), "popstate handler was not called");

    closure.forget();
}

// ============================================================
// Navigation API (доступность)
// ============================================================

#[wasm_bindgen_test]
fn test_navigation_api_availability_check() {
    // Не падает — возвращает bool.
    // В Chrome может быть true, в Firefox — false.
    let _available = navigation::is_navigation_api_available();
}