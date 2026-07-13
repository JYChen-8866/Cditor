use std::ops::Range;

use cditor_core::edit::{InternalTextOffset, TextOffsetMap};
use cditor_core::ids::BlockId;
use cditor_runtime::TableCellPosition;
use gpui::{Bounds, Pixels, Point, TextAlign, WrappedLine as GpuiWrappedLine, point, px, size};

pub(crate) struct RichTextPlatformLayout {
    pub block_id: BlockId,
    pub content_version: u64,
    pub text: String,
    pub lines: Vec<GpuiWrappedLine>,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub text_align: TextAlign,
    pub measured_height: f64,
    pub table_cell_position: Option<TableCellPosition>,
}

pub(crate) fn platform_range_bounds(
    cache: &RichTextPlatformLayout,
    range: Range<usize>,
) -> Option<Bounds<Pixels>> {
    if cache.text.is_empty() {
        let alignment_offset =
            platform_text_alignment_offset(cache.text_align, cache.bounds.size.width, px(0.0));
        return Some(Bounds::new(
            point(cache.bounds.left() + alignment_offset, cache.bounds.top()),
            size(px(1.0), cache.line_height),
        ));
    }
    let segments = platform_range_segment_bounds(
        &cache.lines,
        cache.bounds,
        cache.line_height,
        &cache.text,
        range.clone(),
        cache.text_align,
    );
    if segments.is_empty() {
        return platform_cursor_bounds_for_offset(
            &cache.lines,
            cache.bounds,
            cache.line_height,
            &cache.text,
            range.start,
            px(1.0),
            cache.text_align,
        );
    }
    let mut union = segments[0];
    for segment in segments.iter().skip(1) {
        union = Bounds::from_corners(
            point(
                union.left().min(segment.left()),
                union.top().min(segment.top()),
            ),
            point(
                union.right().max(segment.right()),
                union.bottom().max(segment.bottom()),
            ),
        );
    }
    Some(union)
}

pub(crate) fn platform_index_for_point(
    cache: &RichTextPlatformLayout,
    position: Point<Pixels>,
) -> usize {
    if cache.text.is_empty() || cache.lines.is_empty() {
        return 0;
    }
    if position.y < cache.bounds.top() {
        return 0;
    }
    if position.y > cache.bounds.bottom() {
        return cache.text.len();
    }
    let ranges = hard_line_ranges(&cache.text);
    let relative_y = position.y - cache.bounds.top();
    let Some((line_idx, y_in_line)) =
        platform_wrapped_line_for_y(&cache.lines, cache.line_height, relative_y)
    else {
        return 0;
    };
    let Some(layout) = cache.lines.get(line_idx) else {
        return 0;
    };
    let mut local = platform_local_point_for_bounds(cache.bounds, position);
    let row_idx = (f32::from(y_in_line) / f32::from(cache.line_height).max(1.0))
        .floor()
        .max(0.0) as usize;
    local.x -= platform_wrapped_row_alignment_offset(
        layout,
        row_idx,
        cache.bounds.size.width,
        cache.text_align,
    );
    let offset_in_line =
        match layout.closest_index_for_position(point(local.x, y_in_line), cache.line_height) {
            Ok(index) | Err(index) => index,
        };
    ranges
        .get(line_idx)
        .map(|range| clamp_to_char_boundary(&cache.text, range.start + offset_in_line))
        .unwrap_or(0)
}

pub(crate) fn platform_cursor_bounds_for_offset(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    offset: usize,
    cursor_width: Pixels,
    text_align: TextAlign,
) -> Option<Bounds<Pixels>> {
    // Some platform shapers return one zero-length line for an empty string,
    // others return no lines. Empty text has the same stable caret geometry in
    // both cases and must not depend on the shaper's representation.
    if text.is_empty() {
        let alignment_offset =
            platform_text_alignment_offset(text_align, bounds.size.width, px(0.0));
        return Some(Bounds::new(
            point(bounds.left() + alignment_offset, bounds.top()),
            size(cursor_width, line_height),
        ));
    }
    let ranges = hard_line_ranges(text);
    let (line_idx, offset_in_line) = line_index_for_offset(text, &ranges, offset);
    let layout = lines.get(line_idx)?;
    let hard_range = ranges.get(line_idx)?;
    let cursor_pos = platform_position_for_offset(
        text,
        hard_range,
        layout,
        offset_in_line,
        line_height,
        true,
        bounds.size.width,
        text_align,
    )?;
    let y_offset = bounds.top() + platform_wrapped_line_top(lines, line_height, line_idx);
    Some(Bounds::new(
        point(bounds.left() + cursor_pos.x, y_offset + cursor_pos.y),
        size(cursor_width, line_height),
    ))
}

pub(crate) fn platform_range_segment_bounds(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
    text_align: TextAlign,
) -> Vec<Bounds<Pixels>> {
    if range.start >= range.end || lines.is_empty() {
        return Vec::new();
    }
    let ranges = hard_line_ranges(text);
    let range = normalized_text_range(text, range);
    let (start_line, start_offset) = line_index_for_offset(text, &ranges, range.start);
    let (end_line, end_offset) = line_index_for_offset(text, &ranges, range.end);
    let mut segments = Vec::new();
    for line_idx in start_line..=end_line {
        let Some(hard_range) = ranges.get(line_idx) else {
            continue;
        };
        let line_start = if line_idx == start_line {
            start_offset
        } else {
            0
        };
        let line_end = if line_idx == end_line {
            end_offset
        } else {
            hard_range.len()
        };
        segments.extend(platform_range_segment_bounds_for_hard_line(
            text,
            hard_range,
            lines,
            bounds,
            line_height,
            line_idx,
            line_start,
            line_end,
            text_align,
        ));
    }
    segments
}

fn platform_local_point_for_bounds(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
) -> Point<Pixels> {
    point(position.x - bounds.left(), position.y - bounds.top())
}

fn platform_position_for_offset(
    text: &str,
    hard_range: &Range<usize>,
    line: &GpuiWrappedLine,
    offset: usize,
    line_height: Pixels,
    prefer_next_wrap_start: bool,
    align_width: Pixels,
    text_align: TextAlign,
) -> Option<Point<Pixels>> {
    let offset = clamp_local_offset_to_char_boundary(text, hard_range.start, offset);
    if hard_range.is_empty() && offset == 0 {
        return Some(point(
            platform_text_alignment_offset(text_align, align_width, px(0.0)),
            px(0.0),
        ));
    }
    let offsets = platform_wrapped_row_offsets(line);
    let row_count = offsets.len().saturating_sub(1);
    for row_idx in 0..row_count {
        let row_start =
            clamp_local_offset_to_char_boundary(text, hard_range.start, offsets[row_idx]);
        let row_end =
            clamp_local_offset_to_char_boundary(text, hard_range.start, offsets[row_idx + 1]);
        let is_start_of_wrapped_row = prefer_next_wrap_start && row_idx > 0 && offset == row_start;
        if is_start_of_wrapped_row
            || (offset >= row_start
                && (offset < row_end || (row_idx + 1 == row_count && offset == row_end)))
        {
            let row_start_x = line.unwrapped_layout.x_for_index(row_start);
            let x = line.unwrapped_layout.x_for_index(offset) - row_start_x
                + platform_wrapped_row_alignment_offset(line, row_idx, align_width, text_align);
            return Some(point(x, line_height * row_idx as f32));
        }
    }
    None
}

fn platform_range_segment_bounds_for_hard_line(
    text: &str,
    hard_range: &Range<usize>,
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    line_idx: usize,
    start_offset: usize,
    end_offset: usize,
    text_align: TextAlign,
) -> Vec<Bounds<Pixels>> {
    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };
    let line_top = bounds.top() + platform_wrapped_line_top(lines, line_height, line_idx);
    let offsets = platform_wrapped_row_offsets(line);
    let mut segments = Vec::new();
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start =
            clamp_local_offset_to_char_boundary(text, hard_range.start, offsets[row_idx]);
        let row_end =
            clamp_local_offset_to_char_boundary(text, hard_range.start, offsets[row_idx + 1]);
        let seg_start = clamp_local_offset_to_char_boundary(
            text,
            hard_range.start,
            start_offset.max(row_start).min(row_end),
        );
        let seg_end = clamp_local_offset_to_char_boundary(
            text,
            hard_range.start,
            end_offset.min(row_end).max(row_start),
        );
        if seg_start >= seg_end {
            continue;
        }
        let row_start_x = line.unwrapped_layout.x_for_index(row_start);
        let start_x = line.unwrapped_layout.x_for_index(seg_start) - row_start_x;
        let end_x = line.unwrapped_layout.x_for_index(seg_end) - row_start_x;
        let alignment_offset =
            platform_wrapped_row_alignment_offset(line, row_idx, bounds.size.width, text_align);
        let y = line_top + line_height * row_idx as f32;
        segments.push(Bounds::from_corners(
            point(bounds.left() + alignment_offset + start_x, y),
            point(bounds.left() + alignment_offset + end_x, y + line_height),
        ));
    }
    segments
}

fn platform_wrapped_row_alignment_offset(
    line: &GpuiWrappedLine,
    row_idx: usize,
    available_width: Pixels,
    text_align: TextAlign,
) -> Pixels {
    let offsets = platform_wrapped_row_offsets(line);
    let Some(row_start) = offsets.get(row_idx).copied() else {
        return px(0.0);
    };
    let Some(row_end) = offsets.get(row_idx + 1).copied() else {
        return px(0.0);
    };
    let row_width = (line.unwrapped_layout.x_for_index(row_end)
        - line.unwrapped_layout.x_for_index(row_start))
    .max(px(0.0));
    platform_text_alignment_offset(text_align, available_width, row_width)
}

pub(crate) fn platform_text_alignment_offset(
    text_align: TextAlign,
    available_width: Pixels,
    row_width: Pixels,
) -> Pixels {
    let remaining = (available_width - row_width).max(px(0.0));
    match text_align {
        TextAlign::Left => px(0.0),
        TextAlign::Center => remaining / 2.0,
        TextAlign::Right => remaining,
    }
}

fn hard_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            ranges.push(start..index);
            start = index + ch.len_utf8();
        }
    }
    ranges.push(start..text.len());
    ranges
}

fn line_index_for_offset(text: &str, ranges: &[Range<usize>], offset: usize) -> (usize, usize) {
    let clamped = clamp_to_char_boundary(
        text,
        offset.min(ranges.last().map(|range| range.end).unwrap_or(0)),
    );
    for (index, range) in ranges.iter().enumerate() {
        if clamped <= range.end {
            return (index, clamped.saturating_sub(range.start));
        }
    }
    let last = ranges.len().saturating_sub(1);
    (
        last,
        ranges
            .get(last)
            .map(|range| range.len())
            .unwrap_or_default(),
    )
}

pub(crate) fn normalized_text_range(text: &str, range: Range<usize>) -> Range<usize> {
    let offsets = TextOffsetMap::build(text);
    let range = offsets
        .normalize_internal_range(InternalTextOffset(range.start)..InternalTextOffset(range.end));
    range.start.0..range.end.0
}

fn clamp_local_offset_to_char_boundary(text: &str, hard_start: usize, offset: usize) -> usize {
    clamp_to_char_boundary(text, hard_start.saturating_add(offset)).saturating_sub(hard_start)
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn platform_wrapped_line_top(
    lines: &[GpuiWrappedLine],
    line_height: Pixels,
    line_idx: usize,
) -> Pixels {
    lines.iter().take(line_idx).fold(px(0.0), |height, line| {
        height + line.size(line_height).height
    })
}

fn platform_wrapped_line_for_y(
    lines: &[GpuiWrappedLine],
    line_height: Pixels,
    relative_y: Pixels,
) -> Option<(usize, Pixels)> {
    let mut top = px(0.0);
    for (line_idx, line) in lines.iter().enumerate() {
        let height = line.size(line_height).height;
        if relative_y < top + height || line_idx + 1 == lines.len() {
            return Some((line_idx, (relative_y - top).max(px(0.0))));
        }
        top += height;
    }
    None
}

fn platform_wrapped_row_offsets(line: &GpuiWrappedLine) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(line.wrap_boundaries().len() + 2);
    offsets.push(0);
    for wrap_idx in 0..line.wrap_boundaries().len() {
        if let Some(offset) = platform_wrap_boundary_offset(line, wrap_idx) {
            offsets.push(offset.min(line.len()));
        }
    }
    offsets.push(line.len());
    offsets.dedup();
    offsets
}

fn platform_wrap_boundary_offset(line: &GpuiWrappedLine, wrap_idx: usize) -> Option<usize> {
    let boundary = line.wrap_boundaries().get(wrap_idx)?;
    let run = line.unwrapped_layout.runs.get(boundary.run_ix)?;
    let glyph = run.glyphs.get(boundary.glyph_ix)?;
    Some(glyph.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_text_offsets_clamp_inside_cjk_characters() {
        let text = "埃塞";
        let ranges = hard_line_ranges(text);

        assert_eq!(line_index_for_offset(text, &ranges, 1), (0, 0));
        assert_eq!(line_index_for_offset(text, &ranges, 2), (0, 0));
        assert_eq!(line_index_for_offset(text, &ranges, 3), (0, 3));
        assert_eq!(normalized_text_range(text, 1..4), 0..text.len());
        assert_eq!(normalized_text_range(text, 2..2), 0..0);
        assert_eq!(clamp_local_offset_to_char_boundary(text, 0, 1), 0);
    }

    #[test]
    fn platform_cursor_draws_on_trailing_empty_hard_line() {
        let lines = vec![GpuiWrappedLine::default(), GpuiWrappedLine::default()];
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(300.0), px(48.0)));

        let cursor = platform_cursor_bounds_for_offset(
            &lines,
            bounds,
            px(24.0),
            "\n",
            1,
            px(1.5),
            TextAlign::Left,
        )
        .expect("trailing empty line must have caret geometry");

        assert_eq!(cursor.left(), px(10.0));
        assert_eq!(cursor.top(), px(44.0));
        assert_eq!(cursor.size.height, px(24.0));
    }

    #[test]
    fn platform_point_hit_testing_uses_cache_bounds_as_local_origin() {
        let bounds = Bounds {
            origin: point(px(120.0), px(240.0)),
            size: size(px(300.0), px(80.0)),
        };

        assert_eq!(
            platform_local_point_for_bounds(bounds, point(px(150.0), px(265.0))),
            point(px(30.0), px(25.0))
        );
        assert_eq!(
            platform_local_point_for_bounds(bounds, point(px(90.0), px(210.0))),
            point(px(-30.0), px(-30.0))
        );
    }

    #[test]
    fn empty_text_range_uses_the_stable_line_box_as_its_anchor() {
        let cache = RichTextPlatformLayout {
            block_id: 1,
            content_version: 1,
            text: String::new(),
            lines: Vec::new(),
            bounds: Bounds::new(point(px(120.0), px(240.0)), size(px(300.0), px(24.0))),
            line_height: px(24.0),
            text_align: TextAlign::Left,
            measured_height: 24.0,
            table_cell_position: None,
        };

        assert_eq!(
            platform_range_bounds(&cache, 0..0),
            Some(Bounds::new(
                point(px(120.0), px(240.0)),
                size(px(1.0), px(24.0))
            ))
        );
    }

    #[test]
    fn empty_text_block_still_has_a_paintable_caret() {
        let bounds = Bounds::new(point(px(120.0), px(240.0)), size(px(300.0), px(24.0)));
        assert_eq!(
            platform_cursor_bounds_for_offset(
                &[],
                bounds,
                px(24.0),
                "",
                0,
                px(1.5),
                TextAlign::Left,
            ),
            Some(Bounds::new(
                point(px(120.0), px(240.0)),
                size(px(1.5), px(24.0))
            ))
        );
    }

    #[test]
    fn empty_centered_table_cell_uses_the_same_alignment_for_caret_and_ime_bounds() {
        let bounds = Bounds::new(point(px(120.0), px(240.0)), size(px(300.0), px(24.0)));
        let caret = platform_cursor_bounds_for_offset(
            &[],
            bounds,
            px(24.0),
            "",
            0,
            px(1.5),
            TextAlign::Center,
        )
        .expect("empty centered cell must have caret geometry");
        let cache = RichTextPlatformLayout {
            block_id: 1,
            content_version: 1,
            text: String::new(),
            lines: Vec::new(),
            bounds,
            line_height: px(24.0),
            text_align: TextAlign::Center,
            measured_height: 24.0,
            table_cell_position: Some(TableCellPosition { row: 0, col: 0 }),
        };
        let ime_bounds = platform_range_bounds(&cache, 0..0).unwrap();

        assert_eq!(caret.left(), px(270.0));
        assert_eq!(ime_bounds.left(), caret.left());
    }

    #[test]
    fn table_text_alignment_offsets_all_geometry_from_the_same_width() {
        assert_eq!(
            platform_text_alignment_offset(TextAlign::Left, px(120.0), px(40.0)),
            px(0.0)
        );
        assert_eq!(
            platform_text_alignment_offset(TextAlign::Center, px(120.0), px(40.0)),
            px(40.0)
        );
        assert_eq!(
            platform_text_alignment_offset(TextAlign::Right, px(120.0), px(40.0)),
            px(80.0)
        );
        assert_eq!(
            platform_text_alignment_offset(TextAlign::Center, px(40.0), px(80.0)),
            px(0.0)
        );
    }
}
