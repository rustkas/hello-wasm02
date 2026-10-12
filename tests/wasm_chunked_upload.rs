use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen_test::*;
use hello_wasm02::chunked_upload::*;

wasm_bindgen_test_configure!(run_in_browser);

const CHUNK_URL: &str = "http://localhost:3000/api/upload/chunk";

// ===============================================================
// Чистые функции (без сервера)
// ===============================================================

#[wasm_bindgen_test]
fn test_chunk_count_zero_size() {
    assert_eq!(chunk_count(0.0, 1024), 0);
}

#[wasm_bindgen_test]
fn test_chunk_count_zero_chunk_size() {
    assert_eq!(chunk_count(100.0, 0), 0);
}

#[wasm_bindgen_test]
fn test_chunk_count_exact() {
    assert_eq!(chunk_count(1024.0, 1024), 1);
    assert_eq!(chunk_count(8192.0, 1024), 8);
}

#[wasm_bindgen_test]
fn test_chunk_count_with_remainder() {
    // 32 001 / 8192 = 3.906... → ceil = 4 чанка.
    assert_eq!(chunk_count(32_001.0, 8 * 1024), 4);
    assert_eq!(chunk_count(1025.0, 1024), 2);
}

// ===============================================================
// Интеграционные (требуют auth-server)
// ===============================================================

#[wasm_bindgen_test]
async fn test_chunked_upload_then_delete() {
    let bytes: Vec<u8> = (0..32_000).map(|i| (i % 256) as u8).collect();
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "test.bin")
        .expect("File::new_with_u8_array_sequence");

    let last_done = Rc::new(Cell::new(0u32));
    let last_total = Rc::new(Cell::new(0u32));
    let last_done_c = last_done.clone();
    let last_total_c = last_total.clone();
    let on_progress = move |done: u32, total: u32| {
        last_done_c.set(done);
        last_total_c.set(total);
    };

    let cancel = Rc::new(Cell::new(false));
    let result = upload_chunked(&file, CHUNK_URL, 8 * 1024, cancel, on_progress)
        .await
        .expect("upload failed");

    assert_eq!(last_total.get(), 4);
    assert_eq!(last_done.get(), 4);
    assert!(result.upload_id.starts_with("upload-"));
    assert!(result.response.contains("\"status\":\"ok\""));

    // Статус — файл на месте.
    let status = get_chunked_upload_status(CHUNK_URL, &result.upload_id)
        .await
        .expect("status failed");
    assert!(status.contains(&result.upload_id));

    // Удаляем.
    let deleted = delete_chunked_upload(CHUNK_URL, &result.upload_id)
        .await
        .expect("delete failed");
    assert!(deleted.contains("\"removed\":true"));

    // Повторное удаление — идемпотентно.
    let again = delete_chunked_upload(CHUNK_URL, &result.upload_id)
        .await
        .expect("second delete failed");
    assert!(again.contains("\"removed\":false"));
}

#[wasm_bindgen_test]
async fn test_chunked_upload_cancelled_before_start() {
    let bytes: Vec<u8> = (0..32_000).map(|i| (i % 256) as u8).collect();
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "test.bin")
        .expect("File::new_with_u8_array_sequence");

    let cancel = Rc::new(Cell::new(true));
    let result = upload_chunked(&file, CHUNK_URL, 8 * 1024, cancel, |_, _| {})
        .await;

    assert!(result.is_err(), "ожидалась ошибка отмены");
    let err = result.unwrap_err();
    assert_eq!(err.as_string().as_deref(), Some("aborted"));
}

// ===============================================================
// НОВЫЕ тесты: список загрузок и полный жизненный цикл
// ===============================================================

/// Проверяет, что список загрузок возвращается и парсится без ошибок.
///
/// Не требует, чтобы список был пуст — тесты могут идти в любом
/// порядке, предыдущие могли оставить файлы. Главное — функция
/// **не падает** и возвращает `Vec<UploadInfo>`.
#[wasm_bindgen_test]
async fn test_list_uploads_returns_vec() {
    let items = list_chunked_uploads(CHUNK_URL)
        .await
        .expect("list_chunked_uploads failed");

    // Проверяем инварианты всех элементов.
    for item in &items {
        assert!(
            item.upload_id.starts_with("upload-"),
            "upload_id: {}",
            item.upload_id,
        );
        assert!(!item.filename.is_empty(), "пустое имя файла");
        assert!(item.size > 0.0, "нулевой размер файла");
    }
}

/// Полный жизненный цикл: загрузка → появление в списке →
/// удаление → исчезновение из списка.
#[wasm_bindgen_test]
async fn test_full_lifecycle_upload_list_delete() {
    // Уникальное имя — избегаем коллизий с параллельными тестами
    // и предыдущими прогонами.
    let unique = format!("lifecycle-{}.bin", js_sys::Date::now() as u64);

    let bytes: Vec<u8> = vec![0xAB; 8_000];
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let file =
        web_sys::File::new_with_u8_array_sequence(&parts, &unique)
            .expect("File::new_with_u8_array_sequence");

    let cancel = Rc::new(Cell::new(false));
    let result = upload_chunked(&file, CHUNK_URL, 4 * 1024, cancel, |_, _| {})
        .await
        .expect("upload failed");
    let upload_id = result.upload_id.clone();

    // 1. Файл должен появиться в списке.
    let items_before = list_chunked_uploads(CHUNK_URL)
        .await
        .expect("list #1 failed");
    let found = items_before.iter().any(|it| {
        it.upload_id == upload_id && it.filename == unique
    });
    assert!(
        found,
        "файл {unique} не найден в списке после загрузки",
    );

    // Проверяем, что размер в списке совпадает с загруженным.
    let item = items_before
        .iter()
        .find(|it| it.upload_id == upload_id)
        .expect("не нашли по upload_id");
    assert_eq!(item.size, 8_000.0);

    // 2. Удаляем.
    let deleted = delete_chunked_upload(CHUNK_URL, &upload_id)
        .await
        .expect("delete failed");
    assert!(deleted.contains("\"removed\":true"));

    // 3. Файл должен исчезнуть из списка.
    let items_after = list_chunked_uploads(CHUNK_URL)
        .await
        .expect("list #2 failed");
    let still_there = items_after
        .iter()
        .any(|it| it.upload_id == upload_id);
    assert!(
        !still_there,
        "файл {unique} не должен присутствовать в списке после удаления",
    );
}

/// Имя файла со спецсимволами: пробелы, юникод, дефисы, точки.
///
/// Проверяем, что URL-энкодинг на клиенте и `read_dir` на сервере
/// корректно передают и читают имена с не-ASCII.
#[wasm_bindgen_test]
async fn test_upload_with_special_chars_in_name() {
    // Юникод + пробел + дефис + точка.
    let filename = "тест файл-1.bin";

    let bytes: Vec<u8> = vec![1, 2, 3, 4, 5];
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let file =
        web_sys::File::new_with_u8_array_sequence(&parts, filename)
            .expect("File::new_with_u8_array_sequence");

    let cancel = Rc::new(Cell::new(false));
    let result = upload_chunked(&file, CHUNK_URL, 4 * 1024, cancel, |_, _| {})
        .await
        .expect("upload failed");

    // Имя в списке должно вернуться точно таким же.
    let items = list_chunked_uploads(CHUNK_URL)
        .await
        .expect("list failed");
    let found = items.iter().any(|it| {
        it.upload_id == result.upload_id && it.filename == filename
    });
    assert!(
        found,
        "файл с именем «{filename}» не найден в списке",
    );

    // Убираем за собой.
    let _ = delete_chunked_upload(CHUNK_URL, &result.upload_id).await;
}