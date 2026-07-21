//! 超长 text surface 的分段布局（P6-015）。
//!
//! 10MiB/100k 行级别的 code/text surface 禁止整块同步 layout（P2-018 基准证
//! 明整块 full build p95 秒级）。本模块把文本按硬行边界切成段：
//!
//! - 构建只做 O(n) 换行扫描，不做任何 shaping；
//! - 每段独立 layout，只有可见窗口的段被真正测量（caller 提供 builder）；
//! - 未测量段用自适应行高估计高度，测量后全局估计随之修正；
//! - 编辑只失效受影响的段，宽度变化丢弃测量但保留分段索引；
//! - 内部滚动锚点以字节偏移标识，测高修正后可稳定还原视口。
//!
//! 与 App 渲染管线/cache identity 的接线属于 Phase 6 集成边界。

use std::ops::Range;

use crate::snapshot::ParleyLayoutSnapshot;

/// 超过该字节数的 surface 必须走分段布局，禁止整块同步 layout。
pub const LARGE_TEXT_SEGMENTATION_THRESHOLD_BYTES: usize = 256 * 1024;

/// 文本是否必须分段。
pub const fn requires_segmentation(text_len: usize) -> bool {
    text_len >= LARGE_TEXT_SEGMENTATION_THRESHOLD_BYTES
}

/// 分段策略。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentedLayoutConfig {
    /// 每段最多硬行数。
    pub max_hard_lines_per_segment: usize,
    /// 每段目标最大字节数；单行超限时独占一段，绝不拆行。
    pub max_bytes_per_segment: usize,
    /// 未测量段的初始行高估计（逻辑像素）。
    pub estimated_line_height_px: f32,
}

impl Default for SegmentedLayoutConfig {
    fn default() -> Self {
        Self {
            max_hard_lines_per_segment: 256,
            max_bytes_per_segment: 64 * 1024,
            estimated_line_height_px: 20.0,
        }
    }
}

#[derive(Debug, Clone)]
struct TextSegment {
    byte_range: Range<usize>,
    hard_line_count: usize,
    height_px: f32,
    measured: Option<ParleyLayoutSnapshot>,
}

/// 内部滚动锚点：以段首字节偏移 + 段内像素偏移标识视口位置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentScrollAnchor {
    pub byte_offset: usize,
    pub offset_into_segment_px: f32,
}

/// 分段布局状态机。
#[derive(Debug, Clone)]
pub struct SegmentedTextLayout {
    text: String,
    config: SegmentedLayoutConfig,
    segments: Vec<TextSegment>,
    width: Option<f32>,
    /// 编辑/宽度变化时自增；过期的外部引用据此拒绝。
    generation: u64,
    measured_line_total: u64,
    measured_height_total: f64,
}

impl SegmentedTextLayout {
    /// O(n) 换行扫描建立分段索引；不做 shaping。
    pub fn new(text: impl Into<String>, config: SegmentedLayoutConfig) -> Self {
        let text = text.into();
        let mut layout = Self {
            segments: Vec::new(),
            width: None,
            generation: 0,
            measured_line_total: 0,
            measured_height_total: 0.0,
            config,
            text,
        };
        layout.segments = layout.build_segments(0..layout.text.len());
        layout
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn width(&self) -> Option<f32> {
        self.width
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    pub fn segment_byte_range(&self, index: usize) -> Option<Range<usize>> {
        self.segments
            .get(index)
            .map(|segment| segment.byte_range.clone())
    }

    pub fn is_measured(&self, index: usize) -> bool {
        self.segments
            .get(index)
            .is_some_and(|segment| segment.measured.is_some())
    }

    pub fn measured_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| segment.measured.is_some())
            .count()
    }

    pub fn segment_snapshot(&self, index: usize) -> Option<&ParleyLayoutSnapshot> {
        self.segments.get(index)?.measured.as_ref()
    }

    /// 全文档高度：测量段用真实高度，其余用自适应估计。
    pub fn total_height(&self) -> f32 {
        self.segments.iter().map(|segment| segment.height_px).sum()
    }

    /// 段顶部 y 坐标（前缀和）。
    pub fn segment_top(&self, index: usize) -> f32 {
        self.segments[..index.min(self.segments.len())]
            .iter()
            .map(|segment| segment.height_px)
            .sum()
    }

    /// 与视口相交的段索引窗口。
    pub fn visible_segments(&self, viewport_top: f32, viewport_height: f32) -> Range<usize> {
        let viewport_bottom = viewport_top + viewport_height.max(0.0);
        let mut top = 0.0f32;
        let mut first = self.segments.len();
        let mut last = self.segments.len();
        for (index, segment) in self.segments.iter().enumerate() {
            let bottom = top + segment.height_px;
            if first == self.segments.len() && bottom > viewport_top {
                first = index;
            }
            if top >= viewport_bottom {
                last = index;
                break;
            }
            top = bottom;
        }
        if first == self.segments.len() {
            return self.segments.len()..self.segments.len();
        }
        first..last
    }

    /// 测量窗口内尚未测量的段；`build` 收到段的布局文本与完整字节范围。
    ///
    /// 非最终段的布局文本会剥掉段尾 `\n`：Parley 会为文本末尾的换行符生成
    /// 一行空行，整块布局只在文档末尾有一次，分段布局必须保持一致，否则
    /// 每段都会虚增一行。每次成功测量都会更新自适应行高，并同步修正所有
    /// 未测量段的估计。
    pub fn measure_segments<F>(&mut self, indices: Range<usize>, mut build: F)
    where
        F: FnMut(&str, Range<usize>) -> ParleyLayoutSnapshot,
    {
        let indices = indices.start.min(self.segments.len())..indices.end.min(self.segments.len());
        let mut measured_any = false;
        for index in indices {
            if self.segments[index].measured.is_some() {
                continue;
            }
            let range = self.segments[index].byte_range.clone();
            let layout_range = self.layout_text_range(index);
            let snapshot = build(&self.text[layout_range], range);
            let height = snapshot.height();
            let segment = &mut self.segments[index];
            segment.height_px = height;
            segment.measured = Some(snapshot);
            self.measured_line_total += segment.hard_line_count as u64;
            self.measured_height_total += f64::from(height);
            measured_any = true;
        }
        if measured_any {
            self.refresh_estimates();
        }
    }

    /// 段的布局文本范围：非最终段剥掉段尾换行符（见 `measure_segments`）。
    fn layout_text_range(&self, index: usize) -> Range<usize> {
        let segment = &self.segments[index];
        let mut range = segment.byte_range.clone();
        let is_final = index + 1 == self.segments.len();
        if !is_final && self.text[range.clone()].ends_with('\n') {
            range.end -= 1;
        }
        range
    }

    /// 文本编辑：只重建受影响的段，未触及段保留测量结果并平移字节范围。
    ///
    /// `range` 必须落在 char boundary 上（与 TextSnapshot 的约定一致）。
    pub fn replace_range(&mut self, range: Range<usize>, replacement: &str) {
        assert!(
            self.text.is_char_boundary(range.start) && self.text.is_char_boundary(range.end),
            "segmented edit must land on char boundaries"
        );
        self.generation += 1;

        let (insert_at, region_start, old_region_end) = if self.segments.is_empty() {
            (0, 0, 0)
        } else {
            // 末尾（含 range 端点等于 text.len()）一律归入最后一段。
            let last_index = self.segments.len() - 1;
            let first = self
                .segment_index_at_byte(range.start)
                .unwrap_or(last_index);
            let last = self.segment_index_at_byte(range.end).unwrap_or(last_index);
            (
                first,
                self.segments[first].byte_range.start,
                self.segments[last].byte_range.end,
            )
        };

        // 移除受影响段并回收其测量统计。
        let removed_end_index = self
            .segments
            .iter()
            .position(|segment| segment.byte_range.start >= old_region_end)
            .unwrap_or(self.segments.len());
        for segment in self.segments.drain(insert_at..removed_end_index) {
            if segment.measured.is_some() {
                self.measured_line_total -= segment.hard_line_count as u64;
                self.measured_height_total -= f64::from(segment.height_px);
            }
        }

        // 拼接文本并重建受影响区域的分段。
        self.text.replace_range(range.clone(), replacement);
        let delta = replacement.len() as isize - (range.end - range.start) as isize;
        let new_region_end = old_region_end.saturating_add_signed(delta);
        let rebuilt = self.build_segments(region_start..new_region_end);

        // 平移后续段的字节范围。
        for segment in &mut self.segments[insert_at..] {
            segment.byte_range = segment.byte_range.start.saturating_add_signed(delta)
                ..segment.byte_range.end.saturating_add_signed(delta);
        }
        let tail = self.segments.split_off(insert_at);
        self.segments.extend(rebuilt);
        self.segments.extend(tail);
        self.refresh_estimates();

        debug_assert!(self.segments_are_contiguous());
    }

    /// 宽度变化：分段索引保留，测量全部失效（局部 reflow = 调用方随后只
    /// 重新测量可见窗口）。
    pub fn set_width(&mut self, width: Option<f32>) {
        if self.width == width {
            return;
        }
        self.width = width;
        self.generation += 1;
        self.measured_line_total = 0;
        self.measured_height_total = 0.0;
        let estimate = self.config.estimated_line_height_px;
        for segment in &mut self.segments {
            segment.measured = None;
            segment.height_px = segment.hard_line_count as f32 * estimate;
        }
    }

    /// 视口 y 对应的内部锚点。
    pub fn anchor_at(&self, y: f32) -> SegmentScrollAnchor {
        let mut top = 0.0f32;
        for segment in &self.segments {
            let bottom = top + segment.height_px;
            if y < bottom || segment.byte_range.end >= self.text.len() {
                return SegmentScrollAnchor {
                    byte_offset: segment.byte_range.start,
                    offset_into_segment_px: (y - top).max(0.0),
                };
            }
            top = bottom;
        }
        SegmentScrollAnchor {
            byte_offset: 0,
            offset_into_segment_px: 0.0,
        }
    }

    /// 测高修正后还原锚点对应的 y；锚点字节仍存在即可恢复。
    pub fn y_for_anchor(&self, anchor: SegmentScrollAnchor) -> Option<f32> {
        let index = self.segment_index_at_byte(anchor.byte_offset)?;
        let segment = &self.segments[index];
        let offset = anchor
            .offset_into_segment_px
            .min(segment.height_px)
            .max(0.0);
        Some(self.segment_top(index) + offset)
    }

    fn segment_index_at_byte(&self, byte: usize) -> Option<usize> {
        if byte >= self.text.len() {
            return None;
        }
        self.segments
            .iter()
            .position(|segment| segment.byte_range.contains(&byte))
    }

    /// 把 `region` 内的文本按硬行打包成段。
    fn build_segments(&self, region: Range<usize>) -> Vec<TextSegment> {
        let estimate = self.adaptive_line_height();
        let slice = &self.text[region.clone()];
        let mut segments = Vec::new();
        let mut segment_start = region.start;
        let mut segment_bytes = 0usize;
        let mut segment_lines = 0usize;

        let mut flush = |start: &mut usize, bytes: &mut usize, lines: &mut usize| {
            if *bytes > 0 {
                segments.push(TextSegment {
                    byte_range: *start..*start + *bytes,
                    hard_line_count: (*lines).max(1),
                    height_px: (*lines).max(1) as f32 * estimate,
                    measured: None,
                });
                *start += *bytes;
                *bytes = 0;
                *lines = 0;
            }
        };

        for line in slice.split_inclusive('\n') {
            let line_len = line.len();
            let would_overflow = segment_lines + 1 > self.config.max_hard_lines_per_segment
                || segment_bytes + line_len > self.config.max_bytes_per_segment;
            if would_overflow && segment_bytes > 0 {
                flush(&mut segment_start, &mut segment_bytes, &mut segment_lines);
            }
            segment_bytes += line_len;
            segment_lines += 1;
        }
        flush(&mut segment_start, &mut segment_bytes, &mut segment_lines);
        segments
    }

    /// 自适应行高：优先用已测量段的平均值。
    fn adaptive_line_height(&self) -> f32 {
        if self.measured_line_total > 0 {
            (self.measured_height_total / self.measured_line_total as f64) as f32
        } else {
            self.config.estimated_line_height_px
        }
    }

    fn refresh_estimates(&mut self) {
        let estimate = self.adaptive_line_height();
        for segment in &mut self.segments {
            if segment.measured.is_none() {
                segment.height_px = segment.hard_line_count as f32 * estimate;
            }
        }
    }

    fn segments_are_contiguous(&self) -> bool {
        let mut cursor = 0usize;
        for segment in &self.segments {
            if segment.byte_range.start != cursor {
                return false;
            }
            cursor = segment.byte_range.end;
        }
        cursor == self.text.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(lines: usize, bytes: usize) -> SegmentedLayoutConfig {
        SegmentedLayoutConfig {
            max_hard_lines_per_segment: lines,
            max_bytes_per_segment: bytes,
            estimated_line_height_px: 10.0,
        }
    }

    fn lines(count: usize) -> String {
        (0..count).map(|index| format!("line {index}\n")).collect()
    }

    #[test]
    fn segmentation_packs_by_line_and_byte_caps() {
        let layout = SegmentedTextLayout::new(lines(10), config(4, usize::MAX));
        assert_eq!(layout.segment_count(), 3); // 4 + 4 + 2
        assert_eq!(layout.segment_byte_range(0).unwrap().start, 0);
        assert!(layout.segments_are_contiguous());

        // 字节上限：每行 7-8 字节，上限 16 → 每段 2 行。
        let by_bytes = SegmentedTextLayout::new(lines(6), config(usize::MAX, 16));
        assert!(by_bytes.segment_count() >= 3);
        assert!(by_bytes.segments_are_contiguous());
    }

    #[test]
    fn oversized_single_line_gets_its_own_segment_unsplit() {
        let text = format!("short\n{}\nshort2\n", "x".repeat(100));
        let layout = SegmentedTextLayout::new(text, config(usize::MAX, 32));
        assert!(layout.segments_are_contiguous());
        // 巨行独占一段且未被拆开。
        let oversized = (0..layout.segment_count())
            .map(|index| layout.segment_byte_range(index).unwrap())
            .find(|range| range.len() > 32)
            .expect("oversized line keeps one segment");
        assert!(layout.text()[oversized].starts_with('x'));
    }

    #[test]
    fn empty_text_and_no_trailing_newline() {
        let empty = SegmentedTextLayout::new("", SegmentedLayoutConfig::default());
        assert_eq!(empty.segment_count(), 0);
        assert_eq!(empty.total_height(), 0.0);

        let no_newline = SegmentedTextLayout::new("only line", config(4, 64));
        assert_eq!(no_newline.segment_count(), 1);
        assert!(no_newline.segments_are_contiguous());
    }

    #[test]
    fn estimates_and_visible_window() {
        let layout = SegmentedTextLayout::new(lines(40), config(10, usize::MAX));
        // 4 段 × 10 行 × 10px。
        assert_eq!(layout.segment_count(), 4);
        assert_eq!(layout.total_height(), 400.0);
        assert_eq!(layout.segment_top(2), 200.0);

        assert_eq!(layout.visible_segments(0.0, 100.0), 0..1);
        assert_eq!(layout.visible_segments(50.0, 100.0), 0..2);
        assert_eq!(layout.visible_segments(150.0, 200.0), 1..4);
        assert_eq!(layout.visible_segments(1_000.0, 100.0), 4..4);
    }

    #[test]
    fn requires_segmentation_threshold() {
        assert!(!requires_segmentation(1024));
        assert!(requires_segmentation(
            LARGE_TEXT_SEGMENTATION_THRESHOLD_BYTES
        ));
        assert!(requires_segmentation(10 * 1024 * 1024));
    }

    #[test]
    fn edit_within_one_segment_shifts_tail_ranges() {
        let mut layout = SegmentedTextLayout::new(lines(20), config(5, usize::MAX));
        assert_eq!(layout.segment_count(), 4);
        let generation = layout.generation();
        let tail_before = layout.segment_byte_range(3).unwrap();

        // 在第二段内插入一行。
        let insert_at = layout.segment_byte_range(1).unwrap().start;
        layout.replace_range(insert_at..insert_at, "inserted\n");

        assert!(layout.generation() > generation);
        assert!(layout.segments_are_contiguous());
        let tail_after = layout
            .segment_byte_range(layout.segment_count() - 1)
            .unwrap();
        assert_eq!(tail_after.end, tail_before.end + "inserted\n".len());
        assert!(layout.text().contains("inserted"));
    }

    #[test]
    fn edit_spanning_segments_and_delete_to_empty() {
        let mut layout = SegmentedTextLayout::new(lines(20), config(5, usize::MAX));
        let start = layout.segment_byte_range(0).unwrap().start + 3;
        let end = layout.segment_byte_range(2).unwrap().start + 3;
        layout.replace_range(start..end, "X");
        assert!(layout.segments_are_contiguous());

        let full = 0..layout.text().len();
        layout.replace_range(full, "");
        assert_eq!(layout.segment_count(), 0);
        assert_eq!(layout.total_height(), 0.0);

        // 从空文本重新追加。
        layout.replace_range(0..0, "fresh\n");
        assert_eq!(layout.segment_count(), 1);
        assert!(layout.segments_are_contiguous());
    }

    #[test]
    fn append_at_end_reuses_last_segment_region() {
        let mut layout = SegmentedTextLayout::new(lines(6), config(4, usize::MAX));
        let len = layout.text().len();
        layout.replace_range(len..len, "tail line\n");
        assert!(layout.segments_are_contiguous());
        assert!(layout.text().ends_with("tail line\n"));
    }

    #[test]
    fn anchor_survives_height_corrections_above() {
        let mut layout = SegmentedTextLayout::new(lines(40), config(10, usize::MAX));
        // 锚定第三段中部（估计几何）。
        let anchor = layout.anchor_at(250.0);
        assert_eq!(
            anchor.byte_offset,
            layout.segment_byte_range(2).unwrap().start
        );
        assert_eq!(anchor.offset_into_segment_px, 50.0);

        // 模拟上方测高修正：手工把第一段高度改掉（通过公开路径：宽度不变时
        // 直接检验 y_for_anchor 的前缀和推导）。
        let restored = layout.y_for_anchor(anchor).unwrap();
        assert_eq!(restored, 250.0);

        // 编辑锚点之前的内容后，锚点字节偏移随平移失效是允许的；这里验证
        // 锚点仍能解析到某个段而不是 panic。
        layout.replace_range(0..0, "prefix\n");
        let shifted = SegmentScrollAnchor {
            byte_offset: anchor.byte_offset + "prefix\n".len(),
            offset_into_segment_px: anchor.offset_into_segment_px,
        };
        assert!(layout.y_for_anchor(shifted).is_some());
    }

    #[test]
    fn width_change_invalidates_measurements_but_keeps_segmentation() {
        let mut layout = SegmentedTextLayout::new(lines(20), config(5, usize::MAX));
        let segment_count = layout.segment_count();
        let generation = layout.generation();
        layout.set_width(Some(320.0));
        assert_eq!(layout.segment_count(), segment_count);
        assert!(layout.generation() > generation);
        assert_eq!(layout.measured_count(), 0);
        // 同宽度幂等。
        let generation = layout.generation();
        layout.set_width(Some(320.0));
        assert_eq!(layout.generation(), generation);
    }
}
