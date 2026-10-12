 // use wasm-pack test --chrome --test manual_choose

use hello_wasm02::fsa_choose_exec::PickOutcome;
use hello_wasm02::fsa_choose_fallback;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
#[ignore]
async fn manual_pick_via_input() {
    let out = fsa_choose_fallback::pick_via_input("text/plain").await;
    match out {
        PickOutcome::Picked(file) => {
            web_sys::console::log_1(
                &format!("Выбран файл: {}", file.name()).into(),
            );
        }
        other => {
            web_sys::console::log_1(
                &format!("Результат: {}", other.kind_label()).into(),
            );
        }
    }
}