//! Fatal-error log channel, replacing `cLogFile` (libs-map §1.1, §5.4).
//!
//! In the original, "logging" IS the fatal-error path: every call site is
//! an error report, and the log callback pops an "internal error — exit?"
//! dialog. The file is lazily created — an error-free run leaves no
//! `Logfile.txt` at all (`cLogFile.cpp`). The port keeps that laziness but
//! moves the file out of the install directory (design §4.6: original
//! wrote to CWD = install dir, broken under UAC) to
//! `dirs::data_local_dir()/operation-purge/Logfile.txt`.

use std::io::Write as _;
use std::path::PathBuf;

/// Session header format the original used, kept for continuity even
/// though byte-compatibility with old logs was never a goal (libs-map
/// §5.4: "probably not" load-bearing).
const HEADER_RULE: &str = "------------------------------------------------------------";

pub struct ErrorLog {
    path: PathBuf,
    app_name: &'static str,
    /// Lazily set on first `message()` call — mirrors "file not opened
    /// until first message" (libs-map §1.1).
    header_written: bool,
}

impl ErrorLog {
    pub fn new(app_name: &'static str) -> Self {
        let path = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("operation-purge")
            .join("Logfile.txt");
        Self {
            path,
            app_name,
            header_written: false,
        }
    }

    /// Appends one entry; lazily creates the file and its parent dir and
    /// writes the session header on the very first call (parity with the
    /// original's lazy-open + one-time strftime header, `cLogFile.cpp:74-76`).
    /// Body kept minimal for M0; the fatal-modal channel lands in
    /// `game::screens` at M11.
    pub fn message(&mut self, text: &str) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        if !self.header_written {
            writeln!(file, "{HEADER_RULE}")?;
            self.header_written = true;
        }
        writeln!(file, "{}\n{text}\n", self.app_name)?;
        file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per call: `SystemTime` nanos alone can collide when tests run
    /// concurrently on platforms with coarser-than-nanosecond clock
    /// resolution, which previously made
    /// `header_is_written_only_once_across_multiple_messages` flaky by
    /// letting two tests share one log file. A per-process atomic counter
    /// guarantees distinct paths regardless of clock granularity.
    fn temp_log_path() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "op-errorlog-test-{}-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n
        ))
    }

    /// "an error-free run leaves no `Logfile.txt` at all" (libs-map §1.1) —
    /// the file must not exist merely from constructing an `ErrorLog`.
    #[test]
    fn no_file_is_created_until_the_first_message() {
        let path = temp_log_path();
        let _log = ErrorLog {
            path: path.clone(),
            app_name: "TestApp",
            header_written: false,
        };
        assert!(!path.exists());
    }

    #[test]
    fn first_message_lazily_creates_the_file_with_a_header() {
        let path = temp_log_path();
        let mut log = ErrorLog {
            path: path.clone(),
            app_name: "TestApp",
            header_written: false,
        };
        log.message("something went wrong").unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert!(contents.starts_with(HEADER_RULE));
        assert!(contents.contains("TestApp"));
        assert!(contents.contains("something went wrong"));
    }

    #[test]
    fn header_is_written_only_once_across_multiple_messages() {
        let path = temp_log_path();
        let mut log = ErrorLog {
            path: path.clone(),
            app_name: "TestApp",
            header_written: false,
        };
        log.message("first error").unwrap();
        log.message("second error").unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(contents.matches(HEADER_RULE).count(), 1);
        assert!(contents.contains("first error"));
        assert!(contents.contains("second error"));
    }
}
