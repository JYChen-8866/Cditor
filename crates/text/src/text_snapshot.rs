use std::{ops::Range, sync::Arc};

use unicode_segmentation::UnicodeSegmentation;

const INVALID_OFFSET: usize = usize::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSnapshot {
    text: Arc<str>,
    offsets: Arc<TextOffsetMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextOffsetMap {
    byte_to_utf16: Vec<usize>,
    utf16_to_byte: Vec<usize>,
    grapheme_boundaries: Vec<usize>,
}

impl TextSnapshot {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let offsets = Arc::new(TextOffsetMap::new(&text));
        Self { text, offsets }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len_bytes(&self) -> usize {
        self.text.len()
    }

    pub fn len_utf16(&self) -> usize {
        self.offsets.utf16_to_byte.len().saturating_sub(1)
    }

    pub fn grapheme_count(&self) -> usize {
        self.offsets.grapheme_boundaries.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.text.len())
            .saturating_add(
                self.offsets
                    .byte_to_utf16
                    .len()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
            .saturating_add(
                self.offsets
                    .utf16_to_byte
                    .len()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
            .saturating_add(
                self.offsets
                    .grapheme_boundaries
                    .len()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
    }

    pub fn byte_to_utf16(&self, byte_offset: usize) -> Option<usize> {
        valid_offset(&self.offsets.byte_to_utf16, byte_offset)
    }

    pub fn utf16_to_byte(&self, utf16_offset: usize) -> Option<usize> {
        valid_offset(&self.offsets.utf16_to_byte, utf16_offset)
    }

    pub fn grapheme_to_byte(&self, grapheme_index: usize) -> Option<usize> {
        self.offsets
            .grapheme_boundaries
            .get(grapheme_index)
            .copied()
    }

    pub fn byte_to_grapheme(&self, byte_offset: usize) -> Option<usize> {
        self.offsets
            .grapheme_boundaries
            .binary_search(&byte_offset)
            .ok()
    }

    pub fn is_grapheme_boundary(&self, byte_offset: usize) -> bool {
        self.byte_to_grapheme(byte_offset).is_some()
    }

    pub fn byte_range_to_utf16(&self, range: Range<usize>) -> Option<Range<usize>> {
        valid_ordered_range(range, self.len_bytes()).and_then(|range| {
            Some(self.byte_to_utf16(range.start)?..self.byte_to_utf16(range.end)?)
        })
    }

    pub fn utf16_range_to_byte(&self, range: Range<usize>) -> Option<Range<usize>> {
        valid_ordered_range(range, self.len_utf16()).and_then(|range| {
            Some(self.utf16_to_byte(range.start)?..self.utf16_to_byte(range.end)?)
        })
    }

    pub fn grapheme_range_to_byte(&self, range: Range<usize>) -> Option<Range<usize>> {
        valid_ordered_range(range, self.grapheme_count()).and_then(|range| {
            Some(self.grapheme_to_byte(range.start)?..self.grapheme_to_byte(range.end)?)
        })
    }
}

impl TextOffsetMap {
    fn new(text: &str) -> Self {
        let utf16_len = text.encode_utf16().count();
        let mut byte_to_utf16 = vec![INVALID_OFFSET; text.len() + 1];
        let mut utf16_to_byte = vec![INVALID_OFFSET; utf16_len + 1];
        let mut utf16_offset = 0;

        for (byte_offset, ch) in text.char_indices() {
            byte_to_utf16[byte_offset] = utf16_offset;
            utf16_to_byte[utf16_offset] = byte_offset;
            utf16_offset += ch.len_utf16();
        }
        byte_to_utf16[text.len()] = utf16_len;
        utf16_to_byte[utf16_len] = text.len();

        let mut grapheme_boundaries = text
            .grapheme_indices(true)
            .map(|(byte_offset, _)| byte_offset)
            .collect::<Vec<_>>();
        if grapheme_boundaries.last().copied() != Some(text.len()) {
            grapheme_boundaries.push(text.len());
        }

        Self {
            byte_to_utf16,
            utf16_to_byte,
            grapheme_boundaries,
        }
    }
}

fn valid_offset(table: &[usize], offset: usize) -> Option<usize> {
    table
        .get(offset)
        .copied()
        .filter(|value| *value != INVALID_OFFSET)
}

fn valid_ordered_range(range: Range<usize>, maximum: usize) -> Option<Range<usize>> {
    (range.start <= range.end && range.end <= maximum).then_some(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ascii_cjk_and_surrogate_pairs_without_splitting_scalars() {
        let snapshot = TextSnapshot::new("A中😀Z");

        assert_eq!(snapshot.len_bytes(), 9);
        assert_eq!(snapshot.len_utf16(), 5);
        assert_eq!(snapshot.byte_to_utf16(0), Some(0));
        assert_eq!(snapshot.byte_to_utf16(1), Some(1));
        assert_eq!(snapshot.byte_to_utf16(4), Some(2));
        assert_eq!(snapshot.byte_to_utf16(8), Some(4));
        assert_eq!(snapshot.byte_to_utf16(9), Some(5));
        assert_eq!(snapshot.byte_to_utf16(2), None);
        assert_eq!(snapshot.utf16_to_byte(3), None);
        assert_eq!(snapshot.utf16_to_byte(4), Some(8));
    }

    #[test]
    fn grapheme_map_keeps_combining_and_emoji_zwj_sequences_atomic() {
        let text = "e\u{301}👨‍👩‍👧‍👦中";
        let snapshot = TextSnapshot::new(text);
        let first_end = "e\u{301}".len();
        let family_end = first_end + "👨‍👩‍👧‍👦".len();

        assert_eq!(snapshot.grapheme_count(), 3);
        assert_eq!(snapshot.grapheme_to_byte(0), Some(0));
        assert_eq!(snapshot.grapheme_to_byte(1), Some(first_end));
        assert_eq!(snapshot.grapheme_to_byte(2), Some(family_end));
        assert_eq!(snapshot.grapheme_to_byte(3), Some(text.len()));
        assert_eq!(snapshot.byte_to_grapheme(1), None);
        assert!(snapshot.is_grapheme_boundary(family_end));
    }

    #[test]
    fn range_conversion_rejects_reversed_out_of_bounds_and_split_offsets() {
        let snapshot = TextSnapshot::new("a😀b");

        assert_eq!(snapshot.byte_range_to_utf16(1..5), Some(1..3));
        assert_eq!(snapshot.utf16_range_to_byte(1..3), Some(1..5));
        assert_eq!(snapshot.utf16_range_to_byte(2..3), None);
        assert_eq!(snapshot.byte_range_to_utf16(2..5), None);
        let reversed_start = 5;
        let reversed_end = 1;
        assert_eq!(
            snapshot.byte_range_to_utf16(reversed_start..reversed_end),
            None
        );
        assert_eq!(snapshot.utf16_range_to_byte(0..5), None);
    }

    #[test]
    fn empty_snapshot_has_terminal_boundaries() {
        let snapshot = TextSnapshot::new("");

        assert!(snapshot.is_empty());
        assert_eq!(snapshot.grapheme_count(), 0);
        assert_eq!(snapshot.byte_to_utf16(0), Some(0));
        assert_eq!(snapshot.utf16_to_byte(0), Some(0));
        assert_eq!(snapshot.grapheme_to_byte(0), Some(0));
    }
}
