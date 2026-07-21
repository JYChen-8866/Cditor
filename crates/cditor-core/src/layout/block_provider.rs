use crate::layout::{HeightConfidence, HeightEstimate, PageBlockEstimate};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StableBox {
    pub estimated_height: f64,
    pub min_height: f64,
    pub max_height: Option<f64>,
    pub confidence: HeightConfidence,
}

pub trait StableBoxProvider {
    fn stable_box(&self, available_width: f64) -> StableBox;
}

pub trait BlockLayoutProvider {
    fn estimate_height(&self, width: f64) -> HeightEstimate;
    fn intrinsic_size(&self) -> Option<Size>;
    fn layout_cost(&self) -> u32;
    fn can_measure_offscreen(&self) -> bool;

    fn page_block_estimate(&self, width: f64) -> PageBlockEstimate {
        let estimate = self.estimate_height(width);
        PageBlockEstimate {
            height: estimate.height,
            confidence: estimate.confidence,
            max_error_hint: estimate.max_error_hint,
            estimated_cost: self.layout_cost(),
            text_bytes: 0,
            inline_runs: 0,
            is_complex: self.layout_cost() > 10,
        }
    }

    fn offscreen_measure_confidence(&self, exact_environment_match: bool) -> HeightConfidence {
        if self.can_measure_offscreen() && exact_environment_match {
            HeightConfidence::Exact
        } else if self.can_measure_offscreen() {
            HeightConfidence::Predictive
        } else {
            HeightConfidence::Historical
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphLayoutProvider {
    pub text_bytes: usize,
    pub avg_char_width: f64,
    pub line_height: f64,
    pub padding_y: f64,
}

impl BlockLayoutProvider for ParagraphLayoutProvider {
    fn estimate_height(&self, width: f64) -> HeightEstimate {
        let chars_per_line = (width / self.avg_char_width.max(1.0)).floor().max(1.0);
        let line_count = (self.text_bytes as f64 / chars_per_line).ceil().max(1.0);
        HeightEstimate::new(
            line_count * self.line_height + self.padding_y,
            HeightConfidence::Predictive,
            self.line_height,
        )
    }

    fn intrinsic_size(&self) -> Option<Size> {
        None
    }

    fn layout_cost(&self) -> u32 {
        (self.text_bytes / 512).max(1) as u32
    }

    fn can_measure_offscreen(&self) -> bool {
        true
    }

    fn page_block_estimate(&self, width: f64) -> PageBlockEstimate {
        let mut estimate = <Self as BlockLayoutProvider>::estimate_height(self, width);
        estimate.max_error_hint = self.line_height;
        PageBlockEstimate {
            height: estimate.height,
            confidence: estimate.confidence,
            max_error_hint: estimate.max_error_hint,
            estimated_cost: self.layout_cost(),
            text_bytes: self.text_bytes,
            inline_runs: 0,
            is_complex: self.text_bytes > 8 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlockLayoutProvider {
    pub line_count: usize,
    pub line_height: f64,
    pub padding_y: f64,
    pub toolbar_height: f64,
}

impl BlockLayoutProvider for CodeBlockLayoutProvider {
    fn estimate_height(&self, _width: f64) -> HeightEstimate {
        HeightEstimate::new(
            self.line_count.max(1) as f64 * self.line_height + self.padding_y + self.toolbar_height,
            HeightConfidence::Predictive,
            2.0,
        )
    }

    fn intrinsic_size(&self) -> Option<Size> {
        None
    }

    fn layout_cost(&self) -> u32 {
        (self.line_count / 50).max(1) as u32
    }

    fn can_measure_offscreen(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableLayoutProvider {
    pub row_count: usize,
    pub header_height: f64,
    pub row_height: f64,
    pub min_width: f64,
}

impl BlockLayoutProvider for TableLayoutProvider {
    fn estimate_height(&self, _width: f64) -> HeightEstimate {
        HeightEstimate::new(
            self.header_height + self.row_count as f64 * self.row_height,
            HeightConfidence::Predictive,
            self.row_height * 2.0,
        )
    }

    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size {
            width: self.min_width,
            height: self.header_height + self.row_count as f64 * self.row_height,
        })
    }

    fn layout_cost(&self) -> u32 {
        (self.row_count / 20).max(1) as u32
    }

    fn can_measure_offscreen(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageLayoutProvider {
    pub intrinsic_width: Option<f64>,
    pub intrinsic_height: Option<f64>,
    pub placeholder_height: f64,
}

impl ImageLayoutProvider {
    fn aspect_ratio(&self) -> Option<f64> {
        let width = self.intrinsic_width?;
        let height = self.intrinsic_height?;
        (width > 0.0 && height > 0.0).then_some(width / height)
    }
}

impl StableBoxProvider for ImageLayoutProvider {
    fn stable_box(&self, available_width: f64) -> StableBox {
        let estimated_height = self
            .aspect_ratio()
            .map(|aspect_ratio| available_width / aspect_ratio)
            .unwrap_or(self.placeholder_height.max(1.0));
        StableBox {
            estimated_height,
            min_height: self.placeholder_height.max(1.0),
            max_height: self.intrinsic_height,
            confidence: if self.aspect_ratio().is_some() {
                HeightConfidence::Predictive
            } else {
                HeightConfidence::Default
            },
        }
    }
}

impl BlockLayoutProvider for ImageLayoutProvider {
    fn estimate_height(&self, width: f64) -> HeightEstimate {
        let stable = self.stable_box(width);
        HeightEstimate::new(stable.estimated_height, stable.confidence, 48.0)
    }

    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size {
            width: self.intrinsic_width?,
            height: self.intrinsic_height?,
        })
    }

    fn layout_cost(&self) -> u32 {
        8
    }

    fn can_measure_offscreen(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StableBoxLayoutProvider {
    pub preferred_width: Option<f64>,
    pub preferred_height: f64,
    pub min_height: f64,
    pub max_height: Option<f64>,
    pub cost: u32,
}

impl StableBoxProvider for StableBoxLayoutProvider {
    fn stable_box(&self, _available_width: f64) -> StableBox {
        StableBox {
            estimated_height: self.preferred_height.max(self.min_height).max(1.0),
            min_height: self.min_height.max(1.0),
            max_height: self.max_height,
            confidence: HeightConfidence::Historical,
        }
    }
}

impl BlockLayoutProvider for StableBoxLayoutProvider {
    fn estimate_height(&self, width: f64) -> HeightEstimate {
        let stable = self.stable_box(width);
        HeightEstimate::new(stable.estimated_height, stable.confidence, 16.0)
    }

    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size {
            width: self.preferred_width.unwrap_or(0.0),
            height: self.preferred_height,
        })
    }

    fn layout_cost(&self) -> u32 {
        self.cost.max(1)
    }

    fn can_measure_offscreen(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PagePolicy;

    #[test]
    fn paragraph_estimates_height_by_wrapped_lines() {
        let provider = ParagraphLayoutProvider {
            text_bytes: 100,
            avg_char_width: 10.0,
            line_height: 20.0,
            padding_y: 8.0,
        };
        let estimate = provider.estimate_height(50.0);
        assert_eq!(estimate.height, 408.0);
        assert_eq!(estimate.confidence, HeightConfidence::Predictive);
    }

    #[test]
    fn code_block_height_is_line_count_based() {
        let provider = CodeBlockLayoutProvider {
            line_count: 10,
            line_height: 18.0,
            padding_y: 12.0,
            toolbar_height: 24.0,
        };
        assert_eq!(provider.estimate_height(400.0).height, 216.0);
    }

    #[test]
    fn table_height_uses_header_and_rows() {
        let provider = TableLayoutProvider {
            row_count: 100,
            header_height: 32.0,
            row_height: 28.0,
            min_width: 600.0,
        };
        assert_eq!(provider.estimate_height(500.0).height, 2832.0);
        assert_eq!(provider.intrinsic_size().unwrap().width, 600.0);
    }

    #[test]
    fn image_metadata_missing_still_has_non_zero_stable_height() {
        let provider = ImageLayoutProvider {
            intrinsic_width: None,
            intrinsic_height: None,
            placeholder_height: 180.0,
        };
        let estimate = provider.estimate_height(640.0);
        assert_eq!(estimate.height, 180.0);
        assert!(estimate.height > 0.0);
        assert_eq!(estimate.confidence, HeightConfidence::Default);
    }

    #[test]
    fn image_aspect_ratio_estimates_from_available_width() {
        let provider = ImageLayoutProvider {
            intrinsic_width: Some(1600.0),
            intrinsic_height: Some(900.0),
            placeholder_height: 180.0,
        };
        let estimate = provider.estimate_height(800.0);
        assert_eq!(estimate.height, 450.0);
    }

    #[test]
    fn embed_and_whiteboard_use_stable_box_not_zero_height() {
        let provider = StableBoxLayoutProvider {
            preferred_width: Some(800.0),
            preferred_height: 480.0,
            min_height: 240.0,
            max_height: None,
            cost: 20,
        };
        let stable = provider.stable_box(600.0);
        assert_eq!(stable.estimated_height, 480.0);
        assert!(stable.estimated_height > 0.0);
    }

    #[test]
    fn offscreen_mismatch_cannot_upgrade_to_exact() {
        let provider = TableLayoutProvider {
            row_count: 10,
            header_height: 32.0,
            row_height: 28.0,
            min_width: 600.0,
        };
        assert_eq!(
            provider.offscreen_measure_confidence(false),
            HeightConfidence::Historical
        );
    }

    #[test]
    fn layout_cost_participates_page_policy_estimate() {
        let provider = StableBoxLayoutProvider {
            preferred_width: Some(800.0),
            preferred_height: 480.0,
            min_height: 240.0,
            max_height: None,
            cost: 20,
        };
        let page_estimate = provider.page_block_estimate(800.0);
        let policy = PagePolicy {
            max_estimated_cost: 10,
            ..PagePolicy::default()
        };
        assert!(page_estimate.estimated_cost > policy.max_estimated_cost);
        assert!(page_estimate.is_complex);
    }
}
