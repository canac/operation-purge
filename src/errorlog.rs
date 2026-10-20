//! The single error sink, replacing `cLogFile` (`cLogFile.cpp`).
//!
//! Every [`log_error`] call goes to stderr AND is appended to `Logfile.txt`
//! beside `config.toml`, because the Windows release exe has no console
//! (`windows_subsystem = "windows"`) and stderr alone would vanish. As in
//! the original, the file is created lazily: an error-free run leaves none.

use std::fmt::Display;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const HEADER_RULE: &str = "------------------------------------------------------------";

/// Logs `msg` to stderr and the log file; never panics or fails.
///
/// Prefer the [`log_error!`](crate::log_error) macro for formatted messages.
pub fn log_error(msg: impl Display) {
    static LOG: Mutex<Option<ErrorLog>> = Mutex::new(None);
    let text = msg.to_string();
    eprintln!("{text}");
    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let log = guard.get_or_insert_with(|| ErrorLog::new(log_path()));
    if let Err(e) = log.message(&text) {
        eprintln!("could not append to the error log: {e}");
    }
}

/// `format!`-style front end for [`errorlog::log_error`](crate::errorlog::log_error).
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::errorlog::log_error(format_args!($($arg)*))
    };
}

/// `Logfile.txt` in the same directory as `config.toml` (so `$OP_CONFIG`
/// relocates it too), or `None` when the platform has no config dir.
pub fn log_path() -> Option<PathBuf> {
    crate::app::config::Config::path().map(|p| p.with_file_name("Logfile.txt"))
}

struct ErrorLog {
    path: Option<PathBuf>,
    header_written: bool,
}

impl ErrorLog {
    fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            header_written: false,
        }
    }

    /// Appends one entry, creating the file and writing the session header
    /// on the first call (`cLogFile.cpp:74-76`).
    fn message(&mut self, text: &str) -> std::io::Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        let mut file = open_append(path)?;
        if !self.header_written {
            writeln!(file, "{HEADER_RULE}\nOperation Purge {}", env!("CARGO_PKG_VERSION"))?;
            self.header_written = true;
        }
        writeln!(file, "{text}")?;
        file.flush()
    }
}

fn open_append(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "op-errorlog-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        path.join("Logfile.txt")
    }

    #[test]
    fn no_file_is_created_until_the_first_message() {
        let path = temp_log_path("lazy");
        let _log = ErrorLog::new(Some(path.clone()));
        assert!(!path.exists());
    }

    #[test]
    fn header_is_written_once_and_messages_append() {
        let path = temp_log_path("append");
        let mut log = ErrorLog::new(Some(path.clone()));
        log.message("first error").unwrap();
        log.message("second error").unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).ok();

        assert!(contents.starts_with(HEADER_RULE));
        assert_eq!(contents.matches(HEADER_RULE).count(), 1);
        assert!(contents.contains("first error\nsecond error\n"));
    }

    #[test]
    fn no_path_is_a_silent_no_op() {
        ErrorLog::new(None).message("dropped").unwrap();
    }

    #[test]
    fn log_lives_beside_config() {
        let (Some(log), Some(config)) = (log_path(), crate::app::config::Config::path()) else {
            return;
        };
        assert_eq!(log.parent(), config.parent());
    }
}
