use std::collections::HashSet;

use crate::ids::BlockId;
use serde::{Deserialize, Serialize};

use super::{BlockHeightIndex, BlockHeightIndexError, HeightEstimate};

pub const COLUMN_WEIGHT_TOTAL: u32 = 1_000_000;
pub const DEFAULT_COLUMN_GAP_PX: f64 = 24.0;
pub const MIN_COLUMN_WIDTH_PX: f64 = 120.0;
pub const MAX_COLUMNS_PER_GROUP: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSpec {
    pub block_id: BlockId,
    pub weight: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnLayoutTrack {
    pub block_id: BlockId,
    pub x: f64,
    pub width: f64,
    pub content_height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnsLayout {
    pub group_id: BlockId,
    pub width: f64,
    pub height: f64,
    pub gap: f64,
    pub tracks: Vec<ColumnLayoutTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnsLayoutError {
    InvalidColumnCount(usize),
    DuplicateColumn(BlockId),
    ZeroWeight(BlockId),
    InvalidGeometry,
    InvalidResizeBoundary(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnsLayoutModel {
    group_id: BlockId,
    columns: Vec<ColumnSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnsChildHeightIndex {
    columns: Vec<(BlockId, BlockHeightIndex)>,
    group_height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnsHeightChange {
    pub column_id: BlockId,
    pub child_index: usize,
    pub old_column_height: f64,
    pub new_column_height: f64,
    pub old_group_height: f64,
    pub new_group_height: f64,
    pub group_delta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnsHeightIndexError {
    ColumnOrderMismatch,
    MissingColumn(BlockId),
    Height(BlockHeightIndexError),
}

impl From<BlockHeightIndexError> for ColumnsHeightIndexError {
    fn from(value: BlockHeightIndexError) -> Self {
        Self::Height(value)
    }
}

impl ColumnsChildHeightIndex {
    pub fn new(
        model: &ColumnsLayoutModel,
        columns: Vec<(BlockId, Vec<HeightEstimate>)>,
    ) -> Result<Self, ColumnsHeightIndexError> {
        if model
            .columns()
            .iter()
            .map(|column| column.block_id)
            .ne(columns.iter().map(|(column_id, _)| *column_id))
        {
            return Err(ColumnsHeightIndexError::ColumnOrderMismatch);
        }
        let columns = columns
            .into_iter()
            .map(|(column_id, estimates)| Ok((column_id, BlockHeightIndex::new(estimates)?)))
            .collect::<Result<Vec<_>, ColumnsHeightIndexError>>()?;
        let group_height = columns
            .iter()
            .map(|(_, index)| index.total_height())
            .fold(0.0, f64::max);
        Ok(Self {
            columns,
            group_height,
        })
    }

    pub fn group_height(&self) -> f64 {
        self.group_height
    }

    pub fn column_index(&self, column_id: BlockId) -> Option<&BlockHeightIndex> {
        self.columns
            .iter()
            .find(|(candidate, _)| *candidate == column_id)
            .map(|(_, index)| index)
    }

    pub fn update_child_height(
        &mut self,
        column_id: BlockId,
        child_index: usize,
        height: f64,
    ) -> Result<ColumnsHeightChange, ColumnsHeightIndexError> {
        let old_group_height = self.group_height;
        let (_, column) = self
            .columns
            .iter_mut()
            .find(|(candidate, _)| *candidate == column_id)
            .ok_or(ColumnsHeightIndexError::MissingColumn(column_id))?;
        let old_column_height = column.total_height();
        column.update_height(child_index, height)?;
        let new_column_height = column.total_height();
        self.group_height = self
            .columns
            .iter()
            .map(|(_, index)| index.total_height())
            .fold(0.0, f64::max);
        Ok(ColumnsHeightChange {
            column_id,
            child_index,
            old_column_height,
            new_column_height,
            old_group_height,
            new_group_height: self.group_height,
            group_delta: self.group_height - old_group_height,
        })
    }
}

impl ColumnsLayoutModel {
    pub fn equal(group_id: BlockId, column_ids: Vec<BlockId>) -> Result<Self, ColumnsLayoutError> {
        validate_column_ids(&column_ids)?;
        let count = column_ids.len() as u32;
        let base = COLUMN_WEIGHT_TOTAL / count;
        let remainder = COLUMN_WEIGHT_TOTAL % count;
        let columns = column_ids
            .into_iter()
            .enumerate()
            .map(|(index, block_id)| ColumnSpec {
                block_id,
                weight: base + u32::from((index as u32) < remainder),
            })
            .collect();
        Ok(Self { group_id, columns })
    }

    pub fn from_weights(
        group_id: BlockId,
        columns: Vec<ColumnSpec>,
    ) -> Result<Self, ColumnsLayoutError> {
        validate_column_ids(
            &columns
                .iter()
                .map(|column| column.block_id)
                .collect::<Vec<_>>(),
        )?;
        if let Some(column) = columns.iter().find(|column| column.weight == 0) {
            return Err(ColumnsLayoutError::ZeroWeight(column.block_id));
        }
        let weights = normalize_weights(columns.iter().map(|column| column.weight));
        Ok(Self {
            group_id,
            columns: columns
                .into_iter()
                .zip(weights)
                .map(|(column, weight)| ColumnSpec { weight, ..column })
                .collect(),
        })
    }

    pub fn group_id(&self) -> BlockId {
        self.group_id
    }

    pub fn columns(&self) -> &[ColumnSpec] {
        &self.columns
    }

    pub fn layout(
        &self,
        available_width: f64,
        gap: f64,
        content_heights: &[f64],
    ) -> Result<ColumnsLayout, ColumnsLayoutError> {
        if !available_width.is_finite()
            || !gap.is_finite()
            || available_width < 0.0
            || gap < 0.0
            || content_heights.len() != self.columns.len()
            || content_heights
                .iter()
                .any(|height| !height.is_finite() || *height < 0.0)
        {
            return Err(ColumnsLayoutError::InvalidGeometry);
        }
        let gap_total = gap * self.columns.len().saturating_sub(1) as f64;
        let content_width = (available_width - gap_total).max(0.0);
        let raw_widths = self
            .columns
            .iter()
            .map(|column| content_width * f64::from(column.weight) / f64::from(COLUMN_WEIGHT_TOTAL))
            .collect::<Vec<_>>();
        let widths = enforce_minimum_widths(raw_widths, content_width);
        let mut x = 0.0;
        let tracks = self
            .columns
            .iter()
            .zip(widths)
            .zip(content_heights.iter().copied())
            .map(|((column, width), content_height)| {
                let track = ColumnLayoutTrack {
                    block_id: column.block_id,
                    x,
                    width,
                    content_height,
                };
                x += width + gap;
                track
            })
            .collect::<Vec<_>>();
        Ok(ColumnsLayout {
            group_id: self.group_id,
            width: available_width,
            height: content_heights.iter().copied().fold(0.0, f64::max),
            gap,
            tracks,
        })
    }

    pub fn resize_boundary(
        &mut self,
        boundary: usize,
        delta_px: f64,
        available_width: f64,
        gap: f64,
    ) -> Result<bool, ColumnsLayoutError> {
        if boundary + 1 >= self.columns.len() {
            return Err(ColumnsLayoutError::InvalidResizeBoundary(boundary));
        }
        if !delta_px.is_finite() || !available_width.is_finite() || !gap.is_finite() {
            return Err(ColumnsLayoutError::InvalidGeometry);
        }
        let gap_total = gap * self.columns.len().saturating_sub(1) as f64;
        let content_width = (available_width - gap_total).max(0.0);
        if content_width <= 0.0 {
            return Ok(false);
        }
        let left = self.columns[boundary].weight;
        let right = self.columns[boundary + 1].weight;
        let pair = left + right;
        let min_weight = ((MIN_COLUMN_WIDTH_PX / content_width) * f64::from(COLUMN_WEIGHT_TOTAL))
            .ceil()
            .max(1.0) as u32;
        let effective_min = min_weight.min(pair / 2);
        let delta_weight =
            (delta_px / content_width * f64::from(COLUMN_WEIGHT_TOTAL)).round() as i64;
        let next_left = (i64::from(left) + delta_weight)
            .clamp(i64::from(effective_min), i64::from(pair - effective_min))
            as u32;
        if next_left == left {
            return Ok(false);
        }
        self.columns[boundary].weight = next_left;
        self.columns[boundary + 1].weight = pair - next_left;
        Ok(true)
    }
}

impl ColumnsLayout {
    pub fn column_at_x(&self, x: f64) -> Option<BlockId> {
        if !x.is_finite() {
            return None;
        }
        self.tracks
            .iter()
            .min_by(|left, right| {
                distance_to_track(x, left).total_cmp(&distance_to_track(x, right))
            })
            .map(|track| track.block_id)
    }

    pub fn adjacent_column(&self, block_id: BlockId, direction: i32) -> Option<BlockId> {
        let index = self
            .tracks
            .iter()
            .position(|track| track.block_id == block_id)?;
        let next = if direction < 0 {
            index.checked_sub(1)?
        } else {
            index.checked_add(1)?
        };
        self.tracks.get(next).map(|track| track.block_id)
    }
}

fn validate_column_ids(column_ids: &[BlockId]) -> Result<(), ColumnsLayoutError> {
    if !(2..=MAX_COLUMNS_PER_GROUP).contains(&column_ids.len()) {
        return Err(ColumnsLayoutError::InvalidColumnCount(column_ids.len()));
    }
    let mut seen = HashSet::with_capacity(column_ids.len());
    for block_id in column_ids {
        if !seen.insert(*block_id) {
            return Err(ColumnsLayoutError::DuplicateColumn(*block_id));
        }
    }
    Ok(())
}

fn normalize_weights(weights: impl IntoIterator<Item = u32>) -> Vec<u32> {
    let weights = weights.into_iter().collect::<Vec<_>>();
    let total = weights.iter().map(|weight| u64::from(*weight)).sum::<u64>();
    let mut normalized = weights
        .iter()
        .map(|weight| (u64::from(*weight) * u64::from(COLUMN_WEIGHT_TOTAL) / total) as u32)
        .collect::<Vec<_>>();
    let remainder = COLUMN_WEIGHT_TOTAL - normalized.iter().sum::<u32>();
    let count = normalized.len();
    for index in 0..remainder as usize {
        normalized[index % count] += 1;
    }
    normalized
}

fn enforce_minimum_widths(raw: Vec<f64>, total: f64) -> Vec<f64> {
    if total < MIN_COLUMN_WIDTH_PX * raw.len() as f64 {
        return vec![total / raw.len() as f64; raw.len()];
    }
    let mut widths = raw;
    let mut fixed = vec![false; widths.len()];
    loop {
        let newly_fixed = widths
            .iter()
            .enumerate()
            .filter(|(index, width)| !fixed[*index] && **width < MIN_COLUMN_WIDTH_PX)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if newly_fixed.is_empty() {
            break;
        }
        for index in newly_fixed {
            fixed[index] = true;
            widths[index] = MIN_COLUMN_WIDTH_PX;
        }
        let fixed_total = widths
            .iter()
            .enumerate()
            .filter(|(index, _)| fixed[*index])
            .map(|(_, width)| *width)
            .sum::<f64>();
        let flexible_raw = widths
            .iter()
            .enumerate()
            .filter(|(index, _)| !fixed[*index])
            .map(|(_, width)| *width)
            .sum::<f64>();
        for (index, width) in widths.iter_mut().enumerate() {
            if !fixed[index] {
                *width = if flexible_raw > 0.0 {
                    *width / flexible_raw * (total - fixed_total)
                } else {
                    0.0
                };
            }
        }
    }
    widths
}

fn distance_to_track(x: f64, track: &ColumnLayoutTrack) -> f64 {
    if x < track.x {
        track.x - x
    } else if x > track.x + track.width {
        x - (track.x + track.width)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_columns_fill_width_and_group_uses_tallest_content() {
        let model = ColumnsLayoutModel::equal(10, vec![11, 12, 13]).unwrap();
        let layout = model.layout(648.0, 24.0, &[80.0, 220.0, 120.0]).unwrap();
        assert_eq!(layout.height, 220.0);
        assert_eq!(layout.tracks.len(), 3);
        assert!((layout.tracks.iter().map(|track| track.width).sum::<f64>() - 600.0).abs() < 0.001);
        assert_eq!(layout.tracks[1].x, layout.tracks[0].width + 24.0);
    }

    #[test]
    fn resize_preserves_total_weight_and_clamps_both_columns() {
        let mut model = ColumnsLayoutModel::equal(10, vec![11, 12]).unwrap();
        assert!(model.resize_boundary(0, 10_000.0, 600.0, 24.0).unwrap());
        assert_eq!(
            model
                .columns()
                .iter()
                .map(|column| column.weight)
                .sum::<u32>(),
            COLUMN_WEIGHT_TOTAL
        );
        let layout = model.layout(600.0, 24.0, &[10.0, 10.0]).unwrap();
        assert!(
            layout
                .tracks
                .iter()
                .all(|track| track.width >= MIN_COLUMN_WIDTH_PX)
        );
    }

    #[test]
    fn narrow_layout_degrades_to_equal_nonnegative_tracks() {
        let model = ColumnsLayoutModel::from_weights(
            10,
            vec![
                ColumnSpec {
                    block_id: 11,
                    weight: 9,
                },
                ColumnSpec {
                    block_id: 12,
                    weight: 1,
                },
            ],
        )
        .unwrap();
        let layout = model.layout(100.0, 24.0, &[0.0, 0.0]).unwrap();
        assert_eq!(layout.tracks[0].width, 38.0);
        assert_eq!(layout.tracks[1].width, 38.0);
    }

    #[test]
    fn hit_test_and_horizontal_navigation_use_visual_column_geometry() {
        let model = ColumnsLayoutModel::equal(10, vec![11, 12, 13]).unwrap();
        let layout = model.layout(648.0, 24.0, &[10.0; 3]).unwrap();
        assert_eq!(layout.column_at_x(-50.0), Some(11));
        assert_eq!(layout.column_at_x(648.0), Some(13));
        assert_eq!(layout.adjacent_column(12, -1), Some(11));
        assert_eq!(layout.adjacent_column(12, 1), Some(13));
        assert_eq!(layout.adjacent_column(13, 1), None);
    }

    #[test]
    fn invalid_count_duplicate_ids_and_zero_weights_are_rejected() {
        assert_eq!(
            ColumnsLayoutModel::equal(10, vec![11]),
            Err(ColumnsLayoutError::InvalidColumnCount(1))
        );
        assert_eq!(
            ColumnsLayoutModel::equal(10, vec![11, 11]),
            Err(ColumnsLayoutError::DuplicateColumn(11))
        );
        assert_eq!(
            ColumnsLayoutModel::from_weights(
                10,
                vec![
                    ColumnSpec {
                        block_id: 11,
                        weight: 1
                    },
                    ColumnSpec {
                        block_id: 12,
                        weight: 0
                    }
                ],
            ),
            Err(ColumnsLayoutError::ZeroWeight(12))
        );
    }

    #[test]
    fn width_and_resize_invariants_hold_across_counts_and_extreme_drags() {
        for count in 2..=MAX_COLUMNS_PER_GROUP {
            let ids = (0..count).map(|index| 100 + index as u64).collect();
            let mut model = ColumnsLayoutModel::equal(10, ids).unwrap();
            for width in [0.0, 80.0, 320.0, 720.0, 1920.0] {
                let layout = model
                    .layout(width, DEFAULT_COLUMN_GAP_PX, &vec![32.0; count])
                    .unwrap();
                assert!(layout.tracks.iter().all(|track| track.width >= 0.0));
                let occupied = layout.tracks.iter().map(|track| track.width).sum::<f64>()
                    + DEFAULT_COLUMN_GAP_PX * count.saturating_sub(1) as f64;
                assert!(
                    (occupied - width.max(DEFAULT_COLUMN_GAP_PX * (count - 1) as f64)).abs() < 0.01
                );
            }
            for step in 0..1000 {
                let boundary = step % (count - 1);
                let delta = if step % 2 == 0 { 10_000.0 } else { -10_000.0 };
                model
                    .resize_boundary(boundary, delta, 1920.0, DEFAULT_COLUMN_GAP_PX)
                    .unwrap();
                assert_eq!(
                    model
                        .columns()
                        .iter()
                        .map(|column| column.weight)
                        .sum::<u32>(),
                    COLUMN_WEIGHT_TOTAL
                );
                assert!(model.columns().iter().all(|column| column.weight > 0));
            }
        }
    }

    #[test]
    fn child_height_indexes_update_one_column_and_report_group_max_delta() {
        let model = ColumnsLayoutModel::equal(10, vec![11, 12]).unwrap();
        let estimate =
            |height| HeightEstimate::new(height, crate::layout::HeightConfidence::Exact, 0.0);
        let mut index = ColumnsChildHeightIndex::new(
            &model,
            vec![
                (11, vec![estimate(40.0), estimate(60.0)]),
                (12, vec![estimate(80.0), estimate(50.0)]),
            ],
        )
        .unwrap();
        assert_eq!(index.group_height(), 130.0);

        let change = index.update_child_height(11, 1, 120.0).unwrap();
        assert_eq!(change.old_column_height, 100.0);
        assert_eq!(change.new_column_height, 160.0);
        assert_eq!(change.old_group_height, 130.0);
        assert_eq!(change.new_group_height, 160.0);
        assert_eq!(change.group_delta, 30.0);
        assert_eq!(index.column_index(12).unwrap().total_height(), 130.0);
    }
}
