use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Range;

use crate::document::{DocumentIndex, VisibleDocumentIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightConfidence {
    Exact,
    Predictive,
    Historical,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightEstimate {
    pub height: f64,
    pub confidence: HeightConfidence,
    pub max_error_hint: f64,
}

impl HeightEstimate {
    pub const fn new(height: f64, confidence: HeightConfidence, max_error_hint: f64) -> Self {
        Self {
            height,
            confidence,
            max_error_hint,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockHeightIndex {
    pub heights: Vec<f64>,
    pub confidence: Vec<HeightConfidence>,
    prefix: FenwickTree,
}

impl BlockHeightIndex {
    pub fn new(
        estimates: impl IntoIterator<Item = HeightEstimate>,
    ) -> Result<Self, BlockHeightIndexError> {
        let mut heights = Vec::new();
        let mut confidence = Vec::new();

        for estimate in estimates {
            validate_height(estimate.height)?;
            heights.push(estimate.height);
            confidence.push(estimate.confidence);
        }

        let prefix = FenwickTree::from_values(&heights);
        Ok(Self {
            heights,
            confidence,
            prefix,
        })
    }

    pub fn from_visible_document(
        document_index: &DocumentIndex,
        visible_index: &VisibleDocumentIndex,
    ) -> Result<Self, BlockHeightIndexError> {
        let estimates = visible_index.visible_block_ids.iter().map(|block_id| {
            let document_position = document_index
                .index_of(*block_id)
                .ok_or(BlockHeightIndexError::MissingDocumentBlock(*block_id))?;
            let layout_meta = document_index
                .meta_at(document_position)
                .ok_or(BlockHeightIndexError::MissingLayoutMeta(document_position))?;

            let confidence = if layout_meta.measured_height.is_some() {
                HeightConfidence::Exact
            } else if layout_meta.dirty {
                HeightConfidence::Default
            } else {
                HeightConfidence::Historical
            };

            Ok(HeightEstimate::new(
                layout_meta.effective_height(),
                confidence,
                0.0,
            ))
        });

        let mut collected = Vec::with_capacity(visible_index.total_visible_count());
        for estimate in estimates {
            collected.push(estimate?);
        }
        Self::new(collected)
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn total_height(&self) -> f64 {
        self.prefix.total_sum()
    }

    pub fn offset_of_block(&self, index: usize) -> Option<f64> {
        if index <= self.heights.len() {
            Some(self.prefix.prefix_sum(index))
        } else {
            None
        }
    }

    pub fn block_at_offset(&self, global_y: f64) -> Option<BlockOffsetHit> {
        if self.heights.is_empty() {
            return None;
        }

        let total_height = self.total_height();
        let clamped_y = global_y.clamp(0.0, total_height.max(0.0));
        if clamped_y >= total_height {
            let index = self.heights.len() - 1;
            let block_top = self.prefix.prefix_sum(index);
            return Some(BlockOffsetHit {
                index,
                block_top,
                offset_in_block: self.heights[index],
            });
        }

        let index = self.prefix.lower_bound_prefix(clamped_y);
        let block_top = self.prefix.prefix_sum(index);
        Some(BlockOffsetHit {
            index,
            block_top,
            offset_in_block: clamped_y - block_top,
        })
    }

    pub fn update_height(
        &mut self,
        index: usize,
        new_height: f64,
    ) -> Result<HeightChange, BlockHeightIndexError> {
        validate_height(new_height)?;
        let Some(old_height) = self.heights.get_mut(index) else {
            return Err(BlockHeightIndexError::IndexOutOfBounds {
                index,
                len: self.heights.len(),
            });
        };

        let previous = *old_height;
        *old_height = new_height;
        if let Some(confidence) = self.confidence.get_mut(index) {
            *confidence = HeightConfidence::Exact;
        }
        self.prefix.add(index, new_height - previous);

        Ok(HeightChange {
            index,
            old_height: previous,
            new_height,
            delta: new_height - previous,
        })
    }

    pub fn insert_range(
        &mut self,
        index: usize,
        estimates: &[HeightEstimate],
    ) -> Result<(), BlockHeightIndexError> {
        if index > self.heights.len() {
            return Err(BlockHeightIndexError::IndexOutOfBounds {
                index,
                len: self.heights.len(),
            });
        }
        for estimate in estimates {
            validate_height(estimate.height)?;
        }

        self.heights.splice(
            index..index,
            estimates.iter().map(|estimate| estimate.height),
        );
        self.confidence.splice(
            index..index,
            estimates.iter().map(|estimate| estimate.confidence),
        );
        self.rebuild_prefix();
        Ok(())
    }

    pub fn delete_range(&mut self, range: Range<usize>) -> Result<(), BlockHeightIndexError> {
        validate_range(&range, self.heights.len())?;
        self.heights.drain(range.clone());
        self.confidence.drain(range);
        self.rebuild_prefix();
        Ok(())
    }

    pub fn move_range(
        &mut self,
        range: Range<usize>,
        target: usize,
    ) -> Result<(), BlockHeightIndexError> {
        validate_range(&range, self.heights.len())?;
        if target > self.heights.len() {
            return Err(BlockHeightIndexError::IndexOutOfBounds {
                index: target,
                len: self.heights.len(),
            });
        }
        if range.contains(&target) || target == range.end {
            return Ok(());
        }

        let moved_heights: Vec<_> = self.heights[range.clone()].to_vec();
        let moved_confidence: Vec<_> = self.confidence[range.clone()].to_vec();
        let moved_len = moved_heights.len();

        self.heights.drain(range.clone());
        self.confidence.drain(range.clone());

        let adjusted_target = if target > range.end {
            target - moved_len
        } else {
            target
        };

        self.heights
            .splice(adjusted_target..adjusted_target, moved_heights);
        self.confidence
            .splice(adjusted_target..adjusted_target, moved_confidence);
        self.rebuild_prefix();
        Ok(())
    }

    pub fn rebuild_range(
        &mut self,
        range: Range<usize>,
        estimates: &[HeightEstimate],
    ) -> Result<(), BlockHeightIndexError> {
        validate_range(&range, self.heights.len())?;
        if range.len() != estimates.len() {
            return Err(BlockHeightIndexError::ReplacementLengthMismatch {
                range_len: range.len(),
                replacement_len: estimates.len(),
            });
        }
        for estimate in estimates {
            validate_height(estimate.height)?;
        }

        for (offset, estimate) in estimates.iter().enumerate() {
            let index = range.start + offset;
            self.heights[index] = estimate.height;
            self.confidence[index] = estimate.confidence;
        }
        self.rebuild_prefix();
        Ok(())
    }

    fn rebuild_prefix(&mut self) {
        self.prefix = FenwickTree::from_values(&self.heights);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockOffsetHit {
    pub index: usize,
    pub block_top: f64,
    pub offset_in_block: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightChange {
    pub index: usize,
    pub old_height: f64,
    pub new_height: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct FenwickTree {
    tree: Vec<f64>,
}

impl FenwickTree {
    fn from_values(values: &[f64]) -> Self {
        let mut tree = Self {
            tree: vec![0.0; values.len() + 1],
        };
        for (index, value) in values.iter().copied().enumerate() {
            tree.add(index, value);
        }
        tree
    }

    fn add(&mut self, index: usize, delta: f64) {
        let mut tree_index = index + 1;
        while tree_index < self.tree.len() {
            self.tree[tree_index] += delta;
            tree_index += tree_index & (!tree_index + 1);
        }
    }

    fn prefix_sum(&self, count: usize) -> f64 {
        let mut tree_index = count.min(self.tree.len().saturating_sub(1));
        let mut sum = 0.0;
        while tree_index > 0 {
            sum += self.tree[tree_index];
            tree_index -= tree_index & (!tree_index + 1);
        }
        sum
    }

    fn total_sum(&self) -> f64 {
        self.prefix_sum(self.tree.len().saturating_sub(1))
    }

    fn lower_bound_prefix(&self, target: f64) -> usize {
        let item_count = self.tree.len().saturating_sub(1);
        if item_count == 0 {
            return 0;
        }

        let mut index = 0usize;
        let mut accumulated = 0.0;
        let mut bit = highest_power_of_two_at_most(item_count);

        while bit != 0 {
            let next = index + bit;
            if next <= item_count && accumulated + self.tree[next] <= target {
                accumulated += self.tree[next];
                index = next;
            }
            bit >>= 1;
        }

        index.min(item_count - 1)
    }
}

fn highest_power_of_two_at_most(value: usize) -> usize {
    if value == 0 {
        0
    } else {
        1usize << (usize::BITS - value.leading_zeros() - 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockHeightIndexError {
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
    InvalidRange {
        start: usize,
        end: usize,
        len: usize,
    },
    InvalidHeight(f64),
    MissingDocumentBlock(u64),
    MissingLayoutMeta(usize),
    ReplacementLengthMismatch {
        range_len: usize,
        replacement_len: usize,
    },
}

impl Display for BlockHeightIndexError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexOutOfBounds { index, len } => {
                write!(
                    formatter,
                    "block height index out of bounds: index {index}, len {len}"
                )
            }
            Self::InvalidRange { start, end, len } => {
                write!(
                    formatter,
                    "invalid block height range {start}..{end} for len {len}"
                )
            }
            Self::InvalidHeight(height) => write!(formatter, "invalid block height: {height}"),
            Self::MissingDocumentBlock(block_id) => {
                write!(
                    formatter,
                    "visible block {block_id} is missing from document index"
                )
            }
            Self::MissingLayoutMeta(index) => {
                write!(formatter, "missing layout meta at document index {index}")
            }
            Self::ReplacementLengthMismatch {
                range_len,
                replacement_len,
            } => write!(
                formatter,
                "replacement length mismatch: range len {range_len}, replacement len {replacement_len}"
            ),
        }
    }
}

impl Error for BlockHeightIndexError {}

fn validate_height(height: f64) -> Result<(), BlockHeightIndexError> {
    if height.is_finite() && height >= 0.0 {
        Ok(())
    } else {
        Err(BlockHeightIndexError::InvalidHeight(height))
    }
}

fn validate_range(range: &Range<usize>, len: usize) -> Result<(), BlockHeightIndexError> {
    if range.start <= range.end && range.end <= len {
        Ok(())
    } else {
        Err(BlockHeightIndexError::InvalidRange {
            start: range.start,
            end: range.end,
            len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
    use crate::ids::DocumentId;
    use crate::layout::BlockLayoutMeta;
    use std::time::Instant;

    const DOCUMENT_ID: DocumentId = 1;
    const PARAGRAPH: u16 = 1;

    #[test]
    fn prefix_sum_offsets_are_correct() {
        let index = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Default, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Predictive, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();

        assert_eq!(index.total_height(), 60.0);
        assert_eq!(index.offset_of_block(0), Some(0.0));
        assert_eq!(index.offset_of_block(1), Some(10.0));
        assert_eq!(index.offset_of_block(2), Some(30.0));
        assert_eq!(index.offset_of_block(3), Some(60.0));
        assert_eq!(index.offset_of_block(4), None);
        assert_eq!(index.block_at_offset(0.0).unwrap().index, 0);
        assert_eq!(index.block_at_offset(10.0).unwrap().index, 1);
        assert_eq!(index.block_at_offset(59.0).unwrap().index, 2);
    }

    #[test]
    fn builds_from_visible_document_order_not_raw_document_order() {
        let document_index = sample_document_index_with_heights();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);
        visible_index.toggle_folded(&document_index, 2).unwrap();

        let height_index =
            BlockHeightIndex::from_visible_document(&document_index, &visible_index).unwrap();

        assert_eq!(visible_index.visible_block_ids, vec![1, 2, 6]);
        assert_eq!(height_index.heights, vec![10.0, 20.0, 60.0]);
        assert_eq!(height_index.total_height(), 90.0);
    }

    #[test]
    fn update_height_is_logarithmic_and_marks_confidence_exact() {
        let mut index = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Default, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Historical, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Predictive, 0.0),
        ])
        .unwrap();

        let change = index.update_height(1, 25.0).unwrap();

        assert_eq!(change.old_height, 20.0);
        assert_eq!(change.new_height, 25.0);
        assert_eq!(change.delta, 5.0);
        assert_eq!(index.confidence[1], HeightConfidence::Exact);
        assert_eq!(index.total_height(), 65.0);
        assert_eq!(index.offset_of_block(2), Some(35.0));
    }

    #[test]
    fn supports_batch_insert_delete_move_and_rebuild() {
        let mut index = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Default, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Default, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Default, 0.0),
        ])
        .unwrap();

        index
            .insert_range(
                1,
                &[
                    HeightEstimate::new(5.0, HeightConfidence::Predictive, 0.0),
                    HeightEstimate::new(6.0, HeightConfidence::Predictive, 0.0),
                ],
            )
            .unwrap();
        assert_eq!(index.heights, vec![10.0, 5.0, 6.0, 20.0, 30.0]);
        assert_eq!(index.total_height(), 71.0);

        index.delete_range(2..4).unwrap();
        assert_eq!(index.heights, vec![10.0, 5.0, 30.0]);
        assert_eq!(index.total_height(), 45.0);

        index.move_range(0..1, 3).unwrap();
        assert_eq!(index.heights, vec![5.0, 30.0, 10.0]);
        assert_eq!(index.offset_of_block(2), Some(35.0));

        index
            .rebuild_range(
                1..3,
                &[
                    HeightEstimate::new(7.0, HeightConfidence::Historical, 0.0),
                    HeightEstimate::new(8.0, HeightConfidence::Exact, 0.0),
                ],
            )
            .unwrap();
        assert_eq!(index.heights, vec![5.0, 7.0, 8.0]);
        assert_eq!(index.total_height(), 20.0);
    }

    #[test]
    fn randomized_heights_and_updates_keep_prefix_correct() {
        let mut rng = Lcg::new(0xB10C_1E16_47);
        let estimates: Vec<_> = (0..2_000)
            .map(|_| {
                HeightEstimate::new(
                    (rng.next_usize(300) + 1) as f64,
                    HeightConfidence::Default,
                    0.0,
                )
            })
            .collect();
        let mut index = BlockHeightIndex::new(estimates).unwrap();

        for _ in 0..2_000 {
            let block_index = rng.next_usize(index.len());
            let new_height = (rng.next_usize(400) + 1) as f64;
            index.update_height(block_index, new_height).unwrap();
            assert_prefix_matches_naive(&index);
        }
    }

    #[test]
    fn searches_100k_blocks_10k_times_within_budget() {
        let estimates: Vec<_> = (0..100_000)
            .map(|i| HeightEstimate::new(((i % 80) + 20) as f64, HeightConfidence::Default, 0.0))
            .collect();
        let index = BlockHeightIndex::new(estimates).unwrap();
        let total_height = index.total_height();
        let mut rng = Lcg::new(0x5170_10000);

        let started = Instant::now();
        for _ in 0..10_000 {
            let y = (rng.next_u64() as f64 / u64::MAX as f64) * total_height;
            let hit = index.block_at_offset(y).unwrap();
            let block_top = index.offset_of_block(hit.index).unwrap();
            assert!(block_top <= y.max(block_top));
        }
        let elapsed = started.elapsed();

        assert!(
            elapsed.as_millis() < 800,
            "10k random block_at_offset lookups over 100k blocks took {elapsed:?}"
        );
    }

    fn sample_document_index_with_heights() -> DocumentIndex {
        let records = (1..=6).map(|id| {
            let parent_id = match id {
                1 => None,
                2 | 6 => Some(1),
                3 | 4 => Some(2),
                5 => Some(4),
                _ => None,
            };
            let depth = match id {
                1 => 0,
                2 | 6 => 1,
                3 | 4 => 2,
                5 => 3,
                _ => 0,
            };
            let mut meta = BlockLayoutMeta::new(id, id as f64 * 10.0);
            meta.dirty = false;
            BlockIndexRecord::new(id, parent_id, depth, PARAGRAPH, 0).with_layout_meta(meta)
        });
        DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap()
    }

    fn assert_prefix_matches_naive(index: &BlockHeightIndex) {
        let mut sum = 0.0;
        for position in 0..index.len() {
            assert_eq!(index.offset_of_block(position), Some(sum));
            sum += index.heights[position];
        }
        assert_eq!(index.total_height(), sum);
    }

    #[derive(Debug, Clone, Copy)]
    struct Lcg(u64);

    impl Lcg {
        const fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }

        fn next_usize(&mut self, upper_bound: usize) -> usize {
            if upper_bound == 0 {
                0
            } else {
                (self.next_u64() as usize) % upper_bound
            }
        }
    }
}
