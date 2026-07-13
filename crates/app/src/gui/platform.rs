use std::borrow::Cow;

/// A concrete monospace family that ships with the host operating system.
///
/// GPUI expects a real font family name here; CSS-style generic families are
/// not resolved consistently by every native text backend.
#[cfg(target_os = "windows")]
pub(crate) const EDITOR_MONO_FONT_FAMILY: &str = "Consolas";

#[cfg(target_os = "macos")]
pub(crate) const EDITOR_MONO_FONT_FAMILY: &str = "Menlo";

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) const EDITOR_MONO_FONT_FAMILY: &str = "DejaVu Sans Mono";

/// Convert external/native text to the editor's internal LF convention.
///
/// Windows clipboard and TSF providers commonly expose CRLF. Lone carriage
/// returns also occur in text copied from older native controls, so both forms
/// are normalized without allocating for the overwhelmingly common LF case.
pub(crate) fn normalize_external_line_endings(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }

    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    Cow::Owned(normalized)
}

/// Native text services may report Enter as a text replacement instead of a
/// key event. Only a single logical line break is treated as Enter; pasted or
/// composed multiline content remains ordinary text.
pub(crate) fn is_single_line_break_commit(text: &str) -> bool {
    matches!(text, "\n" | "\r" | "\r\n" | "\u{2028}" | "\u{2029}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_line_endings_are_normalized_without_touching_lf_text() {
        assert!(matches!(
            normalize_external_line_endings("a\nb"),
            Cow::Borrowed(_)
        ));
        assert_eq!(normalize_external_line_endings("a\r\nb"), "a\nb");
        assert_eq!(normalize_external_line_endings("a\rb"), "a\nb");
        assert_eq!(normalize_external_line_endings("a\r\n\r\nb"), "a\n\nb");
    }

    #[test]
    fn only_one_native_line_break_is_an_enter_commit() {
        for text in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            assert!(is_single_line_break_commit(text), "{text:?}");
        }
        for text in ["", "a\n", "\n\n", "a\r\nb"] {
            assert!(!is_single_line_break_commit(text), "{text:?}");
        }
    }
}
