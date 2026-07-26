use std::fmt;
#[cfg(test)]
use std::io::Write;
use std::sync::{Mutex, OnceLock};

pub(crate) fn trace_payload(stage: &str, details: fmt::Arguments<'_>) {
    if payload_trace_enabled() {
        super::stderr::write(format_args!("[cditor][payload][{stage}] {details}"));
    }
}

pub(crate) fn trace_payload_state(stage: &str, details: fmt::Arguments<'_>) {
    if !payload_trace_enabled() {
        return;
    }
    let line = format!("[cditor][payload][{stage}] {details}");
    static LAST_STATE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    let Ok(mut last) = LAST_STATE.get_or_init(|| Mutex::new(None)).lock() else {
        super::stderr::write(format_args!("{line}"));
        return;
    };
    if last.as_deref() == Some(line.as_str()) {
        return;
    }
    super::stderr::write(format_args!("{line}"));
    *last = Some(line);
}

#[cfg(test)]
fn write_payload_line(writer: &mut impl Write, line: fmt::Arguments<'_>) {
    super::stderr::write_line(writer, line);
}

fn payload_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_PAYLOAD")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::write_payload_line;

    #[derive(Default)]
    struct BrokenPipeWriter {
        write_attempts: usize,
    }

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            self.write_attempts += 1;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "diagnostic consumer closed the pipe",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "diagnostic consumer closed the pipe",
            ))
        }
    }

    #[test]
    fn payload_trace_does_not_panic_when_stderr_pipe_is_closed() {
        let mut writer = BrokenPipeWriter::default();

        write_payload_line(
            &mut writer,
            format_args!("[cditor][payload][projection.placeholder] blocks=0"),
        );

        assert!(writer.write_attempts > 0);
    }
}
