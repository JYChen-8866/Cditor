use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use cditor_core::edit::{BidiDirection, DocumentSelection, TextAffinity, TextPosition};
use cditor_core::ids::BlockId;
use cditor_core::version::LayoutVersionNumber;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualMovement {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualRun {
    pub logical_range: Range<usize>,
    pub x_range: Range<f64>,
    pub direction: BidiDirection,
}

impl VisualRun {
    pub fn contains_offset(&self, offset: usize) -> bool {
        self.logical_range.start <= offset && offset <= self.logical_range.end
    }

    fn caret_x(&self, offset: usize) -> f64 {
        if self.logical_range.start == self.logical_range.end {
            return self.x_range.start;
        }
        let logical_width = (self.logical_range.end - self.logical_range.start) as f64;
        let visual_width = self.x_range.end - self.x_range.start;
        let ratio = (offset.saturating_sub(self.logical_range.start) as f64 / logical_width)
            .clamp(0.0, 1.0);
        match self.direction {
            BidiDirection::Rtl => self.x_range.end - ratio * visual_width,
            BidiDirection::Ltr | BidiDirection::Neutral => {
                self.x_range.start + ratio * visual_width
            }
        }
    }

    fn hit_offset(&self, x: f64) -> usize {
        if self.logical_range.start == self.logical_range.end {
            return self.logical_range.start;
        }
        let visual_width = (self.x_range.end - self.x_range.start).max(1.0);
        let visual_ratio = ((x - self.x_range.start) / visual_width).clamp(0.0, 1.0);
        let logical_ratio = match self.direction {
            BidiDirection::Rtl => 1.0 - visual_ratio,
            BidiDirection::Ltr | BidiDirection::Neutral => visual_ratio,
        };
        self.logical_range.start
            + ((self.logical_range.end - self.logical_range.start) as f64 * logical_ratio).round()
                as usize
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualLineLayout {
    pub block_id: BlockId,
    pub line_index: usize,
    pub logical_range: Range<usize>,
    pub visual_runs: Vec<VisualRun>,
    pub baseline: f64,
    pub height: f64,
    pub y: f64,
    pub soft_wrap_start: bool,
    pub soft_wrap_end: bool,
}

impl VisualLineLayout {
    pub fn hit_test(&self, x: f64, y: f64) -> Option<TextPosition> {
        if y < self.y || y > self.y + self.height {
            return None;
        }
        let run = self
            .visual_runs
            .iter()
            .find(|run| x >= run.x_range.start && x <= run.x_range.end)
            .or_else(|| {
                self.visual_runs.iter().min_by(|a, b| {
                    distance_to_range(x, &a.x_range)
                        .partial_cmp(&distance_to_range(x, &b.x_range))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })?;
        let offset = run
            .hit_offset(x)
            .clamp(self.logical_range.start, self.logical_range.end);
        Some(TextPosition {
            block_id: self.block_id,
            offset,
            affinity: self.affinity_for_offset(offset),
        })
    }

    pub fn caret_rect(&self, position: TextPosition) -> Option<Rect> {
        if position.block_id != self.block_id
            || position.offset < self.logical_range.start
            || position.offset > self.logical_range.end
        {
            return None;
        }
        let run = self
            .visual_runs
            .iter()
            .find(|run| run.contains_offset(position.offset))?;
        Some(Rect {
            x: run.caret_x(position.offset),
            y: self.y,
            width: 1.0,
            height: self.height,
        })
    }

    pub fn move_visually(
        &self,
        position: TextPosition,
        movement: VisualMovement,
    ) -> Option<TextPosition> {
        if position.block_id != self.block_id {
            return None;
        }
        let stops = self.visual_caret_stops();
        let current = stops.iter().position(|offset| *offset == position.offset)?;
        let next = match movement {
            VisualMovement::Left => current.checked_sub(1)?,
            VisualMovement::Right => (current + 1 < stops.len()).then_some(current + 1)?,
        };
        let offset = stops[next];
        Some(TextPosition {
            block_id: self.block_id,
            offset,
            affinity: self.affinity_for_offset(offset),
        })
    }

    pub fn word_selection_at(
        &self,
        text: &str,
        position: TextPosition,
    ) -> Option<DocumentSelection> {
        if position.block_id != self.block_id || position.offset > text.len() {
            return None;
        }
        for (start, word) in text.unicode_word_indices() {
            let end = start + word.len();
            if start <= position.offset && position.offset <= end {
                return Some(DocumentSelection {
                    anchor: TextPosition::downstream(self.block_id, start),
                    focus: TextPosition::downstream(self.block_id, end),
                });
            }
        }
        None
    }

    fn visual_caret_stops(&self) -> Vec<usize> {
        let mut stops = Vec::new();
        for run in &self.visual_runs {
            match run.direction {
                BidiDirection::Rtl => {
                    for offset in (run.logical_range.start..=run.logical_range.end).rev() {
                        if stops.last().copied() != Some(offset) {
                            stops.push(offset);
                        }
                    }
                }
                BidiDirection::Ltr | BidiDirection::Neutral => {
                    for offset in run.logical_range.start..=run.logical_range.end {
                        if stops.last().copied() != Some(offset) {
                            stops.push(offset);
                        }
                    }
                }
            }
        }
        stops
    }

    fn affinity_for_offset(&self, offset: usize) -> TextAffinity {
        if offset == self.logical_range.start && self.soft_wrap_start {
            TextAffinity::Downstream
        } else if offset == self.logical_range.end && self.soft_wrap_end {
            TextAffinity::Upstream
        } else {
            TextAffinity::Downstream
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaretGeometryCache {
    pub block_id: BlockId,
    pub content_version: u64,
    pub layout_version: LayoutVersionNumber,
    pub line_boxes: Vec<VisualLineLayout>,
}

impl CaretGeometryCache {
    pub fn caret_rect(
        &self,
        position: TextPosition,
        content_version: u64,
        layout_version: LayoutVersionNumber,
    ) -> Result<Rect, HitTestError> {
        self.ensure_current(position.block_id, content_version, layout_version)?;
        self.line_boxes
            .iter()
            .find_map(|line| line.caret_rect(position))
            .ok_or(HitTestError::PositionOutsideCachedLines)
    }

    pub fn hit_test(
        &self,
        x: f64,
        y: f64,
        content_version: u64,
        layout_version: LayoutVersionNumber,
    ) -> Result<TextPosition, HitTestError> {
        self.ensure_current(self.block_id, content_version, layout_version)?;
        self.line_boxes
            .iter()
            .find_map(|line| line.hit_test(x, y))
            .ok_or(HitTestError::PointOutsideCachedLines)
    }

    pub fn ime_candidate_rect(
        &self,
        position: TextPosition,
        content_version: u64,
        layout_version: LayoutVersionNumber,
    ) -> Result<Rect, HitTestError> {
        self.caret_rect(position, content_version, layout_version)
    }

    fn ensure_current(
        &self,
        block_id: BlockId,
        content_version: u64,
        layout_version: LayoutVersionNumber,
    ) -> Result<(), HitTestError> {
        if self.block_id != block_id {
            return Err(HitTestError::BlockMismatch {
                expected: self.block_id,
                actual: block_id,
            });
        }
        if self.content_version != content_version || self.layout_version != layout_version {
            return Err(HitTestError::StaleGeometry {
                cached_content_version: self.content_version,
                requested_content_version: content_version,
                cached_layout_version: self.layout_version,
                requested_layout_version: layout_version,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitTestError {
    BlockMismatch {
        expected: BlockId,
        actual: BlockId,
    },
    StaleGeometry {
        cached_content_version: u64,
        requested_content_version: u64,
        cached_layout_version: LayoutVersionNumber,
        requested_layout_version: LayoutVersionNumber,
    },
    PointOutsideCachedLines,
    PositionOutsideCachedLines,
}

fn distance_to_range(x: f64, range: &Range<f64>) -> f64 {
    if x < range.start {
        range.start - x
    } else if x > range.end {
        x - range.end
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidi_hit_test_maps_visual_x_to_logical_text_position() {
        let line = bidi_line();

        let ltr_hit = line.hit_test(15.0, 5.0).unwrap();
        let rtl_hit = line.hit_test(45.0, 5.0).unwrap();

        assert_eq!(ltr_hit.block_id, 42);
        assert!(ltr_hit.offset <= 3);
        assert!(rtl_hit.offset >= 3 && rtl_hit.offset <= 6);
    }

    #[test]
    fn bidi_visual_movement_uses_visual_not_logical_order() {
        let line = bidi_line();
        let position = TextPosition::downstream(42, 3);

        let right = line.move_visually(position, VisualMovement::Right).unwrap();

        assert_eq!(right.offset, 6);
    }

    #[test]
    fn soft_wrap_boundary_preserves_affinity_for_caret() {
        let line = VisualLineLayout {
            block_id: 42,
            line_index: 1,
            logical_range: 10..20,
            visual_runs: vec![VisualRun {
                logical_range: 10..20,
                x_range: 0.0..100.0,
                direction: BidiDirection::Ltr,
            }],
            baseline: 12.0,
            height: 16.0,
            y: 20.0,
            soft_wrap_start: true,
            soft_wrap_end: true,
        };

        assert_eq!(
            line.hit_test(0.0, 25.0).unwrap().affinity,
            TextAffinity::Downstream
        );
        assert_eq!(
            line.hit_test(100.0, 25.0).unwrap().affinity,
            TextAffinity::Upstream
        );
    }

    #[test]
    fn text_position_to_caret_rect_uses_visual_run_direction() {
        let line = bidi_line();

        let rect = line.caret_rect(TextPosition::downstream(42, 6)).unwrap();

        assert_eq!(rect.x, 30.0);
        assert_eq!(rect.y, 0.0);
    }

    #[test]
    fn double_click_word_selection_expands_from_hit_offset() {
        let line = VisualLineLayout {
            block_id: 42,
            line_index: 0,
            logical_range: 0..11,
            visual_runs: vec![VisualRun {
                logical_range: 0..11,
                x_range: 0.0..110.0,
                direction: BidiDirection::Ltr,
            }],
            baseline: 12.0,
            height: 16.0,
            y: 0.0,
            soft_wrap_start: false,
            soft_wrap_end: false,
        };

        let selection = line
            .word_selection_at("hello world", TextPosition::downstream(42, 7))
            .unwrap();

        assert_eq!(selection.anchor.offset, 6);
        assert_eq!(selection.focus.offset, 11);
    }

    #[test]
    fn ime_candidate_rect_rejects_stale_geometry() {
        let cache = CaretGeometryCache {
            block_id: 42,
            content_version: 10,
            layout_version: 20,
            line_boxes: vec![bidi_line()],
        };

        let error = cache
            .ime_candidate_rect(TextPosition::downstream(42, 1), 11, 20)
            .unwrap_err();

        assert!(matches!(error, HitTestError::StaleGeometry { .. }));
    }

    fn bidi_line() -> VisualLineLayout {
        VisualLineLayout {
            block_id: 42,
            line_index: 0,
            logical_range: 0..9,
            visual_runs: vec![
                VisualRun {
                    logical_range: 0..3,
                    x_range: 0.0..30.0,
                    direction: BidiDirection::Ltr,
                },
                VisualRun {
                    logical_range: 3..6,
                    x_range: 30.0..60.0,
                    direction: BidiDirection::Rtl,
                },
                VisualRun {
                    logical_range: 6..9,
                    x_range: 60.0..90.0,
                    direction: BidiDirection::Ltr,
                },
            ],
            baseline: 12.0,
            height: 16.0,
            y: 0.0,
            soft_wrap_start: false,
            soft_wrap_end: false,
        }
    }
}
