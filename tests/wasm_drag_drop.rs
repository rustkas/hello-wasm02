use wasm_bindgen_test::*;
use hello_wasm02::drag_drop::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_prevent_default_cancelable_true() {
    // Событие с cancelable = true — prevent_default выставляет флаг.
    let init = web_sys::DragEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);

    let event = web_sys::DragEvent::new_with_event_init_dict("drop", &init)
        .expect("DragEvent::new_with_event_init_dict");

    prevent_default(&event);
    assert!(event.default_prevented());
}

#[wasm_bindgen_test]
fn test_prevent_default_cancelable_false() {
    // Событие с cancelable = false — prevent_default не выставляет флаг,
    // потому что спецификация DOM запрещает отмену таких событий.
    let init = web_sys::DragEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(false);

    let event = web_sys::DragEvent::new_with_event_init_dict("drop", &init)
        .expect("DragEvent::new_with_event_init_dict");

    prevent_default(&event);
    assert!(!event.default_prevented());
}

#[wasm_bindgen_test]
fn test_files_from_drop_event_no_data_transfer() {
    // Событие без DataTransfer — ожидаем None.
    let init = web_sys::DragEventInit::new();
    init.set_cancelable(true);

    let event = web_sys::DragEvent::new_with_event_init_dict("drop", &init)
        .expect("DragEvent::new_with_event_init_dict");

    assert!(files_from_drop_event(&event).is_none());
}

#[wasm_bindgen_test]
fn test_metadata_from_drop_event_no_data_transfer() {
    // Событие без DataTransfer — ожидаем пустой вектор.
    let init = web_sys::DragEventInit::new();
    init.set_cancelable(true);

    let event = web_sys::DragEvent::new_with_event_init_dict("drop", &init)
        .expect("DragEvent::new_with_event_init_dict");

    assert!(metadata_from_drop_event(&event).is_empty());
}

#[wasm_bindgen_test]
fn test_has_files_no_data_transfer() {
    // Событие без DataTransfer — has_files возвращает false.
    let init = web_sys::DragEventInit::new();
    init.set_cancelable(true);

    let event = web_sys::DragEvent::new_with_event_init_dict("drop", &init)
        .expect("DragEvent::new_with_event_init_dict");

    assert!(!has_files(&event));
}

#[wasm_bindgen_test]
fn test_drop_event_type() {
    // Проверяем базовые свойства события: тип и cancelable.
    let init = web_sys::DragEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);

    let event = web_sys::DragEvent::new_with_event_init_dict("drop", &init)
        .expect("DragEvent::new_with_event_init_dict");

    assert_eq!(event.type_(), "drop");
    assert!(event.cancelable());
    assert!(event.bubbles());
}