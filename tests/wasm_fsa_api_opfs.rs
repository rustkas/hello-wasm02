//! Интеграционные тесты `src/fsa_api_opfs.rs` через OPFS.
//!
//! OPFS не требует user gesture — тесты идут в headless Chrome
//! без пропусков. Если OPFS недоступен или `createSyncAccessHandle`
//! отсутствует в движке — тесты мягко пропускаются (`return`).
//!
//! **Важно.** `createSyncAccessHandle` определён только внутри
//! Dedicated Worker. Проверять его в main thread бесполезно —
//! всегда `false`. Поэтому доступность проверяется через
//! `sync_access_available_in_worker()` — она создаёт пробный
//! Worker и спрашивает у него.

use wasm_bindgen_test::*;

use hello_wasm02::fsa_api_opfs::{
    file_size, list_files, opfs_available, opfs_root, read_tail, read_text,
    remove_file, start_logger_worker, sync_access_available_in_worker, 
    write_sync_in_worker, write_text,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Базовые проверки доступности
// ---------------------------------------------------------------------------

/// OPFS-корень возвращает пустое имя — это норма, у корня нет
/// имени. Проверяем работоспособность: создать файл, прочитать,
/// удалить. Если это работает — OPFS доступен по-настоящему.
#[wasm_bindgen_test]
async fn test_opfs_available() {
    if !opfs_available() {
        return; // мягкий skip для старых браузеров
    }
    let root = opfs_root().await.expect("OPFS root");

    let name = "opfs_test_probe.txt";
    let _ = remove_file(&root, name).await;

    write_text(&root, name, "probe").await.expect("write probe");
    let back = read_text(&root, name).await.expect("read probe");
    assert_eq!(back, "probe", "readback matches write");

    remove_file(&root, name).await.expect("cleanup probe");
}

/// Проверка `createSyncAccessHandle` через пробный Worker.
/// В headless Chrome метод доступен из Worker'а, поэтому флаг
/// обычно `true`. В старых движках — `false`, тесты с ним
/// мягко пропускаются.
#[wasm_bindgen_test]
async fn test_sync_access_available_in_worker() {
    let ok: bool = sync_access_available_in_worker().await;
    // Не паникуем, если окружение не поддерживает — просто
    // логируем фактическое значение, чтобы при отладке было видно.
    let msg: String = format!("sync access in worker: {ok}");
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&msg));
}

// ---------------------------------------------------------------------------
// Async write/read roundtrip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_write_and_read_text_roundtrip() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "opfs_test_draft.json";
    let _ = remove_file(&root, name).await;

    let json = r#"{"title":"черновик","items":[1,2,3]}"#;
    write_text(&root, name, json).await.expect("write_text");

    let back = read_text(&root, name).await.expect("read_text");
    assert_eq!(back, json, "содержимое совпадает");

    remove_file(&root, name).await.expect("remove");
}

#[wasm_bindgen_test]
async fn test_write_text_idempotent() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "opfs_test_idem.txt";
    let _ = remove_file(&root, name).await;

    // Три раза пишем — каждый раз Ok, содержимое последнее.
    for i in 0..3 {
        let text: String = format!("version {i}");
        write_text(&root, name, &text).await.expect("write");
    }
    let back = read_text(&root, name).await.expect("read");
    assert_eq!(back, "version 2");

    remove_file(&root, name).await.expect("remove");
}

#[wasm_bindgen_test]
async fn test_remove_file_idempotent() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "opfs_test_remove.txt";
    let _ = remove_file(&root, name).await;

    write_text(&root, name, "bye").await.expect("write");

    // Правило проекта: удаление идемпотентно.
    remove_file(&root, name).await.expect("remove #1");
    remove_file(&root, name).await.expect("remove #2 (already gone)");
    remove_file(&root, name).await.expect("remove #3 (already gone)");
}

// ---------------------------------------------------------------------------
// Sync access handle из Worker
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_sync_write_in_worker_roundtrip() {
    if !sync_access_available_in_worker().await {
        return; // мягкий skip: createSyncAccessHandle недоступен
    }

    let name = "opfs_test_sync.bin";
    let data: Vec<u8> = (0..1024).map(|i| (i & 0xFF) as u8).collect();

    let (written, ms) = write_sync_in_worker(name, &data)
        .await
        .expect("write_sync_in_worker");

    assert_eq!(written, data.len(), "записано ровно столько байт");
    assert!(ms >= 0.0, "время не отрицательное");

    // Проверяем, что файл действительно есть и нужного размера.
    let root = opfs_root().await.expect("root");
    let handle = hello_wasm02::fsa_api_directory::get_file_handle(&root, name, false)
        .await
        .expect("file exists");
    let file = hello_wasm02::fsa_api_file::get_file(&handle)
        .await
        .expect("get_file");
    assert_eq!(file.size() as usize, data.len());

    remove_file(&root, name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn test_sync_write_large_1mb() {
    if !sync_access_available_in_worker().await {
        return;
    }

    let name = "opfs_test_sync_1mb.bin";
    let size: usize = 1024 * 1024;
    let data: Vec<u8> = (0..size).map(|i| (i & 0xFF) as u8).collect();

    let (written, ms) = write_sync_in_worker(name, &data)
        .await
        .expect("write 1 MB sync");

    assert_eq!(written, size);
    // 1 МБ синхронной записью — обычно < 50 мс. Верхний порог
    // разумно завышен, чтобы тест не был хрупким.
    assert!(ms < 500.0, "1 МБ должен писаться быстро, got {ms} ms");

    let root = opfs_root().await.expect("root");
    remove_file(&root, name).await.expect("cleanup");
}

// ---------------------------------------------------------------------------
// file_size / read_tail / list_files
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_file_size_and_read_tail() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "opfs_test_tail.txt";
    let _ = remove_file(&root, name).await;

    // Отсутствующего файла — размер 0, хвост пустой.
    let s0 = file_size(&root, name).await.unwrap_or(0.0);
    assert_eq!(s0, 0.0);
    let t0 = read_tail(&root, name, 64).await.unwrap_or_default();
    assert_eq!(t0, "");

    // Файл из 100 символов. Хвост не больше 10 байт.
    let body = "0123456789".repeat(10);
    write_text(&root, name, &body).await.expect("write");

    let s1 = file_size(&root, name).await.unwrap_or(0.0);
    assert_eq!(s1 as usize, body.len());

    let tail = read_tail(&root, name, 10).await.unwrap_or_default();
    assert!(tail.len() <= 10);
    assert!(body.ends_with(&tail));

    remove_file(&root, name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn test_list_files() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let a = "opfs_test_list_a.txt";
    let b = "opfs_test_list_b.txt";
    let _ = remove_file(&root, a).await;
    let _ = remove_file(&root, b).await;

    write_text(&root, a, "aaa").await.expect("write a");
    write_text(&root, b, "bbbb").await.expect("write b");

    let all = list_files(&root).await.expect("list");
    let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&a));
    assert!(names.contains(&b));

    let sa = all.iter().find(|(n, _)| n == a).map(|(_, s)| *s).unwrap_or(0.0);
    assert_eq!(sa, 3.0);

    remove_file(&root, a).await.expect("cleanup a");
    remove_file(&root, b).await.expect("cleanup b");
}

// ---------------------------------------------------------------------------
// Живой логгер в Worker
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_logger_worker_appends() {
    if !sync_access_available_in_worker().await {
        return;
    }

    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "opfs_test_logger.txt";
    let _ = remove_file(&root, name).await;

    // Стартуем логгер на 20 мс, ждём ~350 мс, стопаем.
    let logger = start_logger_worker(name, 20).expect("start logger");
    gloo_timers::future::TimeoutFuture::new(350).await;
    logger.stop();

    // За 350 мс при интервале 20 мс должно быть не меньше 5 строк.
    let text = read_tail(&root, name, 4096).await.unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() >= 5, "получено {} строк", lines.len());

    // Каждая строка — формата `[HH:MM:SS.mmm] line N`.
    assert!(lines[0].starts_with('['));
    assert!(lines[0].contains("line"));

    remove_file(&root, name).await.expect("cleanup");
}