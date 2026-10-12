//! Wasm-тесты `notif_sw`: доступность, регистрация, showNotification.
//!
//! Регистрация Service Worker в headless-тесте требует, чтобы `sw.js`
//! был доступен по URL. Тесты, требующие реальной регистрации,
//! помечены `#[ignore]` — их надо запускать вручную после `npm start`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::notif_sw;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn service_worker_api_supported() {
    // В headless Chrome на localhost Service Worker API есть.
    assert!(notif_sw::is_supported());
}

#[wasm_bindgen_test]
#[ignore = "требует загруженный sw.js — тест запускается после npm start"]
async fn get_registration_initially_none() {
    let reg = notif_sw::get_registration().await.expect("get_registration");
    assert!(reg.is_none(), "регистрация не должна существовать до register");
}

#[wasm_bindgen_test]
#[ignore = "требует загруженный sw.js — тест запускается после npm start"]
async fn register_and_get() {
    let reg = notif_sw::register("/sw.js").await.expect("register");
    assert!(reg.is_object());
    let reg2 = notif_sw::get_registration().await.expect("get_registration");
    assert!(reg2.is_some());
}