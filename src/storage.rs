//! Помощники для работы с Web Storage API.
//!
//! ## Что в этом модуле
//!
//! - `local_storage` / `session_storage` — доступ к хранилищам.
//! - `get` / `set` / `remove` — базовые операции.
//! - `length` / `key` / `keys` / `entries` — итерация по хранилищу.
//! - `has_key` — проверка наличия ключа.
//! - `clear` — полная очистка.
//! - `is_storage_writable` — проверка, что хранилище принимает записи.
//! - `set_json` / `get_json` — сериализация структур через serde_json.
//!
//! ## Что будет дальше
//!
//! В следующих подглавах модуль расширится:
//! - 15.7 — `parse_storage_event`, `format_storage_change`.
//! - 15.8 — `fill_to_quota`, `measure_sync_block`, `current_origin`.
#![allow(dead_code)]

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::JsValue;
use web_sys::Storage;

// ============================================================
// Получение доступа к хранилищам
// ============================================================

/// Возвращает `window.localStorage`.
///
/// # Ошибки
///
/// Возвращает `Err`, если окно недоступно или браузер запретил
/// доступ к `localStorage` (например, в приватном режиме).
pub fn local_storage() -> Result<Storage, JsValue> {
    return web_sys::window()
        .expect("no window")
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("localStorage is not available"));
}

/// Возвращает `window.sessionStorage`.
pub fn session_storage() -> Result<Storage, JsValue> {
    return web_sys::window()
        .expect("no window")
        .session_storage()?
        .ok_or_else(|| JsValue::from_str("sessionStorage is not available"));
}

// ============================================================
// Базовые операции
// ============================================================

/// Читает значение по ключу.
///
/// Возвращает `None`, если ключа нет.
///
/// Внутри: `get_item` возвращает `Result<Option<String>, JsValue>`.
/// `.ok()` превращает `Result` в `Option`, `.flatten()` убирает
/// вложенный `Option`. Итог — плоский `Option<String>`.
pub fn get(storage: &Storage, key: &str) -> Option<String> {
    return storage.get_item(key).ok().flatten();
}

/// Устанавливает значение.
///
/// # Ошибки
///
/// Возвращает `Err`, если хранилище переполнено (`QuotaExceededError`)
/// или доступ запрещён.
pub fn set(storage: &Storage, key: &str, value: &str) -> Result<(), JsValue> {
    return storage.set_item(key, value);
}

/// Удаляет ключ.
///
/// Удаление несуществующего ключа не считается ошибкой.
pub fn remove(storage: &Storage, key: &str) -> Result<(), JsValue> {
    return storage.remove_item(key);
}

// ============================================================
// Итерация по хранилищу
// ============================================================

/// Возвращает количество ключей в хранилище.
///
/// `length` в `web_sys` возвращает `Result<u32, JsValue>`. Ошибка
/// маловероятна, поэтому используем `unwrap_or(0)` — при сбое
/// вернём 0, что безопасно для итерации.
pub fn length(storage: &Storage) -> u32 {
    return storage.length().unwrap_or(0);
}

/// Возвращает ключ по индексу.
///
/// Индекс — позиция в хранилище, от 0 до `length() - 1`.
/// Если индекс вне диапазона, возвращает `None`.
///
/// Порядок ключей зависит от браузера. Спецификация не гарантирует
/// порядок вставки, но большинство браузеров возвращают ключи
/// в порядке добавления.
pub fn key(storage: &Storage, index: u32) -> Option<String> {
    return storage.key(index).ok().flatten();
}

/// Возвращает все ключи хранилища.
///
/// Обходит хранилище через `length()` + `key(i)`. Возвращает
/// `Vec<String>` в порядке, который вернул браузер.
pub fn keys(storage: &Storage) -> Vec<String> {
    let len = length(storage);
    return (0..len).filter_map(|i| key(storage, i)).collect();
}

/// Возвращает все пары ключ-значение.
///
/// Использует `keys()` для получения списка ключей, затем читает
/// значение каждого через `get()`. Пары, у которых значение не
/// удалось прочитать, пропускаются.
pub fn entries(storage: &Storage) -> Vec<(String, String)> {
    return keys(storage)
        .into_iter()
        .filter_map(|k| get(storage, &k).map(|v| (k, v)))
        .collect();
}

// ============================================================
// Проверка наличия
// ============================================================

/// Проверяет наличие ключа.
///
/// У `Storage` нет метода `has`. Проверка реализуется через
/// `get(key).is_some()`. Обёртка добавлена для читаемости:
/// `has_key(&ls, "theme")` понятнее, чем `get(&ls, "theme").is_some()`.
pub fn has_key(storage: &Storage, key: &str) -> bool {
    return get(storage, key).is_some();
}

// ============================================================
// Полная очистка
// ============================================================

/// Очищает хранилище.
///
/// Удаляет **все** ключи. Не путать с удалением одного ключа
/// через `remove`.
pub fn clear(storage: &Storage) -> Result<(), JsValue> {
    return storage.clear();
}

// ============================================================
// JSON-сериализация
// ============================================================

/// Сохраняет значение как JSON.
///
/// `T` должен реализовать `Serialize`. Обычно это достигается
/// через `#[derive(Serialize)]` на структуре.
///
/// # Ошибки
///
/// Возвращает `Err`, если:
/// - Сериализация в JSON провалилась (редкий случай — структура
///   содержит что-то, что serde не умеет кодировать).
/// - Запись в Storage провалилась (`QuotaExceededError`).
///
/// # Пример
///
/// ```no_run
/// use hello_wasm02::storage;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Config { theme: String }
///
/// let ls = storage::local_storage().unwrap();
/// let config = Config { theme: "dark".into() };
/// storage::set_json(&ls, "config", &config).unwrap();
/// ```
pub fn set_json<T: Serialize>(
    storage: &Storage,
    key: &str,
    value: &T,
) -> Result<(), JsValue> {
    let json = serde_json::to_string(value)
        .map_err(|e| JsValue::from_str(&format!("serialize error: {}", e)))?;
    return set(storage, key, &json);
}

/// Читает значение как JSON.
///
/// `T` должен реализовать `DeserializeOwned`. Обычно это
/// достигается через `#[derive(Deserialize)]` на структуре.
///
/// Возвращает `None`, если:
/// - Ключа нет в хранилище.
/// - Значение не является валидным JSON.
/// - JSON валиден, но не соответствует схеме `T`.
///
/// # Пример
///
/// ```no_run
/// use hello_wasm02::storage;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Config { theme: String }
///
/// let ls = storage::local_storage().unwrap();
/// let config: Option<Config> = storage::get_json(&ls, "config");
/// ```
pub fn get_json<T: DeserializeOwned>(
    storage: &Storage,
    key: &str,
) -> Option<T> {
    let raw = get(storage, key)?;
    return serde_json::from_str(&raw).ok();
}

// ============================================================
// StorageEvent
// ============================================================

/// Описывает событие `storage`.
///
/// ## Особенности Web Storage событий
///
/// - Событие `storage` генерируется **только для localStorage**.
/// - Для sessionStorage события **не генерируются** — данные изолированы
///   по вкладкам, синхронизировать нечего.
/// - Событие приходит **только в другие вкладки** того же origin.
///   Вкладка, в которой произошло изменение, событие **не получает**.
#[derive(Default)]
pub struct StorageChange {
    pub key: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub url: Option<String>,
    pub storage_area: Option<String>,
}

/// Извлекает данные из `StorageEvent`.
///
/// ## Про поле `storageArea`
///
/// Поле не извлекается из самого события. Событие `storage`
/// генерируется только для `localStorage` — `sessionStorage`
/// не генерирует межвкладочных событий. Поэтому `storage_area`
/// всегда `"localStorage"`.
///
/// В синтетических событиях (`StorageEvent::new("storage")`)
/// `storageArea` = `null`. Если бы мы полагались на
/// `event.storage_area()`, синтетические события давали бы `None`,
/// что противоречило бы реальной семантике.
pub fn parse_storage_event(event: &web_sys::StorageEvent) -> StorageChange {
    return StorageChange {
        key: event.key(),
        old_value: event.old_value(),
        new_value: event.new_value(),
        url: event.url(),
        storage_area: Some("localStorage".to_string()),
    };
}

/// Форматирует `StorageChange` для логирования.
///
/// Возвращает многострочное представление, включающее:
/// - изменение (set/remove/clear),
/// - область хранения (`storageArea`),
/// - источник изменения (`url`),
/// - старое значение (если было).
///
/// Реализовано через последовательность `if` с явными `return`,
/// чтобы избежать проблемы `rust-analyzer` с tail-выражениями `match`.
pub fn format_storage_change(change: &StorageChange) -> String {
    let mut lines = Vec::new();

    // Основное сообщение
    if change.key.is_none() {
        lines.push("Storage cleared".to_string());
    } else {
        let key = change.key.as_ref().unwrap();
        if let Some(v) = &change.new_value {
            lines.push(format!("[{}] = \"{}\"", key, v));
        } else {
            lines.push(format!("[{}] removed", key));
        }
    }

    // Область хранения
    if let Some(area) = &change.storage_area {
        lines.push(format!("  area: {}", area));
    }

    // Источник изменения
    if let Some(url) = &change.url {
        lines.push(format!("  from: {}", url));
    }

    // Старое значение — только при set (не при remove и не при clear)
    if let Some(old) = &change.old_value {
        if change.new_value.is_some() {
            lines.push(format!("  was: \"{}\"", old));
        }
    }

    return lines.join("\n");
}

// ============================================================
// Оценка origin
// ============================================================

/// Возвращает текущий origin (protocol + host + port).
///
/// Origin — это тройка из протокола, хоста и порта. Storage
/// привязан к origin: данные, сохранённые на одном origin,
/// не видны на другом.
///
/// Возвращает строку вида `http://127.0.0.1:5173`. Если получить
/// origin не удалось — `"unknown"`.
pub fn current_origin() -> String {
    return web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "unknown".to_string());
}

// ============================================================
// Демонстрация ограничений Storage
// ============================================================

/// Результат заполнения хранилища до квоты.
pub struct FillResult {
    /// Сколько ключей успели записать.
    pub keys_written: u32,
    /// Суммарный размер записанных данных в символах.
    pub total_chars: usize,
    /// Сообщение об ошибке, если она возникла.
    pub error_message: Option<String>,
    /// Достигли ли квоты. `true` — да, `false` — остановились по лимиту.
    pub hit_quota: bool,
}

/// Заполняет хранилище до срабатывания `QuotaExceededError`.
///
/// Пишет ключи `{prefix}:0`, `{prefix}:1`, ... со значением из
/// `chunk_size` символов. Останавливается, когда браузер возвращает
/// ошибку квоты, или когда записано `max_keys` ключей.
///
/// `max_keys` — защита от бесконечного цикла. Если браузер не
/// возвращает ошибку (например, лимит больше ожидаемого), мы всё
/// равно остановимся.
pub fn fill_to_quota(
    storage: &Storage,
    prefix: &str,
    chunk_size: usize,
    max_keys: u32,
) -> FillResult {
    let chunk = "x".repeat(chunk_size);
    let mut keys_written = 0u32;
    let mut total_chars = 0usize;

    loop {
        if keys_written >= max_keys {
            return FillResult {
                keys_written,
                total_chars,
                error_message: Some(format!(
                    "Остановлено после {} ключей (защита от бесконечного цикла)",
                    max_keys
                )),
                hit_quota: false,
            };
        }

        let key = format!("{}:{}", prefix, keys_written);
        match storage.set_item(&key, &chunk) {
            Ok(_) => {
                keys_written += 1;
                total_chars += key.len() + chunk.len();
            }
            Err(e) => {
                let message = e
                    .as_string()
                    .unwrap_or_else(|| format!("{:?}", e));
                return FillResult {
                    keys_written,
                    total_chars,
                    error_message: Some(message),
                    hit_quota: true,
                };
            }
        }
    }
}

/// Удаляет все ключи с заданным префиксом.
///
/// Возвращает количество удалённых ключей. Полезно для очистки
/// после тестов квоты: `clear_prefix(&ls, "quota-test")`.
pub fn clear_prefix(storage: &Storage, prefix: &str) -> u32 {
    let to_remove: Vec<String> = keys(storage)
        .into_iter()
        .filter(|k| k.starts_with(prefix))
        .collect();
    let count = to_remove.len() as u32;
    for k in to_remove {
        storage.remove_item(&k).ok();
    }
    return count;
}

/// Результат замера синхронной блокировки.
pub struct SyncBlockResult {
    /// Сколько миллисекунд заняла запись.
    pub duration_ms: f64,
    /// Сколько байт записано.
    pub bytes_written: usize,
    /// Сколько ключей записано.
    pub keys_written: u32,
}

/// Записывает `size_kb` КБ данных в storage, замеряя время.
///
/// Демонстрирует, что Web Storage — синхронный API: запись блокирует
/// главный поток на всё время операции. Чем больше данных, тем
/// заметнее пауза в UI.
///
/// Использует `js_sys::Date::now()` — миллисекунды с высокой точностью.
/// Останавливается, если запись провалилась.
pub fn measure_sync_block(
    storage: &Storage,
    size_kb: usize,
    prefix: &str,
) -> SyncBlockResult {
    let chunk_size = 1024; // ~1 КБ
    let chunk = "x".repeat(chunk_size);

    let start = js_sys::Date::now();

    let mut keys_written = 0u32;
    let mut bytes_written = 0usize;

    for i in 0..size_kb {
        let key = format!("{}:{}", prefix, i);
        match storage.set_item(&key, &chunk) {
            Ok(_) => {
                keys_written += 1;
                bytes_written += key.len() + chunk.len();
            }
            Err(_) => break,
        }
    }

    let end = js_sys::Date::now();

    return SyncBlockResult {
        duration_ms: end - start,
        bytes_written,
        keys_written,
    };
}

// ============================================================
// Проверка доступности записи
// ============================================================

/// Проверяет, что хранилище принимает записи.
///
/// Работает по принципу «запиши и прочитай»:
/// 1. Записывает тестовый ключ `__test_writable__`.
/// 2. Читает его обратно.
/// 3. Сравнивает с исходным значением.
/// 4. Удаляет тестовый ключ.
///
/// Возвращает `true`, если все четыре шага прошли успешно.
///
/// # Когда это нужно
///
/// В приватном режиме некоторых браузеров `localStorage`
/// открывается, но **запись** в него проваливается с
/// `QuotaExceededError`. Простая проверка `local_storage().is_ok()`
/// этого не поймает. `is_storage_writable` — ловит.
pub fn is_storage_writable(storage: &Storage) -> bool {
    let test_key = "__test_writable__";
    let test_value = "test";

    match storage.set_item(test_key, test_value) {
        Ok(_) => {
            let read_back = get(storage, test_key);
            storage.remove_item(test_key).ok();
            return read_back.as_deref() == Some(test_value);
        }
        Err(_) => return false,
    }
}