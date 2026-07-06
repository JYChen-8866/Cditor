use std::ops::Range;

pub fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn utf8_to_utf16_offset(text: &str, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(text, offset);
    text[..offset].encode_utf16().count()
}

pub fn utf16_to_utf8_offset(text: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut utf16_count = 0;
    for (utf8_offset, ch) in text.char_indices() {
        if utf16_count >= offset {
            return utf8_offset;
        }
        utf16_count += ch.len_utf16();
        if utf16_count >= offset {
            return utf8_offset + ch.len_utf8();
        }
    }
    text.len()
}

pub fn utf8_range_to_utf16_range(text: &str, range: &Range<usize>) -> Range<usize> {
    utf8_to_utf16_offset(text, range.start)..utf8_to_utf16_offset(text, range.end)
}

pub fn utf16_range_to_utf8_range(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = utf16_to_utf8_offset(text, range.start);
    let end = utf16_to_utf8_offset(text, range.end);
    clamp_to_char_boundary(text, start)..clamp_to_char_boundary(text, end)
}

pub fn marked_preview_range_to_base_range(
    preview_range: Range<usize>,
    base_marked_range: Range<usize>,
    preview_marked_range: Range<usize>,
) -> Range<usize> {
    if preview_range.start < preview_marked_range.end
        && preview_marked_range.start < preview_range.end
    {
        base_marked_range
    } else if preview_range.end <= preview_marked_range.start {
        preview_range
    } else {
        let preview_len = preview_marked_range
            .end
            .saturating_sub(preview_marked_range.start);
        let base_len = base_marked_range
            .end
            .saturating_sub(base_marked_range.start);
        let delta = preview_len as isize - base_len as isize;
        shift_range(preview_range, -delta)
    }
}

fn shift_range(range: Range<usize>, delta: isize) -> Range<usize> {
    if delta >= 0 {
        let delta = delta as usize;
        range.start.saturating_add(delta)..range.end.saturating_add(delta)
    } else {
        let delta = (-delta) as usize;
        range.start.saturating_sub(delta)..range.end.saturating_sub(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_utf8_offsets_handle_surrogate_pairs() {
        let text = "a😀中";
        assert_eq!(utf8_to_utf16_offset(text, "a".len()), 1);
        assert_eq!(utf8_to_utf16_offset(text, "a😀".len()), 3);
        assert_eq!(utf16_to_utf8_offset(text, 1), "a".len());
        assert_eq!(utf16_to_utf8_offset(text, 3), "a😀".len());
    }

    #[test]
    fn marked_preview_range_maps_back_to_base_range() {
        assert_eq!(marked_preview_range_to_base_range(1..4, 1..2, 1..4), 1..2);
        assert_eq!(marked_preview_range_to_base_range(4..4, 1..2, 1..4), 2..2);
    }
}
