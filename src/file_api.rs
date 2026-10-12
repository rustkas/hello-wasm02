//! Работа с File, Blob, FileList и метаданными файлов.
//! Модуль автономен: не зависит от других модулей проекта.

use web_sys::{File, FileList};

/// Метаданные файла: имя, размер, MIME-тип, дата изменения.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMetadata {
    pub name: String,
    pub size: f64,
    pub mime_type: String,
    pub last_modified: f64,
}

impl FileMetadata {
    /// Извлекает метаданные из `web_sys::File`.
    pub fn from_file(file: &File) -> Self {
        Self {
            name: file.name(),
            size: file.size(),
            mime_type: file.type_(),
            last_modified: file.last_modified(),
        }
    }
}

/// Возвращает количество файлов в списке.
pub fn file_list_length(list: &FileList) -> u32 {
    list.length()
}

/// Возвращает файл по индексу, если он существует.
pub fn file_list_get(list: &FileList, index: u32) -> Option<File> {
    list.get(index)
}

/// Преобразует `FileList` в вектор метаданных.
pub fn file_list_metadata(list: &FileList) -> Vec<FileMetadata> {
    (0..list.length())
        .filter_map(|i| list.get(i))
        .map(|f| FileMetadata::from_file(&f))
        .collect()
}

/// Проверяет, является ли файл изображением (по MIME-типу).
pub fn is_image(file: &File) -> bool {
    file.type_().starts_with("image/")
}

/// Форматирует размер файла в человекочитаемый вид.
pub fn format_file_size(bytes: f64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes;
    let mut unit_idx: usize = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    return format!("{:.2} {}", size, UNITS[unit_idx])
}