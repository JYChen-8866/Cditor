use std::fmt;
use std::io::{self, Write};

pub(crate) fn write(line: fmt::Arguments<'_>) {
    let stderr = io::stderr();
    write_line(&mut stderr.lock(), line);
}

pub(super) fn write_line(writer: &mut impl Write, line: fmt::Arguments<'_>) {
    let _ = writeln!(writer, "{line}");
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::write_line;

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
        write_line(&mut BrokenPipe, format_args!("gui trace"));
    }
}
