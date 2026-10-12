//! Drag & Drop — получение файлов через DataTransfer.
//! Модуль автономен: не зависит от других модулей проекта,
//! кроме `file_api` (для типа `FileMetadata`).

use web_sys::{DataTransfer, DragEvent, FileList};
use crate::file_api::FileMetadata;

/// Извлекает список файлов из события `drop`.
///
/// `DataTransfer::files()` возвращает `Option<FileList>` — `None`,
/// если событие не содержит `DataTransfer` (синтетическое событие
/// или drop без данных).
pub fn files_from_drop_event(event: &DragEvent) -> Option<FileList> {
    let dt: DataTransfer = event.data_transfer()?;
    dt.files()
}

/// Извлекает метаданные всех файлов из события `drop`.
///
/// Возвращает пустой вектор, если файлов нет — удобно для
/// отображения в UI без дополнительных проверок.
pub fn metadata_from_drop_event(event: &DragEvent) -> Vec<FileMetadata> {
    match files_from_drop_event(event) {
        Some(list) => return crate::file_api::file_list_metadata(&list),
        None => return Vec::new(),
    }
}

/// Проверяет, содержит ли событие хотя бы один файл.
///
/// Полезно для валидации на входе: некоторые перетаскивания
/// содержат только текст или HTML — их нужно игнорировать.
pub fn has_files(event: &DragEvent) -> bool {
    match files_from_drop_event(event) {
        Some(list) => return list.length() > 0,
        None => return false,
    }
}

/// Предотвращает стандартное поведение браузера при drag&drop.
///
/// **Обязательно** вызывать на событиях `dragover` и `drop`.
/// Без `prevent_default` на `dragover` браузер не разрешит drop;
/// без `prevent_default` на `drop` браузер попытается открыть
/// перетащенный файл вместо нашей обработки.
///
/// `stop_propagation` предотвращает всплытие события к родительским
/// элементам — на случай, если у них тоже есть обработчики.
pub fn prevent_default(event: &DragEvent) {
    event.prevent_default();
    event.stop_propagation();
}