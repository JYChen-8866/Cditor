use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Range;

use crate::scroll::ScrollAnchor;
use cditor_core::ids::BlockId;
use cditor_core::layout::{BlockHeightIndex, HeightEstimate, PageLayoutIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockEntityHandle {
    pub block_id: BlockId,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderWindow {
    pub page_range: Range<usize>,
    pub block_range: Range<usize>,
    pub entities: HashMap<BlockId, BlockEntityHandle>,
    pub local_height_index: BlockHeightIndex,
    pub content: RenderWindowContent,
}

impl RenderWindow {
    pub fn loaded(
        page_range: Range<usize>,
        block_range: Range<usize>,
        block_ids: &[BlockId],
        local_height_index: BlockHeightIndex,
        generation: u64,
    ) -> Result<Self, RenderWindowError> {
        if block_range.len() != block_ids.len() || block_range.len() != local_height_index.len() {
            return Err(RenderWindowError::LengthMismatch {
                block_range_len: block_range.len(),
                block_ids_len: block_ids.len(),
                local_heights_len: local_height_index.len(),
            });
        }

        let entities = block_ids
            .iter()
            .copied()
            .map(|block_id| {
                (
                    block_id,
                    BlockEntityHandle {
                        block_id,
                        generation,
                    },
                )
            })
            .collect();

        Ok(Self {
            page_range,
            block_range,
            entities,
            local_height_index,
            content: RenderWindowContent::Loaded,
        })
    }

    pub fn placeholder(placeholder: PlaceholderWindow) -> Self {
        let local_height_index = BlockHeightIndex::new([HeightEstimate::new(
            placeholder.height,
            cditor_core::layout::HeightConfidence::Default,
            0.0,
        )])
        .expect("placeholder height is validated before construction");

        Self {
            page_range: placeholder.page_range.clone(),
            block_range: placeholder.block_range.clone(),
            entities: HashMap::new(),
            local_height_index,
            content: RenderWindowContent::Placeholder(placeholder),
        }
    }

    pub fn placeholder_for_page(
        page_layout_index: &PageLayoutIndex,
        page_index: usize,
    ) -> Result<Self, RenderWindowError> {
        let page =
            page_layout_index
                .pages
                .get(page_index)
                .ok_or(RenderWindowError::PageOutOfBounds {
                    page: page_index,
                    len: page_layout_index.page_count(),
                })?;
        Ok(Self::placeholder(PlaceholderWindow {
            page_range: page_index..page_index + 1,
            block_range: page.block_start..page.block_end(),
            height: page.height,
            target_anchor: None,
        }))
    }

    pub fn is_placeholder(&self) -> bool {
        matches!(self.content, RenderWindowContent::Placeholder(_))
    }

    pub fn contains_block_index(&self, block_index: usize) -> bool {
        self.block_range.start <= block_index && block_index < self.block_range.end
    }

    pub fn contains_page(&self, page_index: usize) -> bool {
        self.page_range.start <= page_index && page_index < self.page_range.end
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn height(&self) -> f64 {
        self.local_height_index.total_height()
    }

    pub fn replace_placeholder_with_loaded(
        self,
        loaded: RenderWindow,
        device_px_tolerance: f64,
    ) -> Result<(RenderWindow, AnchorRestoreCheck), RenderWindowError> {
        let RenderWindowContent::Placeholder(placeholder) = self.content else {
            return Err(RenderWindowError::NotPlaceholder);
        };
        if loaded.is_placeholder() {
            return Err(RenderWindowError::ReplacementIsPlaceholder);
        }

        let height_delta = loaded.height() - placeholder.height;
        let jitter_device_px = height_delta.abs();
        Ok((
            loaded,
            AnchorRestoreCheck {
                height_delta,
                jitter_device_px,
                within_tolerance: jitter_device_px <= device_px_tolerance,
                should_restore_anchor: true,
            },
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderWindowContent {
    Loaded,
    Placeholder(PlaceholderWindow),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceholderWindow {
    pub page_range: Range<usize>,
    pub block_range: Range<usize>,
    pub height: f64,
    pub target_anchor: Option<ScrollAnchor>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorRestoreCheck {
    pub height_delta: f64,
    pub jitter_device_px: f64,
    pub within_tolerance: bool,
    pub should_restore_anchor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderWindowError {
    LengthMismatch {
        block_range_len: usize,
        block_ids_len: usize,
        local_heights_len: usize,
    },
    PageOutOfBounds {
        page: usize,
        len: usize,
    },
    NotPlaceholder,
    ReplacementIsPlaceholder,
}

impl Display for RenderWindowError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthMismatch {
                block_range_len,
                block_ids_len,
                local_heights_len,
            } => write!(
                formatter,
                "render window length mismatch: block_range {block_range_len}, block ids {block_ids_len}, local heights {local_heights_len}"
            ),
            Self::PageOutOfBounds { page, len } => {
                write!(
                    formatter,
                    "render window page out of bounds: page {page}, len {len}"
                )
            }
            Self::NotPlaceholder => write!(formatter, "render window is not a placeholder"),
            Self::ReplacementIsPlaceholder => {
                write!(formatter, "replacement window is also a placeholder")
            }
        }
    }
}

impl Error for RenderWindowError {}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::layout::{HeightConfidence, PagePolicy};

    #[test]
    fn loaded_window_tracks_page_block_ranges_entities_and_local_heights() {
        let heights = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();
        let window = RenderWindow::loaded(2..3, 20..23, &[101, 102, 103], heights, 7).unwrap();

        assert_eq!(window.page_range, 2..3);
        assert_eq!(window.block_range, 20..23);
        assert_eq!(window.entity_count(), 3);
        assert_eq!(window.entities[&102].generation, 7);
        assert_eq!(window.height(), 60.0);
        assert!(!window.is_placeholder());
        assert!(window.contains_page(2));
        assert!(window.contains_block_index(22));
        assert!(!window.contains_block_index(23));
    }

    #[test]
    fn far_page_jump_can_show_placeholder_with_page_height() {
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

        let placeholder = RenderWindow::placeholder_for_page(&page_index, 2).unwrap();

        assert!(placeholder.is_placeholder());
        assert_eq!(placeholder.page_range, 2..3);
        assert_eq!(placeholder.block_range, 20..25);
        assert_eq!(placeholder.height(), page_index.pages[2].height);
        assert_eq!(placeholder.entity_count(), 0);
    }

    #[test]
    fn replacing_placeholder_reports_anchor_restore_jitter() {
        let placeholder = RenderWindow::placeholder(PlaceholderWindow {
            page_range: 1..2,
            block_range: 10..12,
            height: 30.0,
            target_anchor: Some(ScrollAnchor {
                block_id: 10,
                offset_in_block: 0.0,
                viewport_y: 0.0,
            }),
        });
        let loaded_heights = BlockHeightIndex::new([
            HeightEstimate::new(10.4, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(20.1, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();
        let loaded = RenderWindow::loaded(1..2, 10..12, &[10, 11], loaded_heights, 1).unwrap();

        let (_window, check) = placeholder
            .replace_placeholder_with_loaded(loaded, 1.0)
            .unwrap();

        assert!((check.height_delta - 0.5).abs() < f64::EPSILON);
        assert!(check.within_tolerance);
        assert!(check.should_restore_anchor);
    }

    #[test]
    fn entity_count_is_window_scoped_not_document_scoped() {
        let heights = BlockHeightIndex::new(
            (0..1_000).map(|_| HeightEstimate::new(24.0, HeightConfidence::Default, 0.0)),
        )
        .unwrap();
        let block_ids: Vec<_> = (1..=1_000).collect();
        let window = RenderWindow::loaded(0..1, 0..1_000, &block_ids, heights, 1).unwrap();

        assert_eq!(window.entity_count(), 1_000);
    }
}
