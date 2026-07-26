use std::{cmp::Ordering, ops::Range, sync::Arc};

use cditor_core::edit::TextAffinity;
use parley::{BreakReason, Layout, PositionedLayoutItem};

use crate::{
    TextBrush, TextLayoutPosition, TextLayoutRect, TextLayoutSelection, TextLineSnapshot,
    TextSnapshot,
};

const NEWLINE_WHITESPACE_WIDTH_RATIO: f32 = 0.25;

#[derive(Debug, Clone, PartialEq)]
pub struct TextCaretStop {
    pub requested: TextLayoutPosition,
    pub resolved: TextLayoutPosition,
    pub rect: TextLayoutRect,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct TextGeometrySnapshot {
    lines: Arc<[TextLineSnapshot]>,
    caret_stops: Arc<[TextCaretStop]>,
    visual_lines: Arc<[VisualLineGeometry]>,
    estimated_bytes: usize,
}

#[derive(Debug, Clone)]
struct VisualLineGeometry {
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
    explicit_break: bool,
    newline_width: f32,
    clusters: Arc<[VisualClusterGeometry]>,
}

#[derive(Debug, Clone)]
struct VisualClusterGeometry {
    text_range: Range<usize>,
    x0: f32,
    x1: f32,
    selection_x0: f32,
    is_rtl: bool,
    is_soft_line_break: bool,
    is_hard_line_break: bool,
    left: TextLayoutPosition,
    right: TextLayoutPosition,
}

#[derive(Debug, Clone, Copy)]
struct LogicalClusterRef {
    text_start: usize,
    text_end: usize,
    line_index: usize,
    visual_index: usize,
}

impl TextGeometrySnapshot {
    pub(crate) fn from_layout(
        text: &TextSnapshot,
        layout: &Layout<TextBrush>,
        display_scale: f32,
    ) -> Self {
        let lines = collect_lines(layout, display_scale);
        let visual_lines = collect_visual_lines(text.as_str(), layout, display_scale);
        let caret_stops = collect_caret_stops(text, &lines, &visual_lines);
        let estimated_bytes = estimate_bytes(&lines, &caret_stops, &visual_lines);
        Self {
            lines,
            caret_stops,
            visual_lines,
            estimated_bytes,
        }
    }

    pub fn lines(&self) -> &[TextLineSnapshot] {
        &self.lines
    }

    pub fn caret_stops(&self) -> &[TextCaretStop] {
        &self.caret_stops
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }

    pub(crate) fn position_for_point(&self, x: f32, y: f32) -> TextLayoutPosition {
        let Some((line_index, line)) = self.line_for_y(y) else {
            return TextLayoutPosition::downstream(0);
        };
        let Some(cluster) = line
            .clusters
            .iter()
            .find(|cluster| x <= cluster.x1)
            .or_else(|| line.clusters.last())
        else {
            return self
                .caret_stops
                .iter()
                .filter(|stop| stop.line_index == line_index)
                .map(|stop| stop.requested.offset)
                .max()
                .map(TextLayoutPosition::downstream)
                .unwrap_or_else(|| TextLayoutPosition::downstream(0));
        };
        if x <= cluster.x0 + (cluster.x1 - cluster.x0) * 0.5 {
            cluster.left
        } else {
            cluster.right
        }
    }

    pub(crate) fn caret_rect(
        &self,
        text: &str,
        position: TextLayoutPosition,
        width: f32,
    ) -> TextLayoutRect {
        let requested = TextLayoutPosition {
            offset: normalize_byte_offset(text, position.offset, position.affinity),
            affinity: position.affinity,
        };
        let stop = self
            .caret_stops
            .binary_search_by(|candidate| compare_positions(candidate.requested, requested))
            .ok()
            .and_then(|index| self.caret_stops.get(index))
            .or_else(|| nearest_caret_stop(&self.caret_stops, requested.offset));
        let Some(stop) = stop else {
            return TextLayoutRect {
                x: 0.0,
                y: 0.0,
                width: width.max(0.0),
                height: 0.0,
            };
        };
        TextLayoutRect {
            width: width.max(0.0),
            ..stop.rect
        }
    }

    pub(crate) fn selection_rects(
        &self,
        text: &str,
        selection: TextLayoutSelection,
    ) -> Vec<TextLayoutRect> {
        let (anchor, anchor_line) = self.resolved_selection_endpoint(text, selection.anchor);
        let (focus, focus_line) = self.resolved_selection_endpoint(text, selection.focus);
        if anchor.offset == focus.offset && anchor.affinity == focus.affinity {
            return Vec::new();
        }
        let range = anchor.offset.min(focus.offset)..anchor.offset.max(focus.offset);
        if range.is_empty() {
            return Vec::new();
        }
        let first_line = anchor_line.min(focus_line);
        let last_line = anchor_line.max(focus_line);
        let mut rects = Vec::new();
        for line_index in first_line..=last_line {
            let Some(line) = self.visual_lines.get(line_index) else {
                continue;
            };
            if line_index != first_line && line_index != last_line {
                push_rect(
                    &mut rects,
                    line.left,
                    line.right + line.newline_width,
                    line.top,
                    line.bottom,
                );
                continue;
            }
            push_partial_line_rects(&mut rects, line, &range, line_index != last_line);
        }
        rects
    }

    fn resolved_selection_endpoint(
        &self,
        text: &str,
        position: TextLayoutPosition,
    ) -> (TextLayoutPosition, usize) {
        let requested = TextLayoutPosition {
            offset: normalize_byte_offset(text, position.offset, position.affinity),
            affinity: position.affinity,
        };
        self.caret_stops
            .binary_search_by(|candidate| compare_positions(candidate.requested, requested))
            .ok()
            .and_then(|index| self.caret_stops.get(index))
            .map(|stop| (stop.resolved, stop.line_index))
            .unwrap_or_else(|| (requested, self.visual_lines.len().saturating_sub(1)))
    }

    fn line_for_y(&self, y: f32) -> Option<(usize, &VisualLineGeometry)> {
        if self.visual_lines.is_empty() {
            return None;
        }
        let index = self
            .visual_lines
            .partition_point(|line| y >= line.bottom)
            .min(self.visual_lines.len().saturating_sub(1));
        self.visual_lines.get(index).map(|line| (index, line))
    }
}

fn collect_lines(layout: &Layout<TextBrush>, scale: f32) -> Arc<[TextLineSnapshot]> {
    layout
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let metrics = line.metrics();
            TextLineSnapshot {
                index,
                text_range: line.text_range(),
                baseline: metrics.baseline / scale,
                top: metrics.block_min_coord / scale,
                bottom: metrics.block_max_coord / scale,
                advance: metrics.advance / scale,
                offset: metrics.offset / scale,
                logical_left: (metrics.offset + metrics.inline_min_coord) / scale,
                logical_right: (metrics.offset + metrics.inline_min_coord + metrics.advance)
                    / scale,
            }
        })
        .collect::<Vec<_>>()
        .into()
}

fn collect_caret_stops(
    text: &TextSnapshot,
    lines: &[TextLineSnapshot],
    visual_lines: &[VisualLineGeometry],
) -> Arc<[TextCaretStop]> {
    let logical_clusters = collect_logical_cluster_refs(visual_lines);
    let mut boundaries = text
        .as_str()
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    if boundaries.last().copied() != Some(text.len_bytes()) {
        boundaries.push(text.len_bytes());
    }
    let mut stops = Vec::with_capacity(boundaries.len().saturating_mul(2));
    let mut downstream_index = 0usize;
    for offset in boundaries {
        while downstream_index < logical_clusters.len()
            && offset >= logical_clusters[downstream_index].text_end
        {
            downstream_index += 1;
        }
        let downstream = logical_clusters
            .get(downstream_index)
            .copied()
            .filter(|cluster| cluster.text_start <= offset && offset < cluster.text_end);
        let resolved_offset = downstream
            .map(|cluster| cluster.text_start)
            .unwrap_or(text.len_bytes());
        let upstream = downstream_index
            .checked_sub(1)
            .and_then(|index| logical_clusters.get(index))
            .copied();
        for affinity in [TextAffinity::Upstream, TextAffinity::Downstream] {
            let requested = TextLayoutPosition { offset, affinity };
            let resolved = TextLayoutPosition {
                offset: resolved_offset,
                affinity: if downstream.is_none() {
                    TextAffinity::Upstream
                } else if resolved_offset == 0 {
                    TextAffinity::Downstream
                } else {
                    affinity
                },
            };
            let rect = materialized_caret_rect(
                resolved.affinity,
                upstream,
                downstream,
                lines,
                visual_lines,
            );
            let line_index = line_index_for_caret_y(lines, rect.y);
            stops.push(TextCaretStop {
                requested,
                resolved,
                rect,
                line_index,
            });
        }
    }
    stops.sort_by(|left, right| compare_positions(left.requested, right.requested));
    stops.into()
}

fn collect_visual_lines(
    text: &str,
    layout: &Layout<TextBrush>,
    scale: f32,
) -> Arc<[VisualLineGeometry]> {
    layout
        .lines()
        .map(|line| {
            let metrics = line.metrics();
            let mut run_offsets = vec![None; line.len()];
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                    let index = glyph_run.run().index();
                    let offset = glyph_run.offset() / scale;
                    run_offsets[index] = Some(
                        run_offsets[index]
                            .map(|current: f32| current.min(offset))
                            .unwrap_or(offset),
                    );
                }
            }
            let mut clusters = Vec::new();
            let mut fallback_x = (metrics.offset + metrics.inline_min_coord) / scale;
            for run in line.runs() {
                let mut x0 = run_offsets
                    .get(run.index())
                    .copied()
                    .flatten()
                    .unwrap_or(fallback_x);
                for cluster in run.visual_clusters() {
                    let x1 = x0 + cluster.advance() / scale;
                    let raw_range = cluster.text_range();
                    let range =
                        normalize_byte_offset(text, raw_range.start, TextAffinity::Downstream)
                            ..normalize_byte_offset(text, raw_range.end, TextAffinity::Upstream);
                    let (left, right) = if cluster.is_rtl() {
                        (
                            TextLayoutPosition {
                                offset: range.end,
                                affinity: TextAffinity::Upstream,
                            },
                            TextLayoutPosition::downstream(range.start),
                        )
                    } else {
                        let right = if cluster.is_line_break() == Some(BreakReason::Explicit) {
                            TextLayoutPosition::downstream(range.start)
                        } else {
                            TextLayoutPosition {
                                offset: range.end,
                                affinity: TextAffinity::Upstream,
                            }
                        };
                        (TextLayoutPosition::downstream(range.start), right)
                    };
                    clusters.push(VisualClusterGeometry {
                        text_range: range,
                        x0,
                        x1,
                        selection_x0: x0,
                        is_rtl: cluster.is_rtl(),
                        is_soft_line_break: cluster.is_soft_line_break(),
                        is_hard_line_break: cluster.is_hard_line_break(),
                        left,
                        right,
                    });
                    x0 = x1;
                }
                fallback_x = x0;
            }
            clusters.sort_by(|left, right| left.x0.total_cmp(&right.x0));
            for index in 1..clusters.len() {
                clusters[index].selection_x0 = clusters[index - 1].x1;
            }
            VisualLineGeometry {
                top: metrics.block_min_coord / scale,
                bottom: metrics.block_max_coord / scale,
                left: (metrics.offset + metrics.inline_min_coord) / scale,
                right: (metrics.offset + metrics.inline_min_coord + metrics.advance) / scale,
                explicit_break: line.break_reason() == BreakReason::Explicit,
                newline_width: if line.break_reason() == BreakReason::Explicit {
                    (metrics.ascent + metrics.descent) / scale * NEWLINE_WHITESPACE_WIDTH_RATIO
                } else {
                    0.0
                },
                clusters: clusters.into(),
            }
        })
        .collect::<Vec<_>>()
        .into()
}

fn collect_logical_cluster_refs(visual_lines: &[VisualLineGeometry]) -> Vec<LogicalClusterRef> {
    let mut clusters = visual_lines
        .iter()
        .enumerate()
        .flat_map(|(line_index, line)| {
            line.clusters
                .iter()
                .enumerate()
                .map(move |(visual_index, cluster)| LogicalClusterRef {
                    text_start: cluster.text_range.start,
                    text_end: cluster.text_range.end,
                    line_index,
                    visual_index,
                })
        })
        .filter(|cluster| cluster.text_start < cluster.text_end)
        .collect::<Vec<_>>();
    clusters.sort_by_key(|cluster| (cluster.text_start, cluster.text_end));
    clusters
}

fn materialized_caret_rect(
    affinity: TextAffinity,
    upstream: Option<LogicalClusterRef>,
    downstream: Option<LogicalClusterRef>,
    lines: &[TextLineSnapshot],
    visual_lines: &[VisualLineGeometry],
) -> TextLayoutRect {
    let (left, right) = visual_neighbors(affinity, upstream, downstream, visual_lines);
    match (left, right) {
        (Some(left), Some(right)) => {
            let cluster = visual_cluster(visual_lines, left);
            if is_end_of_visual_line(visual_lines, left) {
                if cluster.is_soft_line_break {
                    let at_end = (cluster.is_rtl && affinity == TextAffinity::Downstream)
                        || (!cluster.is_rtl && affinity == TextAffinity::Upstream);
                    if at_end {
                        cluster_caret_rect(visual_lines, left, true)
                    } else {
                        cluster_caret_rect(visual_lines, right, false)
                    }
                } else {
                    cluster_caret_rect(visual_lines, right, false)
                }
            } else {
                cluster_caret_rect(visual_lines, left, true)
            }
        }
        (Some(left), None) if visual_cluster(visual_lines, left).is_hard_line_break => {
            last_line_caret_rect(lines)
        }
        (Some(left), _) => cluster_caret_rect(visual_lines, left, true),
        (_, Some(right)) => cluster_caret_rect(visual_lines, right, false),
        _ => last_line_caret_rect(lines),
    }
}

fn visual_neighbors(
    affinity: TextAffinity,
    upstream: Option<LogicalClusterRef>,
    downstream: Option<LogicalClusterRef>,
    visual_lines: &[VisualLineGeometry],
) -> (Option<LogicalClusterRef>, Option<LogicalClusterRef>) {
    match affinity {
        TextAffinity::Upstream => match upstream {
            Some(cluster) if visual_cluster(visual_lines, cluster).is_rtl => {
                (previous_visual(visual_lines, cluster), Some(cluster))
            }
            Some(cluster) => (Some(cluster), next_visual(visual_lines, cluster)),
            None => match downstream {
                Some(cluster) if visual_cluster(visual_lines, cluster).is_rtl => {
                    (None, Some(cluster))
                }
                Some(cluster) => (Some(cluster), None),
                None => (None, None),
            },
        },
        TextAffinity::Downstream => match downstream {
            Some(cluster) if visual_cluster(visual_lines, cluster).is_rtl => {
                (Some(cluster), next_visual(visual_lines, cluster))
            }
            Some(cluster) => (previous_visual(visual_lines, cluster), Some(cluster)),
            None => match upstream {
                Some(cluster) if visual_cluster(visual_lines, cluster).is_rtl => {
                    (None, Some(cluster))
                }
                Some(cluster) => (Some(cluster), None),
                None => (None, None),
            },
        },
    }
}

fn visual_cluster(
    visual_lines: &[VisualLineGeometry],
    cluster: LogicalClusterRef,
) -> &VisualClusterGeometry {
    &visual_lines[cluster.line_index].clusters[cluster.visual_index]
}

fn is_end_of_visual_line(visual_lines: &[VisualLineGeometry], cluster: LogicalClusterRef) -> bool {
    cluster.visual_index + 1 == visual_lines[cluster.line_index].clusters.len()
}

fn previous_visual(
    visual_lines: &[VisualLineGeometry],
    cluster: LogicalClusterRef,
) -> Option<LogicalClusterRef> {
    if cluster.visual_index > 0 {
        return Some(LogicalClusterRef {
            visual_index: cluster.visual_index - 1,
            ..cluster
        });
    }
    let line_index = cluster.line_index.checked_sub(1)?;
    let visual_index = visual_lines[line_index].clusters.len().checked_sub(1)?;
    Some(LogicalClusterRef {
        text_start: 0,
        text_end: 0,
        line_index,
        visual_index,
    })
}

fn next_visual(
    visual_lines: &[VisualLineGeometry],
    cluster: LogicalClusterRef,
) -> Option<LogicalClusterRef> {
    if cluster.visual_index + 1 < visual_lines[cluster.line_index].clusters.len() {
        return Some(LogicalClusterRef {
            visual_index: cluster.visual_index + 1,
            ..cluster
        });
    }
    let line_index = cluster.line_index + 1;
    (!visual_lines.get(line_index)?.clusters.is_empty()).then_some(LogicalClusterRef {
        text_start: 0,
        text_end: 0,
        line_index,
        visual_index: 0,
    })
}

fn cluster_caret_rect(
    visual_lines: &[VisualLineGeometry],
    cluster: LogicalClusterRef,
    at_end: bool,
) -> TextLayoutRect {
    let line = &visual_lines[cluster.line_index];
    let cluster = visual_cluster(visual_lines, cluster);
    TextLayoutRect {
        x: if at_end { cluster.x1 } else { cluster.x0 },
        y: line.top,
        width: 0.0,
        height: line.bottom - line.top,
    }
}

fn last_line_caret_rect(lines: &[TextLineSnapshot]) -> TextLayoutRect {
    lines.last().map_or(
        TextLayoutRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
        |line| TextLayoutRect {
            x: line.offset,
            y: line.top,
            width: 0.0,
            height: line.bottom - line.top,
        },
    )
}

fn push_partial_line_rects(
    rects: &mut Vec<TextLayoutRect>,
    line: &VisualLineGeometry,
    range: &Range<usize>,
    include_newline: bool,
) {
    let mut current: Option<(f32, f32)> = None;
    let mut selected_clusters = 0usize;
    for cluster in line
        .clusters
        .iter()
        .filter(|cluster| range.contains(&cluster.text_range.start))
    {
        selected_clusters += 1;
        match current {
            Some((start, end)) if cluster.selection_x0 <= end + f32::EPSILON => {
                current = Some((start, end.max(cluster.x1)));
            }
            Some((start, end)) => {
                push_rect(rects, start, end, line.top, line.bottom);
                current = Some((cluster.selection_x0, cluster.x1));
            }
            None => current = Some((cluster.selection_x0, cluster.x1)),
        }
    }
    if let Some((start, mut end)) = current {
        if line.explicit_break
            && (include_newline || (selected_clusters != 0 && line.right == line.left))
        {
            end += line.newline_width;
        }
        push_rect(rects, start, end, line.top, line.bottom);
    }
}

fn push_rect(rects: &mut Vec<TextLayoutRect>, x0: f32, x1: f32, top: f32, bottom: f32) {
    if x1 > x0 {
        rects.push(TextLayoutRect {
            x: x0,
            y: top,
            width: x1 - x0,
            height: bottom - top,
        });
    }
}

fn compare_positions(left: TextLayoutPosition, right: TextLayoutPosition) -> Ordering {
    left.offset
        .cmp(&right.offset)
        .then_with(|| affinity_key(left.affinity).cmp(&affinity_key(right.affinity)))
}

const fn affinity_key(affinity: TextAffinity) -> u8 {
    match affinity {
        TextAffinity::Upstream => 0,
        TextAffinity::Downstream => 1,
    }
}

fn nearest_caret_stop(stops: &[TextCaretStop], offset: usize) -> Option<&TextCaretStop> {
    stops
        .iter()
        .min_by_key(|stop| stop.requested.offset.abs_diff(offset))
}

fn line_index_for_caret_y(lines: &[TextLineSnapshot], caret_y: f32) -> usize {
    // Font ascent/descent boxes of adjacent lines may overlap. A caret at the
    // next visual line's top therefore belongs to the last line that has
    // started, not the first line whose vertical box happens to contain it.
    lines
        .partition_point(|line| line.top <= caret_y)
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1))
}

fn normalize_byte_offset(text: &str, offset: usize, affinity: TextAffinity) -> usize {
    let offset = offset.min(text.len());
    if text.is_char_boundary(offset) {
        return offset;
    }
    match affinity {
        TextAffinity::Upstream => (0..offset)
            .rev()
            .find(|candidate| text.is_char_boundary(*candidate))
            .unwrap_or(0),
        TextAffinity::Downstream => (offset + 1..=text.len())
            .find(|candidate| text.is_char_boundary(*candidate))
            .unwrap_or(text.len()),
    }
}

fn estimate_bytes(
    lines: &[TextLineSnapshot],
    caret_stops: &[TextCaretStop],
    visual_lines: &[VisualLineGeometry],
) -> usize {
    std::mem::size_of::<TextGeometrySnapshot>()
        .saturating_add(
            lines
                .len()
                .saturating_mul(std::mem::size_of::<TextLineSnapshot>()),
        )
        .saturating_add(
            caret_stops
                .len()
                .saturating_mul(std::mem::size_of::<TextCaretStop>()),
        )
        .saturating_add(
            visual_lines
                .len()
                .saturating_mul(std::mem::size_of::<VisualLineGeometry>()),
        )
        .saturating_add(
            visual_lines
                .iter()
                .map(|line| {
                    line.clusters
                        .len()
                        .saturating_mul(std::mem::size_of::<VisualClusterGeometry>())
                })
                .sum::<usize>(),
        )
}
