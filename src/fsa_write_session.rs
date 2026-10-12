//! Модель сессии записи: состояния, интенты, ошибки.
//!
//! Сессия — это транзакция. `write()` идёт в буфер, `close()` фиксирует
//! изменения, `abort()` их отбрасывает. Если сессию не закрыть и не
//! откатить явно, `Drop` вызовет `abort()` — незакоммиченная транзакция
//! не оставляет следов.

use crate::fsa_choose_options::format_str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Open,
    Committed,
    Aborted,
    Errored,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Open => return "открыта",
            SessionState::Committed => return "зафиксирована",
            SessionState::Aborted => return "отменена",
            SessionState::Errored => return "ошибка",
        }
    }
    pub fn is_terminal(self) -> bool {
        return !matches!(self, SessionState::Open);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitStats {
    pub writes: u32,
    pub bytes_written: u64,
    pub elapsed_ms: f64,
    pub keep_existing: bool,
}

impl CommitStats {
    pub fn summary(&self) -> String {
        let keep_label: &str = if self.keep_existing { "с keep" } else { "без keep" };
        let s: String = format_str(format_args!(
            "Зафиксировано {keep_label}: {} операций, {} байт за {:.0} ms",
            self.writes, self.bytes_written, self.elapsed_ms,
        ));
        return s;
    }
}

/// Статистика «наивной» перезаписи: сколько байт прочитали и записали.
#[derive(Debug, Clone, PartialEq)]
pub struct NaiveStats {
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub elapsed_ms: f64,
}

impl NaiveStats {
    pub fn summary(&self) -> String {
        let s: String = format_str(format_args!(
            "Наивно: read {} байт, write {} байт за {:.0} ms",
            self.bytes_read, self.bytes_written, self.elapsed_ms,
        ));
        return s;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WriteError {
    SessionClosed,
    NoSession,
    Io(String),
}

impl WriteError {
    pub fn explain(&self) -> String {
        match self {
            WriteError::SessionClosed => {
                return "✗ Сессия уже закрыта. write/seek/truncate после close/abort \
                        не сработают — откройте новую через createWritable()."
                    .to_string();
            }
            WriteError::NoSession => {
                return "✗ Сессия не открыта. Сначала откройте writable через createWritable()."
                    .to_string();
            }
            WriteError::Io(msg) => {
                let s: String = format_str(format_args!("✗ Ошибка записи: {msg}"));
                return s;
            }
        }
    }
}