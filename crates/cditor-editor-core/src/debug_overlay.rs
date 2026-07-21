use std::ops::Range;

use crate::scroll::{AnchorKind, LayoutPx, ScrollPrecision, VirtualScrollState};
use cditor_core::ids::BlockId;
use cditor_core::layout::HeightConfidence;

#[derive(Debug, Clone, PartialEq)]
pub struct DebugOverlaySnapshot {
    pub global_scroll_top: LayoutPx,
    pub model_total_height: LayoutPx,
    pub displayed_total_height: LayoutPx,
    pub scroll_precision: ScrollPrecision,
    pub current_page: usize,
    pub window_page_range: Range<usize>,
    pub loaded_pages: Vec<usize>,
    pub placeholder_pages: Vec<usize>,
    pub anchor: Option<DebugAnchor>,
    pub entity_count: usize,
    pub pinned_entity_count: usize,
    pub shape_count: usize,
    pub layout_time_ms: f64,
    pub sqlite_query_count: usize,
    pub sqlite_load_time_ms: f64,
    pub height_correction_count: usize,
    pub anchor_restore_count: usize,
    pub scroll_jitter_px: f64,
    pub caret_jitter_px: f64,
    pub window_commit_count: usize,
    pub thumb_reverse_jump_count: usize,
    pub page_boundaries: Vec<PageBoundaryDebug>,
    pub height_regions: Vec<HeightConfidenceRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugAnchor {
    pub kind: AnchorKind,
    pub block_id: BlockId,
    pub viewport_y: LayoutPx,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageBoundaryDebug {
    pub page_index: usize,
    pub global_y: LayoutPx,
    pub measured_ratio: f64,
    pub confidence: HeightConfidence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeightConfidenceRegion {
    pub block_range: Range<usize>,
    pub confidence: HeightConfidence,
    pub global_y_range: Range<LayoutPx>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugOverlayLine {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugOverlayViewModel {
    pub lines: Vec<DebugOverlayLine>,
    pub page_boundary_markers: Vec<PageBoundaryDebug>,
    pub estimated_regions: Vec<HeightConfidenceRegion>,
    pub historical_regions: Vec<HeightConfidenceRegion>,
    pub exact_regions: Vec<HeightConfidenceRegion>,
}

impl DebugOverlaySnapshot {
    pub fn from_scroll_state(
        scroll: &VirtualScrollState,
        current_page: usize,
        window_page_range: Range<usize>,
    ) -> Self {
        Self {
            global_scroll_top: scroll.global_scroll_top,
            model_total_height: scroll.model_total_height,
            displayed_total_height: scroll.displayed_total_height,
            scroll_precision: scroll.precision,
            current_page,
            window_page_range,
            loaded_pages: Vec::new(),
            placeholder_pages: Vec::new(),
            anchor: scroll.anchor.map(|anchor| DebugAnchor {
                kind: AnchorKind::ViewportTop,
                block_id: anchor.block_id,
                viewport_y: anchor.viewport_y,
            }),
            entity_count: 0,
            pinned_entity_count: 0,
            shape_count: 0,
            layout_time_ms: 0.0,
            sqlite_query_count: 0,
            sqlite_load_time_ms: 0.0,
            height_correction_count: 0,
            anchor_restore_count: 0,
            scroll_jitter_px: 0.0,
            caret_jitter_px: 0.0,
            window_commit_count: 0,
            thumb_reverse_jump_count: 0,
            page_boundaries: Vec::new(),
            height_regions: Vec::new(),
        }
    }

    pub fn with_loaded_and_placeholder_pages(
        mut self,
        loaded_pages: Vec<usize>,
        placeholder_pages: Vec<usize>,
    ) -> Self {
        self.loaded_pages = loaded_pages;
        self.placeholder_pages = placeholder_pages;
        self
    }

    pub fn with_anchor(mut self, anchor: DebugAnchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    pub fn with_entity_stats(mut self, entity_count: usize, pinned_entity_count: usize) -> Self {
        self.entity_count = entity_count;
        self.pinned_entity_count = pinned_entity_count;
        self
    }

    pub fn with_layout_stats(mut self, shape_count: usize, layout_time_ms: f64) -> Self {
        self.shape_count = shape_count;
        self.layout_time_ms = layout_time_ms;
        self
    }

    pub fn with_sqlite_stats(mut self, query_count: usize, load_time_ms: f64) -> Self {
        self.sqlite_query_count = query_count;
        self.sqlite_load_time_ms = load_time_ms;
        self
    }

    pub fn with_corrections(mut self, height_corrections: usize, anchor_restores: usize) -> Self {
        self.height_correction_count = height_corrections;
        self.anchor_restore_count = anchor_restores;
        self
    }

    pub fn with_jitter(mut self, scroll_jitter_px: f64, caret_jitter_px: f64) -> Self {
        self.scroll_jitter_px = scroll_jitter_px.abs();
        self.caret_jitter_px = caret_jitter_px.abs();
        self
    }

    pub fn with_page_boundaries(mut self, page_boundaries: Vec<PageBoundaryDebug>) -> Self {
        self.page_boundaries = page_boundaries;
        self
    }

    pub fn with_height_regions(mut self, height_regions: Vec<HeightConfidenceRegion>) -> Self {
        self.height_regions = height_regions;
        self
    }

    pub fn render(&self) -> DebugOverlayViewModel {
        let anchor_kind = self
            .anchor
            .map(|anchor| format!("{:?}", anchor.kind))
            .unwrap_or_else(|| "None".to_owned());
        let anchor_block = self
            .anchor
            .map(|anchor| anchor.block_id.to_string())
            .unwrap_or_else(|| "None".to_owned());
        let anchor_viewport_y = self
            .anchor
            .map(|anchor| format_px(anchor.viewport_y))
            .unwrap_or_else(|| "None".to_owned());

        let lines = vec![
            line("global_scroll_top", format_px(self.global_scroll_top)),
            line("model_total_height", format_px(self.model_total_height)),
            line(
                "displayed_total_height",
                format_px(self.displayed_total_height),
            ),
            line("ScrollPrecision", format!("{:?}", self.scroll_precision)),
            line("current_page", self.current_page.to_string()),
            line("window_page_range", format_range(&self.window_page_range)),
            line("loaded_pages", format_pages(&self.loaded_pages)),
            line("placeholder_pages", format_pages(&self.placeholder_pages)),
            line("anchor_kind", anchor_kind),
            line("anchor_block", anchor_block),
            line("anchor_viewport_y", anchor_viewport_y),
            line("entity_count", self.entity_count.to_string()),
            line("pinned_count", self.pinned_entity_count.to_string()),
            line("shape_count", self.shape_count.to_string()),
            line("layout_time_ms", format_ms(self.layout_time_ms)),
            line("SQLite query count", self.sqlite_query_count.to_string()),
            line("SQLite load time", format_ms(self.sqlite_load_time_ms)),
            line(
                "height correction count",
                self.height_correction_count.to_string(),
            ),
            line(
                "anchor restore count",
                self.anchor_restore_count.to_string(),
            ),
            line("scroll_jitter_px", format_px(self.scroll_jitter_px)),
            line("caret_jitter_px", format_px(self.caret_jitter_px)),
            line("window commit count", self.window_commit_count.to_string()),
            line(
                "thumb reverse-jump count",
                self.thumb_reverse_jump_count.to_string(),
            ),
        ];

        DebugOverlayViewModel {
            lines,
            page_boundary_markers: self.page_boundaries.clone(),
            estimated_regions: self.regions_for(HeightConfidence::Default),
            historical_regions: self.regions_for(HeightConfidence::Historical),
            exact_regions: self.regions_for(HeightConfidence::Exact),
        }
    }

    fn regions_for(&self, confidence: HeightConfidence) -> Vec<HeightConfidenceRegion> {
        self.height_regions
            .iter()
            .filter(|region| match confidence {
                HeightConfidence::Default => {
                    matches!(
                        region.confidence,
                        HeightConfidence::Default | HeightConfidence::Predictive
                    )
                }
                _ => region.confidence == confidence,
            })
            .cloned()
            .collect()
    }
}

pub fn scroll_jitter_px(
    expected_anchor_viewport_y: LayoutPx,
    actual_anchor_viewport_y: LayoutPx,
) -> f64 {
    (actual_anchor_viewport_y - expected_anchor_viewport_y).abs()
}

pub fn caret_jitter_px(
    expected_caret_viewport_y: LayoutPx,
    actual_caret_viewport_y: LayoutPx,
) -> f64 {
    (actual_caret_viewport_y - expected_caret_viewport_y).abs()
}

fn line(key: impl Into<String>, value: impl Into<String>) -> DebugOverlayLine {
    DebugOverlayLine {
        key: key.into(),
        value: value.into(),
    }
}

fn format_px(value: f64) -> String {
    format!("{value:.2}px")
}

fn format_ms(value: f64) -> String {
    format!("{value:.2}ms")
}

fn format_range(range: &Range<usize>) -> String {
    format!("{}..{}", range.start, range.end)
}

fn format_pages(pages: &[usize]) -> String {
    pages
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scroll::{ScrollOrigin, VirtualScrollState};

    fn line_value<'a>(view: &'a DebugOverlayViewModel, key: &str) -> Option<&'a str> {
        view.lines
            .iter()
            .find(|line| line.key == key)
            .map(|line| line.value.as_str())
    }

    #[test]
    fn manual_scroll_debug_overlay_displays_scroll_height_window_and_anchor() {
        let mut scroll = VirtualScrollState::new(600.0, 10_000.0).unwrap();
        scroll
            .scroll_to_global_offset(1_234.0, ScrollOrigin::UserWheel)
            .unwrap();
        let snapshot = DebugOverlaySnapshot::from_scroll_state(&scroll, 3, 2..6)
            .with_loaded_and_placeholder_pages(vec![2, 3, 4], vec![5])
            .with_anchor(DebugAnchor {
                kind: AnchorKind::ViewportTop,
                block_id: 42,
                viewport_y: 0.0,
            })
            .with_entity_stats(120, 4)
            .with_layout_stats(2_000, 3.5)
            .with_sqlite_stats(7, 1.2);

        let view = snapshot.render();

        assert_eq!(line_value(&view, "global_scroll_top"), Some("1234.00px"));
        assert_eq!(line_value(&view, "model_total_height"), Some("10000.00px"));
        assert_eq!(
            line_value(&view, "displayed_total_height"),
            Some("10000.00px")
        );
        assert_eq!(line_value(&view, "current_page"), Some("3"));
        assert_eq!(line_value(&view, "window_page_range"), Some("2..6"));
        assert_eq!(line_value(&view, "loaded_pages"), Some("2,3,4"));
        assert_eq!(line_value(&view, "placeholder_pages"), Some("5"));
        assert_eq!(line_value(&view, "anchor_block"), Some("42"));
        assert_eq!(line_value(&view, "entity_count"), Some("120"));
        assert_eq!(line_value(&view, "pinned_count"), Some("4"));
        assert_eq!(line_value(&view, "shape_count"), Some("2000"));
        assert_eq!(line_value(&view, "SQLite query count"), Some("7"));
    }

    #[test]
    fn height_chaos_overlay_shows_corrections_and_jitter() {
        let scroll = VirtualScrollState::new(600.0, 10_000.0).unwrap();
        let snapshot = DebugOverlaySnapshot::from_scroll_state(&scroll, 0, 0..2)
            .with_corrections(12, 1)
            .with_jitter(
                scroll_jitter_px(100.0, 101.5),
                caret_jitter_px(220.0, 221.25),
            );
        let view = snapshot.render();

        assert_eq!(line_value(&view, "height correction count"), Some("12"));
        assert_eq!(line_value(&view, "anchor restore count"), Some("1"));
        assert_eq!(line_value(&view, "scroll_jitter_px"), Some("1.50px"));
        assert_eq!(line_value(&view, "caret_jitter_px"), Some("1.25px"));
    }

    #[test]
    fn overlay_visualizes_page_boundaries_and_height_confidence_regions() {
        let scroll = VirtualScrollState::new(600.0, 10_000.0).unwrap();
        let snapshot = DebugOverlaySnapshot::from_scroll_state(&scroll, 1, 1..3)
            .with_page_boundaries(vec![
                PageBoundaryDebug {
                    page_index: 1,
                    global_y: 1_000.0,
                    measured_ratio: 0.8,
                    confidence: HeightConfidence::Predictive,
                },
                PageBoundaryDebug {
                    page_index: 2,
                    global_y: 2_000.0,
                    measured_ratio: 1.0,
                    confidence: HeightConfidence::Exact,
                },
            ])
            .with_height_regions(vec![
                HeightConfidenceRegion {
                    block_range: 100..150,
                    confidence: HeightConfidence::Predictive,
                    global_y_range: 1_000.0..1_500.0,
                },
                HeightConfidenceRegion {
                    block_range: 150..200,
                    confidence: HeightConfidence::Historical,
                    global_y_range: 1_500.0..2_000.0,
                },
                HeightConfidenceRegion {
                    block_range: 200..220,
                    confidence: HeightConfidence::Exact,
                    global_y_range: 2_000.0..2_200.0,
                },
            ]);

        let view = snapshot.render();

        assert_eq!(view.page_boundary_markers.len(), 2);
        assert_eq!(view.estimated_regions.len(), 1);
        assert_eq!(view.historical_regions.len(), 1);
        assert_eq!(view.exact_regions.len(), 1);
    }
}
