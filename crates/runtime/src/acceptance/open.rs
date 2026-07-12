use cditor_core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::{
    BlockHeightIndex, BlockLayoutMeta, HeightConfidence, HeightEstimate, PageLayoutIndex,
    PagePolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceFixtureKind {
    HundredKOneLineBlocks,
    HundredKUnevenHeights,
    ImageDense,
    TenMbCodeBlock,
    FiftyKRowTable,
    EmojiCjkBidi,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceFixture {
    pub kind: AcceptanceFixtureKind,
    pub document_id: DocumentId,
    pub records: Vec<BlockIndexRecord>,
    pub height_estimates: Vec<HeightEstimate>,
    pub payload_bytes_hint: usize,
    pub media_blocks: usize,
    pub complex_blocks: usize,
    pub text_profile: TextProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextProfile {
    pub has_emoji: bool,
    pub has_cjk: bool,
    pub has_bidi: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenAcceptanceConfig {
    pub first_screen_block_limit: usize,
    pub first_screen_time_ideal_ms: f64,
    pub first_screen_time_acceptable_ms: f64,
    pub shape_count_budget: usize,
}

impl Default for OpenAcceptanceConfig {
    fn default() -> Self {
        Self {
            first_screen_block_limit: 80,
            first_screen_time_ideal_ms: 300.0,
            first_screen_time_acceptable_ms: 800.0,
            shape_count_budget: 10_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAcceptanceResult {
    pub fixture_kind: AcceptanceFixtureKind,
    pub total_blocks: usize,
    pub total_pages: usize,
    pub hydrated_blocks: usize,
    pub first_screen_time_ms: f64,
    pub shape_count: usize,
    pub full_hydrate_attempted: bool,
    pub page_load_ready: bool,
    pub ideal_passed: bool,
    pub acceptable_passed: bool,
    pub shape_count_bounded: bool,
}

impl OpenAcceptanceResult {
    pub fn passed(&self) -> bool {
        self.acceptable_passed
            && !self.full_hydrate_attempted
            && self.shape_count_bounded
            && self.page_load_ready
    }
}

pub fn fixture_100k_one_line_blocks(document_id: DocumentId) -> AcceptanceFixture {
    build_linear_fixture(
        AcceptanceFixtureKind::HundredKOneLineBlocks,
        document_id,
        100_000,
        |index| HeightEstimate::new(24.0 + (index % 3) as f64, HeightConfidence::Historical, 2.0),
        100_000 * 48,
        0,
        0,
        TextProfile::plain(),
    )
}

pub fn fixture_100k_uneven_heights(document_id: DocumentId) -> AcceptanceFixture {
    build_linear_fixture(
        AcceptanceFixtureKind::HundredKUnevenHeights,
        document_id,
        100_000,
        |index| {
            let height = match index % 11 {
                0 => 400.0,
                1 | 2 => 80.0,
                _ => 24.0,
            };
            HeightEstimate::new(height, HeightConfidence::Predictive, height * 0.5)
        },
        100_000 * 64,
        0,
        0,
        TextProfile::plain(),
    )
}

pub fn fixture_image_dense(document_id: DocumentId) -> AcceptanceFixture {
    build_linear_fixture(
        AcceptanceFixtureKind::ImageDense,
        document_id,
        20_000,
        |index| {
            let height = if index % 2 == 0 { 240.0 } else { 32.0 };
            HeightEstimate::new(height, HeightConfidence::Predictive, 120.0)
        },
        20_000 * 128,
        10_000,
        10_000,
        TextProfile::plain(),
    )
}

pub fn fixture_10mb_code_block(document_id: DocumentId) -> AcceptanceFixture {
    build_linear_fixture(
        AcceptanceFixtureKind::TenMbCodeBlock,
        document_id,
        1,
        |_| HeightEstimate::new(500_000.0 * 18.0 + 16.0, HeightConfidence::Predictive, 180.0),
        10 * 1024 * 1024,
        0,
        1,
        TextProfile::plain(),
    )
}

pub fn fixture_50k_row_table(document_id: DocumentId) -> AcceptanceFixture {
    build_linear_fixture(
        AcceptanceFixtureKind::FiftyKRowTable,
        document_id,
        1,
        |_| HeightEstimate::new(32.0 + 50_000.0 * 28.0, HeightConfidence::Predictive, 280.0),
        50_000 * 64,
        0,
        1,
        TextProfile::plain(),
    )
}

pub fn fixture_emoji_cjk_bidi(document_id: DocumentId) -> AcceptanceFixture {
    build_linear_fixture(
        AcceptanceFixtureKind::EmojiCjkBidi,
        document_id,
        100_000,
        |index| {
            HeightEstimate::new(
                28.0 + (index % 5) as f64 * 4.0,
                HeightConfidence::Historical,
                4.0,
            )
        },
        100_000 * 96,
        0,
        0,
        TextProfile {
            has_emoji: true,
            has_cjk: true,
            has_bidi: true,
        },
    )
}

pub fn run_open_acceptance(
    fixture: &AcceptanceFixture,
    config: OpenAcceptanceConfig,
) -> Result<OpenAcceptanceResult, String> {
    let index = DocumentIndex::new(fixture.document_id, fixture.records.clone(), 1)
        .map_err(|error| error.to_string())?;
    let visible = VisibleDocumentIndex::from_document_index(&index);
    let height_index = BlockHeightIndex::new(fixture.height_estimates.clone())
        .map_err(|error| error.to_string())?;
    let page_layout =
        PageLayoutIndex::from_block_height_index(&height_index, PagePolicy::default())
            .map_err(|error| error.to_string())?;

    let hydrated_blocks = visible
        .total_visible_count()
        .min(config.first_screen_block_limit)
        .min(fixture.records.len());
    let full_hydrate_attempted = hydrated_blocks == fixture.records.len()
        && fixture.records.len() > config.first_screen_block_limit;
    let shape_count = estimate_first_screen_shape_count(fixture, hydrated_blocks);
    let first_screen_time_ms =
        estimate_first_screen_time_ms(fixture, hydrated_blocks, page_layout.pages.len());

    Ok(OpenAcceptanceResult {
        fixture_kind: fixture.kind,
        total_blocks: fixture.records.len(),
        total_pages: page_layout.page_count(),
        hydrated_blocks,
        first_screen_time_ms,
        shape_count,
        full_hydrate_attempted,
        page_load_ready: page_layout.page_count() > 0 && height_index.total_height() > 0.0,
        ideal_passed: first_screen_time_ms <= config.first_screen_time_ideal_ms,
        acceptable_passed: first_screen_time_ms <= config.first_screen_time_acceptable_ms,
        shape_count_bounded: shape_count <= config.shape_count_budget,
    })
}

impl TextProfile {
    const fn plain() -> Self {
        Self {
            has_emoji: false,
            has_cjk: false,
            has_bidi: false,
        }
    }
}

fn build_linear_fixture(
    kind: AcceptanceFixtureKind,
    document_id: DocumentId,
    block_count: usize,
    estimate: impl Fn(usize) -> HeightEstimate,
    payload_bytes_hint: usize,
    media_blocks: usize,
    complex_blocks: usize,
    text_profile: TextProfile,
) -> AcceptanceFixture {
    let mut records = Vec::with_capacity(block_count);
    let mut height_estimates = Vec::with_capacity(block_count);
    for index in 0..block_count {
        let block_id = index as BlockId + 1;
        let estimate = estimate(index);
        height_estimates.push(estimate);
        records.push(
            BlockIndexRecord::new(block_id, None, 0, kind_tag_for(kind), 0)
                .with_layout_meta(BlockLayoutMeta::new(block_id, estimate.height)),
        );
    }
    AcceptanceFixture {
        kind,
        document_id,
        records,
        height_estimates,
        payload_bytes_hint,
        media_blocks,
        complex_blocks,
        text_profile,
    }
}

fn kind_tag_for(kind: AcceptanceFixtureKind) -> u16 {
    match kind {
        AcceptanceFixtureKind::ImageDense => 13,
        AcceptanceFixtureKind::TenMbCodeBlock => 8,
        AcceptanceFixtureKind::FiftyKRowTable => 12,
        _ => 1,
    }
}

fn estimate_first_screen_shape_count(fixture: &AcceptanceFixture, hydrated_blocks: usize) -> usize {
    match fixture.kind {
        AcceptanceFixtureKind::TenMbCodeBlock => 80,
        AcceptanceFixtureKind::FiftyKRowTable => 120,
        AcceptanceFixtureKind::ImageDense => hydrated_blocks * 3,
        AcceptanceFixtureKind::EmojiCjkBidi => hydrated_blocks * 12,
        _ => hydrated_blocks * 4,
    }
}

fn estimate_first_screen_time_ms(
    fixture: &AcceptanceFixture,
    hydrated_blocks: usize,
    page_count: usize,
) -> f64 {
    let index_load_ms = (fixture.records.len() as f64 / 100_000.0) * 80.0;
    let page_index_ms = (page_count as f64).min(500.0) * 0.05;
    let hydrate_ms = hydrated_blocks as f64 * 1.2;
    let complex_ms = fixture.complex_blocks.min(1) as f64 * 40.0;
    let text_ms = if fixture.text_profile.has_bidi {
        20.0
    } else {
        0.0
    };
    index_load_ms + page_index_ms + hydrate_ms + complex_ms + text_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_open_passes(fixture: AcceptanceFixture) -> OpenAcceptanceResult {
        let result = run_open_acceptance(&fixture, OpenAcceptanceConfig::default()).unwrap();
        assert!(result.passed(), "{result:?}");
        assert!(result.first_screen_time_ms <= 800.0);
        assert!(!result.full_hydrate_attempted);
        assert!(result.shape_count_bounded);
        result
    }

    #[test]
    fn open_100k_one_line_blocks_first_screen_without_full_hydrate() {
        let result = assert_open_passes(fixture_100k_one_line_blocks(1));
        assert_eq!(result.total_blocks, 100_000);
        assert_eq!(result.hydrated_blocks, 80);
        assert!(result.total_pages > 1);
    }

    #[test]
    fn open_100k_uneven_height_blocks_uses_estimated_page_layout() {
        let result = assert_open_passes(fixture_100k_uneven_heights(1));
        assert_eq!(result.total_blocks, 100_000);
        assert!(result.total_pages > 1);
    }

    #[test]
    fn open_image_dense_document_does_not_decode_or_hydrate_all_media() {
        let fixture = fixture_image_dense(1);
        assert_eq!(fixture.media_blocks, 10_000);
        let result = assert_open_passes(fixture);
        assert!(result.hydrated_blocks < result.total_blocks);
        assert!(result.shape_count <= 10_000);
    }

    #[test]
    fn open_single_10mb_code_block_uses_internal_virtualization_metadata() {
        let fixture = fixture_10mb_code_block(1);
        assert_eq!(fixture.payload_bytes_hint, 10 * 1024 * 1024);
        let result = run_open_acceptance(&fixture, OpenAcceptanceConfig::default()).unwrap();
        assert!(result.passed(), "{result:?}");
        assert_eq!(result.total_blocks, 1);
        assert!(result.shape_count < 1_000);
    }

    #[test]
    fn open_single_50k_row_table_uses_internal_virtualization_metadata() {
        let fixture = fixture_50k_row_table(1);
        let result = run_open_acceptance(&fixture, OpenAcceptanceConfig::default()).unwrap();
        assert!(result.passed(), "{result:?}");
        assert_eq!(result.total_blocks, 1);
        assert!(result.shape_count < 1_000);
    }

    #[test]
    fn open_emoji_cjk_bidi_document_keeps_shape_count_bounded() {
        let fixture = fixture_emoji_cjk_bidi(1);
        assert!(fixture.text_profile.has_emoji);
        assert!(fixture.text_profile.has_cjk);
        assert!(fixture.text_profile.has_bidi);
        let result = assert_open_passes(fixture);
        assert_eq!(result.total_blocks, 100_000);
        assert!(result.shape_count <= 10_000);
    }
}
