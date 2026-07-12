use std::{collections::HashMap, ops::Range};

use cditor_core::ids::BlockId;
use cditor_core::layout::text_line_height_for_kind;
use cditor_core::rich_text::InlineMark;

use super::RichTextLayoutInput;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextLayoutKey {
    pub block_id: BlockId,
    pub content_version: u64,
    pub width_bucket: u16,
    pub theme_version: u64,
    pub font_version: u64,
    pub scale_factor_bits: u64,
}

impl TextLayoutKey {
    pub fn new(
        block_id: BlockId,
        content_version: u64,
        width_bucket: u16,
        theme_version: u64,
        font_version: u64,
        scale_factor: f64,
    ) -> Self {
        Self {
            block_id,
            content_version,
            width_bucket,
            theme_version,
            font_version,
            scale_factor_bits: scale_factor.to_bits(),
        }
    }

    pub fn from_input(input: &RichTextLayoutInput, width_bucket: u16, scale_factor: f64) -> Self {
        Self::new(
            input.block_id,
            input.content_version,
            width_bucket,
            input.theme_version,
            input.font_version,
            scale_factor,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RichTextLayoutMetrics {
    pub width_px: f64,
    pub estimated_height_px: f64,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WrappedLine {
    pub logical_range: Range<usize>,
    pub y: f64,
    pub height: f64,
    pub runs: Vec<VisualRun>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualRun {
    pub logical_range: Range<usize>,
    pub x: f64,
    pub width: f64,
    pub mark_style: InlineStyle,
}

impl InlineStyle {
    pub fn from_marks(marks: &[InlineMark]) -> Self {
        let mut style = Self::default();
        for mark in marks {
            match mark {
                InlineMark::Bold => style.bold = true,
                InlineMark::Italic => style.italic = true,
                InlineMark::Underline => style.underline = true,
                InlineMark::Strike => style.strike = true,
                InlineMark::Code => style.code = true,
                InlineMark::Link { .. } => style.link = true,
                InlineMark::Color(_) | InlineMark::Background(_) => {}
            }
        }
        style
    }
}

impl WrappedLine {
    pub fn end_y(&self) -> f64 {
        self.y + self.height
    }
}

impl VisualRun {
    pub fn contains_x(&self, x: f64) -> bool {
        self.x <= x && x < self.x + self.width
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextHitPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextCaretRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RichTextLayout {
    pub lines: Vec<WrappedLine>,
    pub metrics: RichTextLayoutMetrics,
    pub height: f64,
}

impl RichTextLayout {
    pub fn offset_for_point(&self, text: &str, point: TextHitPoint) -> usize {
        let Some(line) = self
            .lines
            .iter()
            .find(|line| line.y <= point.y && point.y < line.end_y())
            .or_else(|| self.lines.last())
        else {
            return text.len();
        };
        if line.runs.is_empty() {
            return line.logical_range.start.min(text.len());
        }
        for run in &line.runs {
            if point.x < run.x + run.width {
                return offset_in_run_for_x(text, run, point.x).min(text.len());
            }
        }
        line.logical_range.end.min(text.len())
    }

    pub fn caret_rect_for_offset(&self, text: &str, offset: usize) -> TextCaretRect {
        let offset = previous_char_boundary(text, offset.min(text.len()));
        for line in &self.lines {
            if line.logical_range.start <= offset && offset <= line.logical_range.end {
                let x = line
                    .runs
                    .iter()
                    .find(|run| {
                        run.logical_range.start <= offset && offset <= run.logical_range.end
                    })
                    .map(|run| x_for_offset_in_run(text, run, offset))
                    .unwrap_or(0.0);
                return TextCaretRect {
                    x,
                    y: line.y,
                    width: 1.0,
                    height: line.height,
                };
            }
        }
        let Some(line) = self.lines.last() else {
            return TextCaretRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 24.0,
            };
        };
        TextCaretRect {
            x: line.runs.last().map(|run| run.x + run.width).unwrap_or(0.0),
            y: line.y,
            width: 1.0,
            height: line.height,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedRichTextLayout {
    pub key: TextLayoutKey,
    pub layout: RichTextLayout,
    pub cache_hit: bool,
}

#[derive(Debug, Clone)]
pub struct RichTextLayoutCache {
    entries: HashMap<TextLayoutKey, RichTextLayout>,
    order: Vec<TextLayoutKey>,
    capacity: usize,
}

impl RichTextLayoutCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn layout(
        &mut self,
        input: &RichTextLayoutInput,
        width_bucket: u16,
        scale_factor: f64,
    ) -> CachedRichTextLayout {
        let key = TextLayoutKey::from_input(input, width_bucket, scale_factor);
        if let Some(layout) = self.entries.get(&key) {
            return CachedRichTextLayout {
                key,
                layout: layout.clone(),
                cache_hit: true,
            };
        }

        let layout = wrap_rich_text(input);
        self.insert(key.clone(), layout.clone());
        CachedRichTextLayout {
            key,
            layout,
            cache_hit: false,
        }
    }

    fn insert(&mut self, key: TextLayoutKey, layout: RichTextLayout) {
        if !self.entries.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.entries.insert(key, layout);
        while self.entries.len() > self.capacity {
            if let Some(evicted) = self.order.first().cloned() {
                self.order.remove(0);
                self.entries.remove(&evicted);
            } else {
                break;
            }
        }
    }
}

const ASCII_CHAR_WIDTH: f64 = 7.5;
const WIDE_CHAR_WIDTH: f64 = 14.0;

pub fn wrap_rich_text(input: &RichTextLayoutInput) -> RichTextLayout {
    let line_height = text_line_height_for_kind(&input.kind);
    let max_chars_per_line = (input.width_px / ASCII_CHAR_WIDTH).floor().max(1.0) as usize;
    let mut builder = LineWrapBuilder::new(line_height);
    let mut global_byte_offset = 0usize;

    for span in &input.spans {
        let style = InlineStyle::from_marks(&span.marks);
        for ch in span.text.chars() {
            let char_len = ch.len_utf8();
            if ch == '\n' {
                builder.finish_line(global_byte_offset);
                global_byte_offset += char_len;
                builder.start_line(global_byte_offset);
                continue;
            }
            if builder.line_char_count >= max_chars_per_line {
                builder.finish_line(global_byte_offset);
                builder.start_line(global_byte_offset);
            }

            let char_start = global_byte_offset;
            let char_end = global_byte_offset + char_len;
            let char_width = estimated_char_width(ch);
            builder.push_char(char_start..char_end, char_width, style.clone());
            global_byte_offset = char_end;
        }
    }

    builder.finish_line(global_byte_offset);
    let line_count = builder.lines.len().max(1);
    let height = line_count as f64 * line_height;
    RichTextLayout {
        lines: builder.lines,
        metrics: RichTextLayoutMetrics {
            width_px: input.width_px,
            estimated_height_px: height,
            line_count,
        },
        height,
    }
}

fn offset_in_run_for_x(text: &str, run: &VisualRun, x: f64) -> usize {
    let target = (x - run.x).max(0.0).min(run.width);
    let slice = text.get(run.logical_range.clone()).unwrap_or_default();
    let mut offset = run.logical_range.start;
    let mut current_x = 0.0;
    for ch in slice.chars() {
        let char_width = estimated_char_width(ch);
        if target < current_x + char_width / 2.0 {
            return offset;
        }
        current_x += char_width;
        offset += ch.len_utf8();
    }
    run.logical_range.end
}

fn x_for_offset_in_run(text: &str, run: &VisualRun, offset: usize) -> f64 {
    let offset = previous_char_boundary(text, offset.min(text.len()));
    let slice = text.get(run.logical_range.clone()).unwrap_or_default();
    let mut current = run.logical_range.start;
    let mut x = run.x;
    for ch in slice.chars() {
        if current >= offset {
            break;
        }
        x += estimated_char_width(ch);
        current += ch.len_utf8();
    }
    x
}

fn estimated_char_width(ch: char) -> f64 {
    if ch.is_ascii() {
        ASCII_CHAR_WIDTH
    } else {
        WIDE_CHAR_WIDTH
    }
}

fn previous_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

struct LineWrapBuilder {
    lines: Vec<WrappedLine>,
    current_runs: Vec<VisualRun>,
    line_start: usize,
    line_y: f64,
    line_height: f64,
    line_width: f64,
    line_char_count: usize,
}

impl LineWrapBuilder {
    fn new(line_height: f64) -> Self {
        Self {
            lines: Vec::new(),
            current_runs: Vec::new(),
            line_start: 0,
            line_y: 0.0,
            line_height,
            line_width: 0.0,
            line_char_count: 0,
        }
    }

    fn start_line(&mut self, line_start: usize) {
        self.line_start = line_start;
        self.line_width = 0.0;
        self.line_char_count = 0;
        self.current_runs.clear();
    }

    fn push_char(&mut self, range: Range<usize>, width: f64, style: InlineStyle) {
        if let Some(last) = self.current_runs.last_mut()
            && last.mark_style == style
            && last.logical_range.end == range.start
        {
            last.logical_range.end = range.end;
            last.width += width;
        } else {
            self.current_runs.push(VisualRun {
                logical_range: range,
                x: self.line_width,
                width,
                mark_style: style,
            });
        }
        self.line_width += width;
        self.line_char_count += 1;
    }

    fn finish_line(&mut self, line_end: usize) {
        self.lines.push(WrappedLine {
            logical_range: self.line_start..line_end,
            y: self.line_y,
            height: self.line_height,
            runs: std::mem::take(&mut self.current_runs),
        });
        self.line_y += self.line_height;
    }
}

impl RichTextLayoutMetrics {
    pub fn estimate_from_input(input: &RichTextLayoutInput) -> Self {
        let text_len = input
            .spans
            .iter()
            .map(|span| span.text.len())
            .sum::<usize>();
        let line_count = text_len.saturating_div(80).saturating_add(1).max(1);
        Self {
            width_px: input.width_px,
            estimated_height_px: line_count as f64 * 24.0,
            line_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind};

    fn layout_input(text: &str, width_px: f64) -> RichTextLayoutInput {
        RichTextLayoutInput {
            block_id: 1,
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            spans: vec![InlineSpan::plain(text)],
            width_px,
            theme_version: 1,
            font_version: 1,
        }
    }

    #[test]
    fn layout_hit_test_and_caret_rect_use_utf8_offsets() {
        let input = layout_input("你好ab", 320.0);
        let text = input.spans[0].text.clone();
        let layout = wrap_rich_text(&input);

        let offset = layout.offset_for_point(&text, TextHitPoint { x: 20.0, y: 4.0 });
        assert!(text.is_char_boundary(offset));

        let caret = layout.caret_rect_for_offset(&text, "你".len());
        assert_eq!(caret.y, 0.0);
        assert!(caret.x > 0.0);
        assert_eq!(caret.height, 24.0);

        let mixed_caret = layout.caret_rect_for_offset(&text, "你好a".len());
        assert_eq!(mixed_caret.x, WIDE_CHAR_WIDTH * 2.0 + ASCII_CHAR_WIDTH);
    }

    #[test]
    fn heading_layout_uses_larger_line_height_to_avoid_overlap() {
        let mut input = layout_input("Heading", 320.0);
        input.kind = RichBlockKind::Heading { level: 1 };

        let layout = wrap_rich_text(&input);

        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].height, 39.0);
        assert_eq!(layout.height, 39.0);
    }

    #[test]
    fn rich_text_wraps_long_text() {
        let input = layout_input("abcdefghijklmnopqrstuvwxyz", 80.0);

        let layout = wrap_rich_text(&input);

        assert!(layout.lines.len() >= 3);
        assert_eq!(layout.metrics.line_count, layout.lines.len());
        assert_eq!(layout.height, layout.metrics.estimated_height_px);
        assert_eq!(layout.lines[0].logical_range, 0..10);
        assert_eq!(layout.lines[1].logical_range, 10..20);
    }

    #[test]
    fn rich_text_wrap_does_not_split_utf8() {
        let input = layout_input("你好🙂世界🙂", 16.0);

        let layout = wrap_rich_text(&input);

        assert!(layout.lines.len() > 1);
        for line in &layout.lines {
            assert!(
                input.spans[0]
                    .text
                    .is_char_boundary(line.logical_range.start)
            );
            assert!(input.spans[0].text.is_char_boundary(line.logical_range.end));
            for run in &line.runs {
                assert!(
                    input.spans[0]
                        .text
                        .is_char_boundary(run.logical_range.start)
                );
                assert!(input.spans[0].text.is_char_boundary(run.logical_range.end));
            }
        }
    }

    #[test]
    fn text_layout_cache_hits_same_key() {
        let input = layout_input("hello cache", 160.0);
        let mut cache = RichTextLayoutCache::new(8);

        let first = cache.layout(&input, 160, 1.0);
        let second = cache.layout(&input, 160, 1.0);

        assert!(!first.cache_hit);
        assert!(second.cache_hit);
        assert_eq!(first.key, second.key);
        assert_eq!(first.layout, second.layout);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn text_layout_cache_misses_after_content_change() {
        let mut input = layout_input("hello cache", 160.0);
        let mut cache = RichTextLayoutCache::new(8);

        let first = cache.layout(&input, 160, 1.0);
        input.content_version += 1;
        let second = cache.layout(&input, 160, 1.0);

        assert!(!first.cache_hit);
        assert!(!second.cache_hit);
        assert_ne!(first.key, second.key);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn wrapped_line_model_represents_lines_and_runs_for_hit_test() {
        let line = WrappedLine {
            logical_range: 0..11,
            y: 24.0,
            height: 24.0,
            runs: vec![
                VisualRun {
                    logical_range: 0..5,
                    x: 0.0,
                    width: 40.0,
                    mark_style: InlineStyle::default(),
                },
                VisualRun {
                    logical_range: 6..11,
                    x: 48.0,
                    width: 44.0,
                    mark_style: InlineStyle {
                        bold: true,
                        ..InlineStyle::default()
                    },
                },
            ],
        };

        assert_eq!(line.end_y(), 48.0);
        assert_eq!(line.runs.len(), 2);
        assert!(line.runs[0].contains_x(12.0));
        assert!(!line.runs[0].contains_x(48.0));
        assert!(line.runs[1].mark_style.bold);
    }

    #[test]
    fn inline_style_tracks_marks_used_by_visual_runs() {
        let style = InlineStyle::from_marks(&[
            InlineMark::Bold,
            InlineMark::Code,
            InlineMark::Link {
                href: "https://example.com".to_owned(),
            },
        ]);

        assert!(style.bold);
        assert!(style.code);
        assert!(style.link);
        assert!(!style.italic);
    }

    #[test]
    fn text_layout_key_changes_when_width_bucket_changes() {
        let narrow = TextLayoutKey::new(1, 2, 320, 3, 4, 2.0);
        let wide = TextLayoutKey::new(1, 2, 640, 3, 4, 2.0);

        assert_ne!(narrow, wide);

        let mut keys = HashSet::new();
        keys.insert(narrow.clone());
        keys.insert(wide.clone());

        assert!(keys.contains(&narrow));
        assert!(keys.contains(&wide));
        assert_eq!(keys.len(), 2);
    }
}
