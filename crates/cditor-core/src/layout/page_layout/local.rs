use std::ops::Range;

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct PageLocalHeightIndex {
    pub page_index: usize,
    pub block_start: usize,
    pub heights: Vec<f64>,
    pub confidence: Vec<HeightConfidence>,
    pub max_error_hints: Vec<f64>,
    prefix: PageHeightFenwick,
    dirty_range: Option<Range<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageLocalBlockOffsetHit {
    pub local_index: usize,
    pub global_block_index: usize,
    pub block_top: f64,
    pub offset_in_block: f64,
}

impl PageLocalHeightIndex {
    pub(super) fn new(
        page_index: usize,
        block_start: usize,
        heights: Vec<f64>,
        confidence: Vec<HeightConfidence>,
        max_error_hints: Vec<f64>,
    ) -> Self {
        let prefix = PageHeightFenwick::from_values(&heights);
        Self {
            page_index,
            block_start,
            heights,
            confidence,
            max_error_hints,
            prefix,
            dirty_range: None,
        }
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn total_height(&self) -> f64 {
        self.prefix.total_sum()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn range_total_height(&self, range: Range<usize>) -> Option<f64> {
        (range.start <= range.end && range.end <= self.len())
            .then(|| self.prefix.prefix_sum(range.end) - self.prefix.prefix_sum(range.start))
    }

    pub fn dirty_range(&self) -> Option<Range<usize>> {
        self.dirty_range.clone()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty_range = None;
    }

    pub fn offset_of_block(&self, local_index: usize) -> Option<f64> {
        (local_index <= self.heights.len()).then(|| self.prefix.prefix_sum(local_index))
    }

    pub fn block_at_offset(&self, local_y: f64) -> Option<PageLocalBlockOffsetHit> {
        if self.heights.is_empty() {
            return None;
        }
        let total_height = self.total_height();
        let clamped_y = local_y.clamp(0.0, total_height.max(0.0));
        let local_index = if clamped_y >= total_height {
            self.heights.len() - 1
        } else {
            self.prefix.lower_bound_prefix(clamped_y)
        };
        let block_top = self.prefix.prefix_sum(local_index);
        Some(PageLocalBlockOffsetHit {
            local_index,
            global_block_index: self.block_start + local_index,
            block_top,
            offset_in_block: (clamped_y - block_top).clamp(0.0, self.heights[local_index]),
        })
    }

    pub fn update_height(
        &mut self,
        local_index: usize,
        new_height: f64,
    ) -> Result<PageHeightChange, PageLayoutIndexError> {
        validate_height(new_height)?;
        let Some(old_height) = self.heights.get_mut(local_index) else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page: self.page_index,
                len: self.heights.len(),
            });
        };
        let previous = *old_height;
        *old_height = new_height;
        self.confidence[local_index] = HeightConfidence::Exact;
        self.max_error_hints[local_index] = 0.0;
        let delta = new_height - previous;
        self.prefix.add(local_index, delta);
        self.mark_dirty(local_index..local_index + 1);
        Ok(PageHeightChange {
            page: self.page_index,
            old_height: previous,
            new_height,
            delta,
        })
    }

    pub fn update_heights(
        &mut self,
        range: Range<usize>,
        heights: &[f64],
    ) -> Result<f64, PageLayoutIndexError> {
        if range.start > range.end || range.end > self.len() {
            return Err(PageLayoutIndexError::InvalidLocalRange {
                start: range.start,
                end: range.end,
                len: self.len(),
            });
        }
        if range.len() != heights.len() {
            return Err(PageLayoutIndexError::LocalReplacementLengthMismatch {
                range_len: range.len(),
                replacement_len: heights.len(),
            });
        }
        for height in heights {
            validate_height(*height)?;
        }

        let mut total_delta = 0.0;
        for (offset, new_height) in heights.iter().copied().enumerate() {
            let local_index = range.start + offset;
            let old_height = self.heights[local_index];
            self.heights[local_index] = new_height;
            self.confidence[local_index] = HeightConfidence::Exact;
            self.max_error_hints[local_index] = 0.0;
            let delta = new_height - old_height;
            total_delta += delta;
            self.prefix.add(local_index, delta);
        }
        if !range.is_empty() {
            self.mark_dirty(range);
        }
        Ok(total_delta)
    }

    fn mark_dirty(&mut self, range: Range<usize>) {
        self.dirty_range = Some(match self.dirty_range.take() {
            Some(current) => current.start.min(range.start)..current.end.max(range.end),
            None => range,
        });
    }
}
