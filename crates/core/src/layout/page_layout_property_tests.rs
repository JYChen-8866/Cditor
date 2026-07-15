use super::*;
use crate::layout::HeightEstimate;

#[test]
fn random_page_height_updates_keep_total_height_correct() {
    let mut rng = Lcg::new(0xA11CE);
    let height_index =
        BlockHeightIndex::new((0..1_000).map(|index| {
            HeightEstimate::new((index % 37 + 1) as f64, HeightConfidence::Default, 0.0)
        }))
        .unwrap();
    let mut page_index = PageLayoutIndex::from_block_height_index(
        &height_index,
        PagePolicy {
            max_blocks: 17,
            target_height: 1_000.0,
            ..PagePolicy::default()
        },
    )
    .unwrap();

    for _ in 0..1_000 {
        let page = rng.next_usize(page_index.page_count());
        let new_height = (rng.next_usize(2_000) + 1) as f64;
        page_index.update_page_height(page, new_height).unwrap();
        let expected: f64 = page_index.pages.iter().map(|page| page.height).sum();
        assert_eq!(page_index.total_height(), expected);
    }
}

#[test]
fn page_policy_splits_by_cost_text_inline_runs_and_complex_blocks() {
    let estimates = [
        PageBlockEstimate {
            height: 10.0,
            confidence: HeightConfidence::Predictive,
            max_error_hint: 1.0,
            estimated_cost: 5,
            text_bytes: 10,
            inline_runs: 10,
            is_complex: false,
        },
        PageBlockEstimate {
            height: 10.0,
            confidence: HeightConfidence::Predictive,
            max_error_hint: 1.0,
            estimated_cost: 6,
            text_bytes: 10,
            inline_runs: 10,
            is_complex: false,
        },
        PageBlockEstimate {
            height: 10.0,
            confidence: HeightConfidence::Predictive,
            max_error_hint: 1.0,
            estimated_cost: 1,
            text_bytes: 100,
            inline_runs: 10,
            is_complex: true,
        },
        PageBlockEstimate {
            height: 10.0,
            confidence: HeightConfidence::Predictive,
            max_error_hint: 1.0,
            estimated_cost: 1,
            text_bytes: 10,
            inline_runs: 100,
            is_complex: true,
        },
    ];

    let page_index = PageLayoutIndex::from_block_estimates(
        estimates,
        PagePolicy {
            max_blocks: 10,
            target_height: 1_000.0,
            max_estimated_cost: 10,
            max_text_bytes: 110,
            max_inline_runs: 110,
            max_complex_blocks: 1,
        },
    )
    .unwrap();

    assert_eq!(page_index.page_count(), 3);
    page_index.validate_covers_blocks(4).unwrap();
}

#[test]
fn cached_pages_reject_empty_pages_and_invalid_aggregates() {
    let valid = PageLayout {
        page_index: 0,
        block_start: 0,
        block_count: 1,
        height: 24.0,
        measured_ratio: 1.0,
        confidence: HeightConfidence::Exact,
        max_error_hint: 0.0,
        dirty: false,
    };

    let mut empty = valid;
    empty.block_count = 0;
    assert!(matches!(
        PageLayoutIndex::from_cached_pages(vec![empty], PagePolicy::default(), 0),
        Err(PageLayoutIndexError::EmptyCachedPage { page: 0 })
    ));

    let mut invalid_ratio = valid;
    invalid_ratio.measured_ratio = 1.1;
    assert!(matches!(
        PageLayoutIndex::from_cached_pages(vec![invalid_ratio], PagePolicy::default(), 1),
        Err(PageLayoutIndexError::InvalidMeasuredRatio(_))
    ));

    let mut invalid_error = valid;
    invalid_error.max_error_hint = f64::NAN;
    assert!(matches!(
        PageLayoutIndex::from_cached_pages(vec![invalid_error], PagePolicy::default(), 1),
        Err(PageLayoutIndexError::InvalidMaxErrorHint(_))
    ));
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
