use super::*;

pub(super) fn platform_text_runs(
    spans: &[InlineSpan],
    kind: &RichBlockKind,
    marked_range: Option<&Range<usize>>,
    theme: GuiTheme,
    window: &Window,
) -> Vec<TextRun> {
    let text = plain_text_from_spans(spans);
    let base_font = window.text_style().font();
    let base_color = Hsla::from(rgb(text_color_for_kind(kind, theme)));
    if spans.is_empty() {
        return vec![TextRun {
            len: text.len(),
            font: base_font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let span_ranges = span_ranges(spans);
    let mut boundaries = vec![0, text.len()];
    for (range, _) in &span_ranges {
        boundaries.push(range.start);
        boundaries.push(range.end);
    }
    if let Some(marked_range) = marked_range {
        boundaries.push(marked_range.start.min(text.len()));
        boundaries.push(marked_range.end.min(text.len()));
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    let mut span_idx = 0usize;
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if start >= end {
            continue;
        }
        while span_idx < span_ranges.len() && span_ranges[span_idx].0.end <= start {
            span_idx += 1;
        }
        let marks = span_ranges
            .get(span_idx)
            .filter(|(range, _)| range.start <= start && start < range.end)
            .map(|(_, span)| span.marks.as_slice())
            .unwrap_or(&[]);
        let mut font = base_font.clone();
        if marks.iter().any(|mark| matches!(mark, InlineMark::Bold))
            && font.weight < FontWeight::BOLD
        {
            font.weight = FontWeight::BOLD;
        }
        if marks.iter().any(|mark| matches!(mark, InlineMark::Italic)) {
            font.style = FontStyle::Italic;
        }
        let is_link = marks
            .iter()
            .any(|mark| matches!(mark, InlineMark::Link { .. }));
        let color = if is_link {
            Hsla::from(rgb(theme.focused))
        } else {
            base_color
        };
        let is_marked = marked_range
            .map(|range| start < range.end && range.start < end)
            .unwrap_or(false);
        let underline = (is_marked
            || marks
                .iter()
                .any(|mark| matches!(mark, InlineMark::Underline | InlineMark::Link { .. })))
        .then_some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.0),
            wavy: false,
        });
        runs.push(TextRun {
            len: end - start,
            font,
            color,
            background_color: marks
                .iter()
                .any(|mark| matches!(mark, InlineMark::Code))
                .then_some(Hsla::from(rgb(theme.code_background))),
            underline,
            strikethrough: None,
        });
    }
    runs
}

pub(super) fn plain_text_from_spans(spans: &[InlineSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

fn span_ranges(spans: &[InlineSpan]) -> Vec<(Range<usize>, &InlineSpan)> {
    let mut offset = 0usize;
    spans
        .iter()
        .map(|span| {
            let start = offset;
            offset += span.text.len();
            (start..offset, span)
        })
        .collect()
}

pub(super) fn text_size_for_kind(kind: &RichBlockKind) -> Pixels {
    match kind {
        RichBlockKind::Heading { level: 1 } => px(28.0),
        RichBlockKind::Heading { level: 2 } => px(24.0),
        RichBlockKind::Heading { .. } => px(20.0),
        RichBlockKind::Code { .. } => px(14.0),
        _ => px(16.0),
    }
}

pub(super) fn line_height_for_kind(kind: &RichBlockKind, text_size: Pixels) -> Pixels {
    match kind {
        RichBlockKind::Code { .. } => px(24.0),
        _ => text_size * 1.25,
    }
}

pub(super) fn text_color_for_kind(kind: &RichBlockKind, theme: GuiTheme) -> u32 {
    match kind {
        RichBlockKind::Code { .. } => theme.code_text,
        RichBlockKind::Quote => theme.quote_text,
        _ => theme.text,
    }
}

pub(crate) fn platform_range_bounds(
    cache: &RichTextPlatformLayout,
    range: Range<usize>,
) -> Option<Bounds<Pixels>> {
    let segments = platform_range_segment_bounds(
        &cache.lines,
        cache.bounds,
        cache.line_height,
        &cache.text,
        range.clone(),
    );
    if segments.is_empty() {
        return platform_cursor_bounds_for_offset(
            &cache.lines,
            cache.bounds,
            cache.line_height,
            &cache.text,
            range.start,
            px(1.0),
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
    let offset_in_line = match layout.closest_index_for_position(
        point(position.x - cache.bounds.left(), y_in_line),
        cache.line_height,
    ) {
        Ok(index) | Err(index) => index,
    };
    ranges
        .get(line_idx)
        .map(|range| range.start + offset_in_line)
        .unwrap_or(0)
}

pub(super) fn platform_cursor_bounds_for_offset(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    offset: usize,
    cursor_width: Pixels,
) -> Option<Bounds<Pixels>> {
    let ranges = hard_line_ranges(text);
    let (line_idx, offset_in_line) = line_index_for_offset(&ranges, offset);
    let layout = lines.get(line_idx)?;
    let cursor_pos = platform_position_for_offset(layout, offset_in_line, line_height, true)?;
    let y_offset = bounds.top() + platform_wrapped_line_top(lines, line_height, line_idx);
    Some(Bounds::new(
        point(bounds.left() + cursor_pos.x, y_offset + cursor_pos.y),
        size(cursor_width, line_height),
    ))
}

fn platform_position_for_offset(
    line: &GpuiWrappedLine,
    offset: usize,
    line_height: Pixels,
    prefer_next_wrap_start: bool,
) -> Option<Point<Pixels>> {
    let offsets = platform_wrapped_row_offsets(line);
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let is_start_of_wrapped_row = prefer_next_wrap_start && row_idx > 0 && offset == row_start;
        if is_start_of_wrapped_row || (offset >= row_start && offset < row_end) {
            let row_start_x = line.unwrapped_layout.x_for_index(row_start);
            let x = line.unwrapped_layout.x_for_index(offset) - row_start_x;
            return Some(point(x, line_height * row_idx as f32));
        }
    }
    line.position_for_index(offset, line_height)
}

pub(super) fn platform_range_segment_bounds(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
) -> Vec<Bounds<Pixels>> {
    if range.start >= range.end || lines.is_empty() {
        return Vec::new();
    }
    let ranges = hard_line_ranges(text);
    let (start_line, start_offset) = line_index_for_offset(&ranges, range.start);
    let (end_line, end_offset) = line_index_for_offset(&ranges, range.end);
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
            lines,
            bounds,
            line_height,
            line_idx,
            line_start,
            line_end,
        ));
    }
    segments
}

fn platform_range_segment_bounds_for_hard_line(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    line_idx: usize,
    start_offset: usize,
    end_offset: usize,
) -> Vec<Bounds<Pixels>> {
    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };
    let line_top = bounds.top() + platform_wrapped_line_top(lines, line_height, line_idx);
    let offsets = platform_wrapped_row_offsets(line);
    let mut segments = Vec::new();
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let seg_start = start_offset.max(row_start).min(row_end);
        let seg_end = end_offset.min(row_end).max(row_start);
        if seg_start >= seg_end {
            continue;
        }
        let row_start_x = line.unwrapped_layout.x_for_index(row_start);
        let start_x = line.unwrapped_layout.x_for_index(seg_start) - row_start_x;
        let end_x = line.unwrapped_layout.x_for_index(seg_end) - row_start_x;
        let y = line_top + line_height * row_idx as f32;
        segments.push(Bounds::from_corners(
            point(bounds.left() + start_x, y),
            point(bounds.left() + end_x, y + line_height),
        ));
    }
    segments
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

fn line_index_for_offset(ranges: &[Range<usize>], offset: usize) -> (usize, usize) {
    let clamped = offset.min(ranges.last().map(|range| range.end).unwrap_or(0));
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
