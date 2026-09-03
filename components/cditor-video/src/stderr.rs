//! Bounded FFmpeg diagnostic stream handling.
//!
//! FFmpeg normally emits short human-readable lines, but malformed inputs and
//! external filters can write an arbitrarily long line without a newline. The
//! standard `BufRead::lines` iterator grows its temporary `String` until that
//! newline arrives. Read the stream in chunks instead so diagnostics can never
//! become an unbounded process allocation.

use std::{
    io::{self, BufRead, BufReader, Read},
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const STDERR_MAX_LINE_BYTES: usize = 4096;
pub(crate) const STDERR_MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

/// Delivers newline-delimited diagnostics while retaining at most one bounded
/// line in memory. A partial final line is delivered when the stream reaches
/// EOF. The callback is not invoked after cancellation is observed.
pub(crate) fn read_bounded_lines<R, F>(
    reader: R,
    stop_requested: &AtomicBool,
    mut callback: F,
) -> io::Result<()>
where
    R: Read,
    F: FnMut(String),
{
    let mut reader = BufReader::new(reader);
    let mut line = Vec::with_capacity(STDERR_MAX_LINE_BYTES);
    let mut truncated = false;

    loop {
        if stop_requested.load(Ordering::SeqCst) {
            break;
        }
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            if !line.is_empty() || truncated {
                callback(finish_line(&line, truncated));
            }
            break;
        }

        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let content_len = newline.unwrap_or(buffer.len());
        let consumed = newline.map_or(buffer.len(), |index| index + 1);

        if !truncated {
            let remaining = STDERR_MAX_LINE_BYTES.saturating_sub(line.len());
            let copied = content_len.min(remaining);
            line.extend_from_slice(&buffer[..copied]);
            if copied < content_len {
                truncated = true;
            }
        }
        reader.consume(consumed);

        if newline.is_some() {
            // Match `BufRead::lines`: remove the LF and an optional CR while
            // keeping empty lines visible to the diagnostic ring.
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            callback(finish_line(&line, truncated));
            line.clear();
            truncated = false;
        }
    }
    Ok(())
}

fn finish_line(bytes: &[u8], truncated: bool) -> String {
    let text = String::from_utf8_lossy(bytes);
    // Invalid UTF-8 replacement characters can expand the byte length, so do
    // the final bound on the UTF-8 string as well as on the input bytes.
    if truncated {
        let mut output = text.into_owned();
        output.push_str("...");
        truncate_utf8(&output, STDERR_MAX_LINE_BYTES)
    } else if text.len() > STDERR_MAX_LINE_BYTES {
        truncate_utf8(&text, STDERR_MAX_LINE_BYTES)
    } else {
        text.into_owned()
    }
}

pub(crate) fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    if max_bytes <= 3 {
        let mut end = max_bytes;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        return value[..end].to_owned();
    }
    let mut end = max_bytes - 3;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut output = value[..end].to_owned();
    output.push_str("...");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn long_line_is_read_in_bounded_chunks() {
        let input = format!("{}\nnext", "x".repeat(STDERR_MAX_LINE_BYTES * 32));
        let stop = AtomicBool::new(false);
        let mut lines = Vec::new();
        read_bounded_lines(Cursor::new(input), &stop, |line| lines.push(line)).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), STDERR_MAX_LINE_BYTES);
        assert!(lines[0].ends_with("..."));
        assert_eq!(lines[1], "next");
    }

    #[test]
    fn partial_line_and_crlf_match_line_reader_behavior() {
        let stop = AtomicBool::new(false);
        let mut lines = Vec::new();
        read_bounded_lines(Cursor::new(b"one\r\ntwo"), &stop, |line| lines.push(line)).unwrap();
        assert_eq!(lines, ["one", "two"]);
    }

    #[test]
    fn cancellation_stops_before_emitting_buffered_data() {
        let stop = AtomicBool::new(true);
        let mut emitted = false;
        read_bounded_lines(Cursor::new(b"line\n"), &stop, |_| emitted = true).unwrap();
        assert!(!emitted);
    }
}
