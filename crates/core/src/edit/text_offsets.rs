use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct InternalTextOffset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PlatformUtf16Offset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GraphemeIndex(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiDirection {
    Ltr,
    Rtl,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidiRun {
    pub range: Range<InternalTextOffset>,
    pub direction: BidiDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOffsetMap {
    text_len: usize,
    internal_to_utf16: Vec<(InternalTextOffset, PlatformUtf16Offset)>,
    utf16_to_internal: Vec<(PlatformUtf16Offset, InternalTextOffset)>,
    grapheme_boundaries: Vec<InternalTextOffset>,
    bidi_runs: Vec<BidiRun>,
}

impl TextOffsetMap {
    pub fn build(text: &str) -> Self {
        let mut internal_to_utf16 = vec![(InternalTextOffset(0), PlatformUtf16Offset(0))];
        let mut utf16_to_internal = vec![(PlatformUtf16Offset(0), InternalTextOffset(0))];
        let mut utf16 = 0;
        for (byte_index, ch) in text.char_indices() {
            utf16 += ch.len_utf16();
            let internal = InternalTextOffset(byte_index + ch.len_utf8());
            let platform = PlatformUtf16Offset(utf16);
            internal_to_utf16.push((internal, platform));
            utf16_to_internal.push((platform, internal));
        }

        let mut grapheme_boundaries = Vec::new();
        grapheme_boundaries.push(InternalTextOffset(0));
        for (byte_index, grapheme) in text.grapheme_indices(true) {
            let end = InternalTextOffset(byte_index + grapheme.len());
            if grapheme_boundaries.last().copied() != Some(end) {
                grapheme_boundaries.push(end);
            }
        }
        if grapheme_boundaries.last().copied() != Some(InternalTextOffset(text.len())) {
            grapheme_boundaries.push(InternalTextOffset(text.len()));
        }

        let bidi_runs = build_bidi_runs(text);

        Self {
            text_len: text.len(),
            internal_to_utf16,
            utf16_to_internal,
            grapheme_boundaries,
            bidi_runs,
        }
    }

    pub fn text_len(&self) -> usize {
        self.text_len
    }

    pub fn grapheme_boundaries(&self) -> &[InternalTextOffset] {
        &self.grapheme_boundaries
    }

    pub fn bidi_runs(&self) -> &[BidiRun] {
        &self.bidi_runs
    }

    pub fn internal_to_utf16(
        &self,
        offset: InternalTextOffset,
    ) -> Result<PlatformUtf16Offset, TextOffsetError> {
        self.internal_to_utf16
            .iter()
            .find_map(|(internal, platform)| (*internal == offset).then_some(*platform))
            .ok_or(TextOffsetError::InvalidInternalOffset(offset))
    }

    pub fn utf16_to_internal(
        &self,
        offset: PlatformUtf16Offset,
    ) -> Result<InternalTextOffset, TextOffsetError> {
        self.utf16_to_internal
            .iter()
            .find_map(|(platform, internal)| (*platform == offset).then_some(*internal))
            .ok_or(TextOffsetError::InvalidUtf16Offset(offset))
    }

    pub fn utf16_range_to_internal_range(
        &self,
        range: Range<PlatformUtf16Offset>,
    ) -> Result<Range<InternalTextOffset>, TextOffsetError> {
        let start = self.utf16_to_internal(range.start)?;
        let end = self.utf16_to_internal(range.end)?;
        self.validate_grapheme_range(start..end)?;
        Ok(start..end)
    }

    pub fn is_grapheme_boundary(&self, offset: InternalTextOffset) -> bool {
        self.grapheme_boundaries.binary_search(&offset).is_ok()
    }

    pub fn grapheme_index_of(
        &self,
        offset: InternalTextOffset,
    ) -> Result<GraphemeIndex, TextOffsetError> {
        self.grapheme_boundaries
            .binary_search(&offset)
            .map(GraphemeIndex)
            .map_err(|_| TextOffsetError::NotGraphemeBoundary(offset))
    }

    pub fn validate_grapheme_range(
        &self,
        range: Range<InternalTextOffset>,
    ) -> Result<(), TextOffsetError> {
        if range.start > range.end || range.end.0 > self.text_len {
            return Err(TextOffsetError::InvalidInternalRange(range));
        }
        if !self.is_grapheme_boundary(range.start) {
            return Err(TextOffsetError::NotGraphemeBoundary(range.start));
        }
        if !self.is_grapheme_boundary(range.end) {
            return Err(TextOffsetError::NotGraphemeBoundary(range.end));
        }
        Ok(())
    }

    pub fn previous_grapheme_boundary(
        &self,
        offset: InternalTextOffset,
    ) -> Option<InternalTextOffset> {
        self.grapheme_boundaries
            .iter()
            .copied()
            .rev()
            .find(|boundary| *boundary < offset)
    }

    pub fn next_grapheme_boundary(&self, offset: InternalTextOffset) -> Option<InternalTextOffset> {
        self.grapheme_boundaries
            .iter()
            .copied()
            .find(|boundary| *boundary > offset)
    }

    pub fn backspace_range(
        &self,
        caret: InternalTextOffset,
    ) -> Result<Option<Range<InternalTextOffset>>, TextOffsetError> {
        if !self.is_grapheme_boundary(caret) {
            return Err(TextOffsetError::NotGraphemeBoundary(caret));
        }
        Ok(self
            .previous_grapheme_boundary(caret)
            .map(|previous| previous..caret))
    }

    pub fn delete_range(
        &self,
        caret: InternalTextOffset,
    ) -> Result<Option<Range<InternalTextOffset>>, TextOffsetError> {
        if !self.is_grapheme_boundary(caret) {
            return Err(TextOffsetError::NotGraphemeBoundary(caret));
        }
        Ok(self.next_grapheme_boundary(caret).map(|next| caret..next))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOffsetError {
    InvalidInternalOffset(InternalTextOffset),
    InvalidUtf16Offset(PlatformUtf16Offset),
    InvalidInternalRange(Range<InternalTextOffset>),
    NotGraphemeBoundary(InternalTextOffset),
}

fn build_bidi_runs(text: &str) -> Vec<BidiRun> {
    let mut runs = Vec::new();
    let mut current_direction: Option<BidiDirection> = None;
    let mut current_start = 0;
    let mut last_end = 0;

    for (byte_index, ch) in text.char_indices() {
        let direction = bidi_direction(ch);
        let end = byte_index + ch.len_utf8();
        if direction == BidiDirection::Neutral {
            last_end = end;
            continue;
        }
        match current_direction {
            None => {
                current_direction = Some(direction);
                current_start = byte_index;
            }
            Some(existing) if existing == direction => {}
            Some(existing) => {
                runs.push(BidiRun {
                    range: InternalTextOffset(current_start)..InternalTextOffset(byte_index),
                    direction: existing,
                });
                current_direction = Some(direction);
                current_start = byte_index;
            }
        }
        last_end = end;
    }

    if let Some(direction) = current_direction {
        runs.push(BidiRun {
            range: InternalTextOffset(current_start)..InternalTextOffset(last_end),
            direction,
        });
    }
    runs
}

fn bidi_direction(ch: char) -> BidiDirection {
    match ch as u32 {
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF => BidiDirection::Rtl,
        value if char::from_u32(value).is_some_and(|c| c.is_alphabetic() || c.is_numeric()) => {
            BidiDirection::Ltr
        }
        _ => BidiDirection::Neutral,
    }
}
