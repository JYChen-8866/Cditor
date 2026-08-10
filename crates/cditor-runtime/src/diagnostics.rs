use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Diagnostic output is best-effort. A closed shell pipe must never terminate
/// the editor, including builds that use `panic = "abort"`.
pub(crate) fn write_stderr(line: fmt::Arguments<'_>) {
    let stderr = io::stderr();
    write_line(&mut stderr.lock(), line);
    append_log_line(line);
}

/// Appends one diagnostics line to the persistent log file.
///
/// Every `[cditor]` diagnostics funnel (runtime and GUI side) routes through
/// this in addition to stderr: a Windows GUI process has no console, so
/// stderr-only diagnostics vanish exactly where they are needed most. The
/// destination resolves once per process:
///
/// - `CDITOR_LOG_FILE=<path>` — explicit file path;
/// - `CDITOR_LOG_FILE=off|0|none|false` — disable file persistence;
/// - unset — `<temp dir>/cditor-diagnostics.log`; the chosen path is printed
///   to stderr on first write.
///
/// The previous run's file is rotated to `*.prev.log` on first open. Lines
/// carry a Unix-epoch `[secs.millis]` prefix for correlation.
pub fn append_log_line(line: fmt::Arguments<'_>) {
    let Some(sink) = log_sink() else {
        return;
    };
    let Ok(mut file) = sink.lock() else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let _ = writeln!(
        file,
        "[{}.{:03}] {line}",
        timestamp.as_secs(),
        timestamp.subsec_millis()
    );
}

fn write_line(writer: &mut impl Write, line: fmt::Arguments<'_>) {
    let _ = writeln!(writer, "{line}");
}

fn log_sink() -> Option<&'static Mutex<File>> {
    static SINK: OnceLock<Option<Mutex<File>>> = OnceLock::new();
    SINK.get_or_init(open_log_sink).as_ref()
}

fn open_log_sink() -> Option<Mutex<File>> {
    let path = configured_log_path(std::env::var("CDITOR_LOG_FILE").ok())?;
    rotate_previous_log(&path);
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => {
            write_line(
                &mut io::stderr().lock(),
                format_args!("[cditor][log] diagnostics persisted to {}", path.display()),
            );
            Some(Mutex::new(file))
        }
        Err(error) => {
            write_line(
                &mut io::stderr().lock(),
                format_args!(
                    "[cditor][log] cannot open {}: {error}; file logging disabled",
                    path.display()
                ),
            );
            None
        }
    }
}

fn configured_log_path(configured: Option<String>) -> Option<PathBuf> {
    match configured {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Some(default_log_path());
            }
            if matches!(
                trimmed.to_ascii_lowercase().as_str(),
                "off" | "0" | "none" | "false"
            ) {
                return None;
            }
            Some(PathBuf::from(trimmed))
        }
        None => Some(default_log_path()),
    }
}

fn default_log_path() -> PathBuf {
    std::env::temp_dir().join("cditor-diagnostics.log")
}

fn rotate_previous_log(path: &Path) {
    if !path.exists() {
        return;
    }
    let mut previous = path.to_path_buf();
    previous.set_extension("prev.log");
    if previous == path {
        return;
    }
    let _ = std::fs::rename(path, &previous);
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::{configured_log_path, default_log_path, write_line};

    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
        }
    }

    #[test]
    fn closed_diagnostic_pipe_is_ignored() {
        write_line(&mut BrokenPipe, format_args!("runtime timing"));
    }

    #[test]
    fn log_path_resolution_covers_default_explicit_and_disabled() {
        assert_eq!(configured_log_path(None), Some(default_log_path()));
        assert_eq!(
            configured_log_path(Some("  ".to_owned())),
            Some(default_log_path())
        );
        assert_eq!(
            configured_log_path(Some("/tmp/custom-cditor.log".to_owned())),
            Some("/tmp/custom-cditor.log".into())
        );
        for disabled in ["off", "OFF", "0", "none", "false"] {
            assert_eq!(configured_log_path(Some(disabled.to_owned())), None);
        }
    }

    #[test]
    fn rotation_target_replaces_only_the_final_extension() {
        let mut path = std::path::PathBuf::from("/tmp/cditor-diagnostics.log");
        path.set_extension("prev.log");
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/cditor-diagnostics.prev.log")
        );
    }
}
