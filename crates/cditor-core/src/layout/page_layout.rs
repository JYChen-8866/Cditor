use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Range;

use crate::layout::{BlockHeightIndex, HeightConfidence};

pub const DEFAULT_MAX_PAGE_BLOCKS: usize = 1_000;
pub const DEFAULT_TARGET_PAGE_HEIGHT: f64 = 30_000.0;
pub const PAGE_POLICY_VERSION: u64 = 1;

mod cache;
mod local;
mod mutation;

pub use cache::{CachedPageMismatchPolicy, CachedPageRestore};
pub use local::{PageLocalBlockOffsetHit, PageLocalHeightIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageLayoutIdentity {
    pub document_id: u64,
    pub structure_version: u64,
    pub visibility_version: u64,
    pub layout_key_hash: u64,
    pub page_policy_version: u64,
    pub page_index: usize,
}

impl PageLayoutIdentity {
    pub fn for_page(
        document_id: u64,
        structure_version: u64,
        visibility_version: u64,
        layout_key_hash: u64,
        page_policy_version: u64,
        page_index: usize,
    ) -> Self {
        Self {
            document_id,
            structure_version,
            visibility_version,
            layout_key_hash,
            page_policy_version,
            page_index,
        }
    }

    pub fn same_layout_context(&self, other: &Self) -> bool {
        self.document_id == other.document_id
            && self.structure_version == other.structure_version
            && self.visibility_version == other.visibility_version
            && self.layout_key_hash == other.layout_key_hash
            && self.page_policy_version == other.page_policy_version
    }

    pub fn for_page_index(self, page_index: usize) -> Self {
        Self { page_index, ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageLayout {
    pub page_index: usize,
    pub block_start: usize,
    pub block_count: usize,
    pub height: f64,
    pub measured_ratio: f32,
    pub confidence: HeightConfidence,
    pub max_error_hint: f64,
    pub dirty: bool,
}

impl PageLayout {
    pub fn block_end(&self) -> usize {
        self.block_start + self.block_count
    }

    pub fn contains_block_index(&self, block_index: usize) -> bool {
        self.block_start <= block_index && block_index < self.block_end()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagePolicy {
    pub max_blocks: usize,
    pub target_height: f64,
    pub max_estimated_cost: u32,
    pub max_text_bytes: usize,
    pub max_inline_runs: usize,
    pub max_complex_blocks: usize,
}

impl Default for PagePolicy {
    fn default() -> Self {
        Self {
            max_blocks: DEFAULT_MAX_PAGE_BLOCKS,
            target_height: DEFAULT_TARGET_PAGE_HEIGHT,
            max_estimated_cost: 10_000,
            max_text_bytes: 256 * 1024,
            max_inline_runs: 20_000,
            max_complex_blocks: 32,
        }
    }
}

impl PagePolicy {
    pub fn validate(&self) -> Result<(), PageLayoutIndexError> {
        if self.max_blocks == 0 {
            return Err(PageLayoutIndexError::InvalidPagePolicy(
                "max_blocks must be > 0",
            ));
        }
        if !self.target_height.is_finite() || self.target_height <= 0.0 {
            return Err(PageLayoutIndexError::InvalidPagePolicy(
                "target_height must be finite and > 0",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageBlockEstimate {
    pub height: f64,
    pub confidence: HeightConfidence,
    pub max_error_hint: f64,
    pub estimated_cost: u32,
    pub text_bytes: usize,
    pub inline_runs: usize,
    pub is_complex: bool,
}

impl PageBlockEstimate {
    pub const fn simple(height: f64, confidence: HeightConfidence) -> Self {
        Self {
            height,
            confidence,
            max_error_hint: 0.0,
            estimated_cost: 1,
            text_bytes: 0,
            inline_runs: 0,
            is_complex: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayoutIndex {
    pub identity: Option<PageLayoutIdentity>,
    pub policy: PagePolicy,
    pub pages: Vec<PageLayout>,
    height_index: PageHeightFenwick,
}

impl PageLayoutIndex {
    pub fn from_block_height_index(
        block_height_index: &BlockHeightIndex,
        policy: PagePolicy,
    ) -> Result<Self, PageLayoutIndexError> {
        let estimates = block_height_index
            .heights
            .iter()
            .copied()
            .zip(block_height_index.confidence.iter().copied())
            .map(|(height, confidence)| PageBlockEstimate::simple(height, confidence));
        Self::from_block_estimates(estimates, policy)
    }

    pub fn from_block_estimates(
        estimates: impl IntoIterator<Item = PageBlockEstimate>,
        policy: PagePolicy,
    ) -> Result<Self, PageLayoutIndexError> {
        policy.validate()?;
        let estimates: Vec<_> = estimates.into_iter().collect();
        for estimate in &estimates {
            validate_height(estimate.height)?;
        }

        let mut pages = Vec::new();
        let mut start = 0usize;
        let mut count = 0usize;
        let mut height = 0.0;
        let mut measured = 0usize;
        let mut max_error_hint = 0.0;
        let mut confidence = HeightConfidence::Exact;
        let mut estimated_cost = 0u32;
        let mut text_bytes = 0usize;
        let mut inline_runs = 0usize;
        let mut complex_blocks = 0usize;

        for (block_index, estimate) in estimates.iter().copied().enumerate() {
            let would_exceed = count > 0
                && (count + 1 > policy.max_blocks
                    || height + estimate.height > policy.target_height
                    || estimated_cost.saturating_add(estimate.estimated_cost)
                        > policy.max_estimated_cost
                    || text_bytes.saturating_add(estimate.text_bytes) > policy.max_text_bytes
                    || inline_runs.saturating_add(estimate.inline_runs) > policy.max_inline_runs
                    || complex_blocks + usize::from(estimate.is_complex)
                        > policy.max_complex_blocks);

            if would_exceed {
                pages.push(build_page(
                    pages.len(),
                    start,
                    count,
                    height,
                    measured,
                    confidence,
                    max_error_hint,
                ));
                start = block_index;
                count = 0;
                height = 0.0;
                measured = 0;
                max_error_hint = 0.0;
                confidence = HeightConfidence::Exact;
                estimated_cost = 0;
                text_bytes = 0;
                inline_runs = 0;
                complex_blocks = 0;
            }

            count += 1;
            height += estimate.height;
            measured += usize::from(estimate.confidence == HeightConfidence::Exact);
            max_error_hint += estimate.max_error_hint;
            confidence = aggregate_confidence(confidence, estimate.confidence);
            estimated_cost = estimated_cost.saturating_add(estimate.estimated_cost);
            text_bytes = text_bytes.saturating_add(estimate.text_bytes);
            inline_runs = inline_runs.saturating_add(estimate.inline_runs);
            complex_blocks += usize::from(estimate.is_complex);
        }

        if count > 0 {
            pages.push(build_page(
                pages.len(),
                start,
                count,
                height,
                measured,
                confidence,
                max_error_hint,
            ));
        }

        let height_index = PageHeightFenwick::from_pages(&pages);
        Ok(Self {
            identity: None,
            policy,
            pages,
            height_index,
        })
    }

    pub fn from_cached_pages(
        pages: Vec<PageLayout>,
        policy: PagePolicy,
        total_visible_blocks: usize,
    ) -> Result<Self, PageLayoutIndexError> {
        policy.validate()?;
        for (expected_index, page) in pages.iter().enumerate() {
            if page.page_index != expected_index {
                return Err(PageLayoutIndexError::CachedPageIndexMismatch {
                    expected: expected_index,
                    actual: page.page_index,
                });
            }
            validate_height(page.height)?;
            if !page.measured_ratio.is_finite() || !(0.0..=1.0).contains(&page.measured_ratio) {
                return Err(PageLayoutIndexError::InvalidMeasuredRatio(
                    page.measured_ratio,
                ));
            }
            if !page.max_error_hint.is_finite() || page.max_error_hint < 0.0 {
                return Err(PageLayoutIndexError::InvalidMaxErrorHint(
                    page.max_error_hint,
                ));
            }
            if page.block_count == 0 {
                return Err(PageLayoutIndexError::EmptyCachedPage {
                    page: page.page_index,
                });
            }
        }
        let height_index = PageHeightFenwick::from_pages(&pages);
        let index = Self {
            identity: None,
            policy,
            pages,
            height_index,
        };
        index.validate_covers_blocks(total_visible_blocks)?;
        Ok(index)
    }

    pub fn with_identity(mut self, identity: PageLayoutIdentity) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn total_height(&self) -> f64 {
        self.height_index.total_sum()
    }

    pub fn offset_of_page(&self, page: usize) -> Option<f64> {
        if page <= self.pages.len() {
            Some(self.height_index.prefix_sum(page))
        } else {
            None
        }
    }

    pub fn page_bounds(&self, page: usize) -> Option<Range<f64>> {
        let top = self.offset_of_page(page)?;
        let layout = self.pages.get(page)?;
        Some(top..top + layout.height)
    }

    pub fn page_at_offset(&self, global_y: f64) -> Option<PageOffsetHit> {
        if self.pages.is_empty() {
            return None;
        }

        let total_height = self.total_height();
        let clamped_y = global_y.clamp(0.0, total_height.max(0.0));
        if clamped_y >= total_height {
            let page_index = self.pages.len() - 1;
            let page_top = self.height_index.prefix_sum(page_index);
            return Some(PageOffsetHit {
                page_index,
                page_top,
                offset_in_page: self.pages[page_index].height,
            });
        }

        let page_index = self.height_index.lower_bound_prefix(clamped_y);
        let page_top = self.height_index.prefix_sum(page_index);
        Some(PageOffsetHit {
            page_index,
            page_top,
            offset_in_page: clamped_y - page_top,
        })
    }

    pub fn page_layout_identity(&self) -> Option<PageLayoutIdentity> {
        self.identity
    }

    pub fn page_identity(&self, page: usize) -> Option<PageLayoutIdentity> {
        self.pages
            .get(page)
            .and_then(|_| self.identity.map(|identity| identity.for_page_index(page)))
    }

    pub fn assert_identity_matches(
        &self,
        identity: PageLayoutIdentity,
    ) -> Result<(), PageLayoutIndexError> {
        match self.identity {
            Some(current)
                if current.same_layout_context(&identity)
                    && identity.page_index < self.pages.len() =>
            {
                Ok(())
            }
            Some(current) if !current.same_layout_context(&identity) => {
                Err(PageLayoutIndexError::PageIdentityMismatch {
                    cached: current,
                    expected: identity,
                })
            }
            Some(_) => Err(PageLayoutIndexError::PageOutOfBounds {
                page: identity.page_index,
                len: self.pages.len(),
            }),
            None => Err(PageLayoutIndexError::MissingPageIdentity),
        }
    }

    pub fn page_summary(&self, page: usize) -> Option<PageSummary> {
        self.pages.get(page).copied().map(PageSummary::from)
    }

    pub fn page_summaries(&self) -> impl Iterator<Item = PageSummary> + '_ {
        self.pages.iter().copied().map(PageSummary::from)
    }

    pub fn local_height_index_from_global(
        &self,
        page: usize,
        global: &BlockHeightIndex,
    ) -> Result<PageLocalHeightIndex, PageLayoutIndexError> {
        let Some(summary) = self.page_summary(page) else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page,
                len: self.pages.len(),
            });
        };
        if summary.block_end() > global.len() {
            return Err(PageLayoutIndexError::CoverageTailMismatch {
                covered: global.len(),
                total: summary.block_end(),
            });
        }
        self.local_height_index(
            page,
            &global.heights[summary.block_start..summary.block_end()],
            &global.confidence[summary.block_start..summary.block_end()],
        )
    }

    pub fn local_height_index(
        &self,
        page: usize,
        heights: &[f64],
        confidence: &[HeightConfidence],
    ) -> Result<PageLocalHeightIndex, PageLayoutIndexError> {
        let Some(summary) = self.page_summary(page) else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page,
                len: self.pages.len(),
            });
        };
        if heights.len() != summary.block_count || confidence.len() != summary.block_count {
            return Err(PageLayoutIndexError::CoverageTailMismatch {
                covered: heights.len().min(confidence.len()),
                total: summary.block_count,
            });
        }
        for height in heights {
            validate_height(*height)?;
        }
        let non_exact_count = confidence
            .iter()
            .filter(|confidence| **confidence != HeightConfidence::Exact)
            .count();
        let error_per_non_exact = if non_exact_count == 0 {
            0.0
        } else {
            summary.max_error_hint / non_exact_count as f64
        };
        Ok(PageLocalHeightIndex::new(
            summary.page_index,
            summary.block_start,
            heights.to_vec(),
            confidence.to_vec(),
            confidence
                .iter()
                .map(|confidence| {
                    if *confidence == HeightConfidence::Exact {
                        0.0
                    } else {
                        error_per_non_exact
                    }
                })
                .collect(),
        ))
    }

    pub fn rebuild_from_block_height_index(
        &mut self,
        block_height_index: &BlockHeightIndex,
    ) -> Result<(), PageLayoutIndexError> {
        let identity = self.identity;
        let rebuilt = Self::from_block_height_index(block_height_index, self.policy)?;
        self.pages = rebuilt.pages;
        self.height_index = rebuilt.height_index;
        self.identity = identity;
        Ok(())
    }

    pub fn synchronize_page_from_local(
        &mut self,
        local: &PageLocalHeightIndex,
    ) -> Result<PageHeightChange, PageLayoutIndexError> {
        let page = local.page_index;
        let Some(layout) = self.pages.get(page) else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page,
                len: self.pages.len(),
            });
        };
        if layout.block_start != local.block_start || layout.block_count != local.len() {
            return Err(PageLayoutIndexError::CoverageGapOrOverlap {
                expected_start: layout.block_start,
                actual_start: local.block_start,
            });
        }
        let exact = local
            .confidence
            .iter()
            .filter(|confidence| **confidence == HeightConfidence::Exact)
            .count();
        let old_height = layout.height;
        let new_height = local.total_height();
        let layout = &mut self.pages[page];
        layout.height = new_height;
        layout.measured_ratio = exact as f32 / local.len().max(1) as f32;
        layout.confidence = local
            .confidence
            .iter()
            .copied()
            .fold(HeightConfidence::Exact, aggregate_confidence);
        layout.dirty =
            local.dirty_range().is_some() || layout.confidence != HeightConfidence::Exact;
        layout.max_error_hint = local.max_error_hints.iter().sum();
        self.height_index.add(page, new_height - old_height);
        Ok(PageHeightChange {
            page,
            old_height,
            new_height,
            delta: new_height - old_height,
        })
    }

    pub fn update_page_height(
        &mut self,
        page: usize,
        new_height: f64,
    ) -> Result<PageHeightChange, PageLayoutIndexError> {
        validate_height(new_height)?;
        let Some(layout) = self.pages.get_mut(page) else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page,
                len: self.pages.len(),
            });
        };

        let old_height = layout.height;
        layout.height = new_height;
        layout.confidence = HeightConfidence::Exact;
        layout.measured_ratio = 1.0;
        layout.max_error_hint = 0.0;
        layout.dirty = false;
        self.height_index.add(page, new_height - old_height);

        Ok(PageHeightChange {
            page,
            old_height,
            new_height,
            delta: new_height - old_height,
        })
    }

    pub fn page_for_block_index(&self, block_index: usize) -> Option<usize> {
        let mut low = 0usize;
        let mut high = self.pages.len();
        while low < high {
            let mid = (low + high) / 2;
            let page = &self.pages[mid];
            if block_index < page.block_start {
                high = mid;
            } else if block_index >= page.block_end() {
                low = mid + 1;
            } else {
                return Some(mid);
            }
        }
        None
    }

    pub fn validate_covers_blocks(
        &self,
        total_visible_blocks: usize,
    ) -> Result<(), PageLayoutIndexError> {
        let mut expected_start = 0usize;
        for page in &self.pages {
            if page.block_start != expected_start {
                return Err(PageLayoutIndexError::CoverageGapOrOverlap {
                    expected_start,
                    actual_start: page.block_start,
                });
            }
            expected_start = expected_start.checked_add(page.block_count).ok_or(
                PageLayoutIndexError::CoverageOverflow {
                    page: page.page_index,
                },
            )?;
        }
        if expected_start != total_visible_blocks {
            return Err(PageLayoutIndexError::CoverageTailMismatch {
                covered: expected_start,
                total: total_visible_blocks,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageOffsetHit {
    pub page_index: usize,
    pub page_top: f64,
    pub offset_in_page: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageHeightChange {
    pub page: usize,
    pub old_height: f64,
    pub new_height: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSummary {
    pub page_index: usize,
    pub block_start: usize,
    pub block_count: usize,
    pub height: f64,
    pub measured_ratio: f32,
    pub confidence: HeightConfidence,
    pub max_error_hint: f64,
    pub dirty: bool,
}

impl PageSummary {
    pub fn block_end(&self) -> usize {
        self.block_start + self.block_count
    }
}

impl From<PageLayout> for PageSummary {
    fn from(layout: PageLayout) -> Self {
        Self {
            page_index: layout.page_index,
            block_start: layout.block_start,
            block_count: layout.block_count,
            height: layout.height,
            measured_ratio: layout.measured_ratio,
            confidence: layout.confidence,
            max_error_hint: layout.max_error_hint,
            dirty: layout.dirty,
        }
    }
}

fn build_page(
    page_index: usize,
    block_start: usize,
    block_count: usize,
    height: f64,
    measured: usize,
    confidence: HeightConfidence,
    max_error_hint: f64,
) -> PageLayout {
    PageLayout {
        page_index,
        block_start,
        block_count,
        height,
        measured_ratio: if block_count == 0 {
            0.0
        } else {
            measured as f32 / block_count as f32
        },
        confidence,
        max_error_hint,
        dirty: confidence != HeightConfidence::Exact,
    }
}

fn aggregate_confidence(a: HeightConfidence, b: HeightConfidence) -> HeightConfidence {
    match (confidence_rank(a), confidence_rank(b)) {
        (a_rank, b_rank) if a_rank <= b_rank => a,
        _ => b,
    }
}

fn confidence_rank(confidence: HeightConfidence) -> u8 {
    match confidence {
        HeightConfidence::Default => 0,
        HeightConfidence::Historical => 1,
        HeightConfidence::Predictive => 2,
        HeightConfidence::Exact => 3,
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PageHeightFenwick {
    tree: Vec<f64>,
}

impl PageHeightFenwick {
    fn from_pages(pages: &[PageLayout]) -> Self {
        Self::from_values(&pages.iter().map(|page| page.height).collect::<Vec<_>>())
    }

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
pub enum PageLayoutIndexError {
    InvalidPagePolicy(&'static str),
    InvalidHeight(f64),
    PageOutOfBounds {
        page: usize,
        len: usize,
    },
    CoverageGapOrOverlap {
        expected_start: usize,
        actual_start: usize,
    },
    CoverageTailMismatch {
        covered: usize,
        total: usize,
    },
    CachedPageIndexMismatch {
        expected: usize,
        actual: usize,
    },
    EmptyCachedPage {
        page: usize,
    },
    CoverageOverflow {
        page: usize,
    },
    InvalidMeasuredRatio(f32),
    InvalidMaxErrorHint(f64),
    MissingPageIdentity,
    PageIdentityMismatch {
        cached: PageLayoutIdentity,
        expected: PageLayoutIdentity,
    },
    InvalidLocalRange {
        start: usize,
        end: usize,
        len: usize,
    },
    LocalReplacementLengthMismatch {
        range_len: usize,
        replacement_len: usize,
    },
    InvalidPageSplit {
        page: usize,
        split_at: usize,
        block_count: usize,
    },
}

impl Display for PageLayoutIndexError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPagePolicy(message) => write!(formatter, "invalid page policy: {message}"),
            Self::InvalidHeight(height) => write!(formatter, "invalid page height: {height}"),
            Self::PageOutOfBounds { page, len } => {
                write!(
                    formatter,
                    "page index out of bounds: page {page}, len {len}"
                )
            }
            Self::CoverageGapOrOverlap {
                expected_start,
                actual_start,
            } => write!(
                formatter,
                "page coverage gap/overlap: expected block_start {expected_start}, actual {actual_start}"
            ),
            Self::CoverageTailMismatch { covered, total } => write!(
                formatter,
                "page coverage tail mismatch: covered {covered}, total {total}"
            ),
            Self::CachedPageIndexMismatch { expected, actual } => write!(
                formatter,
                "cached page index mismatch: expected {expected}, actual {actual}"
            ),
            Self::EmptyCachedPage { page } => {
                write!(formatter, "cached page {page} contains no blocks")
            }
            Self::CoverageOverflow { page } => {
                write!(
                    formatter,
                    "cached page {page} block coverage overflows usize"
                )
            }
            Self::InvalidMeasuredRatio(ratio) => {
                write!(formatter, "invalid measured ratio: {ratio}")
            }
            Self::InvalidMaxErrorHint(error) => {
                write!(formatter, "invalid max error hint: {error}")
            }
            Self::MissingPageIdentity => write!(formatter, "page layout identity is missing"),
            Self::PageIdentityMismatch { cached, expected } => write!(
                formatter,
                "page layout identity mismatch: cached {cached:?}, expected {expected:?}"
            ),
            Self::InvalidLocalRange { start, end, len } => {
                write!(
                    formatter,
                    "invalid page-local range {start}..{end} for len {len}"
                )
            }
            Self::LocalReplacementLengthMismatch {
                range_len,
                replacement_len,
            } => write!(
                formatter,
                "page-local replacement length mismatch: range {range_len}, replacement {replacement_len}"
            ),
            Self::InvalidPageSplit {
                page,
                split_at,
                block_count,
            } => write!(
                formatter,
                "invalid split for page {page}: split {split_at}, block count {block_count}"
            ),
        }
    }
}

impl Error for PageLayoutIndexError {}

fn validate_height(height: f64) -> Result<(), PageLayoutIndexError> {
    if height.is_finite() && height >= 0.0 {
        Ok(())
    } else {
        Err(PageLayoutIndexError::InvalidHeight(height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::HeightEstimate;

    #[test]
    fn last_page_can_be_smaller_than_page_size() {
        let height_index = BlockHeightIndex::new(
            (0..25).map(|_| HeightEstimate::new(10.0, HeightConfidence::Default, 0.0)),
        )
        .unwrap();
        let page_index = PageLayoutIndex::from_block_height_index(
            &height_index,
            PagePolicy {
                max_blocks: 10,
                target_height: 1_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();

        assert_eq!(page_index.page_count(), 3);
        assert_eq!(page_index.pages[0].block_start, 0);
        assert_eq!(page_index.pages[0].block_count, 10);
        assert_eq!(page_index.pages[1].block_start, 10);
        assert_eq!(page_index.pages[1].block_count, 10);
        assert_eq!(page_index.pages[2].block_start, 20);
        assert_eq!(page_index.pages[2].block_count, 5);
        page_index.validate_covers_blocks(25).unwrap();
    }

    #[test]
    fn page_offsets_and_page_at_offset_are_stable() {
        let height_index = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(40.0, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();
        let page_index = PageLayoutIndex::from_block_height_index(
            &height_index,
            PagePolicy {
                max_blocks: 2,
                target_height: 1_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();

        assert_eq!(page_index.total_height(), 100.0);
        assert_eq!(page_index.offset_of_page(0), Some(0.0));
        assert_eq!(page_index.offset_of_page(1), Some(30.0));
        assert_eq!(page_index.offset_of_page(2), Some(100.0));
        assert_eq!(page_index.page_bounds(1), Some(30.0..100.0));
        assert_eq!(page_index.page_bounds(2), None);
        assert_eq!(page_index.page_at_offset(0.0).unwrap().page_index, 0);
        assert_eq!(page_index.page_at_offset(29.9).unwrap().page_index, 0);
        assert_eq!(page_index.page_at_offset(30.0).unwrap().page_index, 1);
        assert_eq!(page_index.page_at_offset(99.0).unwrap().page_index, 1);
        assert_eq!(page_index.page_for_block_index(0), Some(0));
        assert_eq!(page_index.page_for_block_index(2), Some(1));
        assert_eq!(page_index.page_for_block_index(4), None);
    }

    #[test]
    fn cached_pages_require_contiguous_valid_coverage() {
        let pages = vec![PageLayout {
            page_index: 0,
            block_start: 0,
            block_count: 2,
            height: 48.0,
            measured_ratio: 1.0,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
            dirty: false,
        }];
        let cached =
            PageLayoutIndex::from_cached_pages(pages.clone(), PagePolicy::default(), 2).unwrap();
        assert_eq!(cached.total_height(), 48.0);

        let error =
            PageLayoutIndex::from_cached_pages(pages, PagePolicy::default(), 3).unwrap_err();
        assert!(matches!(
            error,
            PageLayoutIndexError::CoverageTailMismatch {
                covered: 2,
                total: 3
            }
        ));
    }

    #[test]
    fn page_local_index_maps_offsets_and_synchronizes_summary() {
        let global = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Historical, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Historical, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Historical, 0.0),
            HeightEstimate::new(40.0, HeightConfidence::Historical, 0.0),
        ])
        .unwrap();
        let mut pages = PageLayoutIndex::from_block_height_index(
            &global,
            PagePolicy {
                max_blocks: 2,
                target_height: 1_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();
        let mut local = pages.local_height_index_from_global(1, &global).unwrap();
        assert_eq!(local.total_height(), 70.0);
        assert_eq!(local.offset_of_block(1), Some(30.0));
        let hit = local.block_at_offset(35.0).unwrap();
        assert_eq!(hit.local_index, 1);
        assert_eq!(hit.global_block_index, 3);
        assert_eq!(hit.offset_in_block, 5.0);

        local.update_height(0, 50.0).unwrap();
        let change = pages.synchronize_page_from_local(&local).unwrap();
        assert_eq!(change.delta, 20.0);
        assert_eq!(pages.pages[1].height, 90.0);
        assert_eq!(pages.total_height(), 120.0);
    }

    #[test]
    fn page_local_batch_updates_merge_and_clear_dirty_ranges() {
        let global = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Historical, 2.0),
            HeightEstimate::new(20.0, HeightConfidence::Historical, 2.0),
            HeightEstimate::new(30.0, HeightConfidence::Historical, 2.0),
            HeightEstimate::new(40.0, HeightConfidence::Historical, 2.0),
        ])
        .unwrap();
        let pages =
            PageLayoutIndex::from_block_height_index(&global, PagePolicy::default()).unwrap();
        let mut local = pages.local_height_index_from_global(0, &global).unwrap();

        assert_eq!(local.range_total_height(1..3), Some(50.0));
        local.update_height(3, 45.0).unwrap();
        assert_eq!(local.dirty_range(), Some(3..4));
        assert_eq!(local.update_heights(1..3, &[25.0, 35.0]).unwrap(), 10.0);
        assert_eq!(local.dirty_range(), Some(1..4));
        assert_eq!(local.total_height(), 115.0);
        local.clear_dirty();
        assert_eq!(local.dirty_range(), None);
        assert!(matches!(
            local.update_heights(1..3, &[1.0]),
            Err(PageLayoutIndexError::LocalReplacementLengthMismatch { .. })
        ));
    }

    #[test]
    fn property_page_ranges_cover_all_blocks_without_overlap() {
        for block_count in 0..250usize {
            let height_index = BlockHeightIndex::new((0..block_count).map(|index| {
                HeightEstimate::new((index % 9 + 1) as f64, HeightConfidence::Default, 0.0)
            }))
            .unwrap();
            let page_index = PageLayoutIndex::from_block_height_index(
                &height_index,
                PagePolicy {
                    max_blocks: 13,
                    target_height: 35.0,
                    ..PagePolicy::default()
                },
            )
            .unwrap();
            page_index.validate_covers_blocks(block_count).unwrap();
            for block_index in 0..block_count {
                let page = page_index.page_for_block_index(block_index).unwrap();
                assert!(page_index.pages[page].contains_block_index(block_index));
            }
        }
    }
}

#[cfg(test)]
#[path = "page_layout_property_tests.rs"]
mod property_tests;
