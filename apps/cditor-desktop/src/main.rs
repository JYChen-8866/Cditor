#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use cditor_desktop::environment::cditor_from_env;
use cditor_sdk::Cditor;
use std::io::Write;
#[cfg(feature = "ai")]
use std::sync::Arc;

#[cfg(feature = "ai")]
use cditor_ai_openai::OpenAiCompatibleProvider;

fn write_stderr(message: std::fmt::Arguments<'_>) {
    write_stderr_to(&mut std::io::stderr().lock(), message);
}

fn write_stderr_to(writer: &mut impl Write, message: std::fmt::Arguments<'_>) {
    let _ = writer.write_fmt(message);
    let _ = writer.write_all(b"\n");
}

fn configured_cditor() -> Cditor {
    let cditor = cditor_from_env().unwrap_or_else(|error| {
        write_stderr(format_args!(
            "Invalid Cditor desktop configuration: {error}"
        ));
        std::process::exit(2);
    });
    write_stderr(format_args!(
        "Cditor backend: {:?}, document: {:?}",
        cditor.options().backend,
        cditor.options().document_id
    ));
    configure_ai(cditor)
}

#[cfg(feature = "ai")]
fn configure_ai(cditor: Cditor) -> Cditor {
    match OpenAiCompatibleProvider::from_env() {
        Ok(provider) => cditor.with_ai_provider(Arc::new(provider)),
        Err(error) => {
            write_stderr(format_args!("AI provider disabled: {error}"));
            cditor
        }
    }
}

#[cfg(not(feature = "ai"))]
fn configure_ai(cditor: Cditor) -> Cditor {
    cditor.without_ai()
}

fn main() {
    cditor_desktop::wiring::run_desktop(configured_cditor());
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }
    }

    #[test]
    fn desktop_diagnostics_ignore_a_closed_stderr_pipe() {
        write_stderr_to(&mut ClosedPipe, format_args!("payload trace"));
    }
}
