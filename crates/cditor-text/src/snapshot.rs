use std::sync::Arc;

use parley::{Layout, PositionedLayoutItem};

use super::{InlineBoxKind, TextAlignment, TextBrush};
use crate::{TextGeometrySnapshot, TextPaintPlan, TextSnapshot};

#[derive(Clone)]
pub struct TextLayoutSnapshot {
    pub(crate) inner: Arc<ParleyLayoutData>,
}

pub(crate) struct ParleyLayoutData {
    pub text: TextSnapshot,
    pub layout: Layout<TextBrush>,
    pub clusters: Arc<[TextClusterSnapshot]>,
    pub geometry: TextGeometrySnapshot,
    pub estimated_bytes: usize,
    pub display_scale: f32,
    pub width: Option<f32>,
    pub alignment: TextAlignment,
    pub quantize: bool,
    pub paint_plan: TextPaintPlan,
}

impl std::fmt::Debug for TextLayoutSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextLayoutSnapshot")
            .field("text_len", &self.inner.text.len_bytes())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("line_count", &self.line_count())
            .field("is_rtl", &self.is_rtl())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLineSnapshot {
    pub index: usize,
    pub text_range: std::ops::Range<usize>,
    pub baseline: f32,
    pub top: f32,
    pub bottom: f32,
    pub advance: f32,
    pub offset: f32,
    pub logical_left: f32,
    pub logical_right: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionedInlineBox {
    pub id: u64,
    pub kind: InlineBoxKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextClusterSnapshot {
    pub text_range: std::ops::Range<usize>,
    pub line_index: usize,
    pub run_index: usize,
    pub logical_index: usize,
    pub visual_index: usize,
    pub is_rtl: bool,
    pub is_word_boundary: bool,
    pub is_emoji: bool,
    pub is_ligature_start: bool,
    pub is_ligature_continuation: bool,
}

impl TextLayoutSnapshot {
    pub(crate) fn new(
        text: TextSnapshot,
        layout: Layout<TextBrush>,
        display_scale: f32,
        width: Option<f32>,
        alignment: TextAlignment,
        quantize: bool,
    ) -> Self {
        let clusters = collect_cluster_snapshots(&layout);
        let geometry = TextGeometrySnapshot::from_layout(&text, &layout, display_scale);
        let paint_plan = TextPaintPlan::from_layout(&layout, display_scale);
        let estimated_bytes = estimate_layout_bytes(&text, &layout, &clusters)
            .saturating_add(geometry.estimated_bytes());
        Self {
            inner: Arc::new(ParleyLayoutData {
                text,
                layout,
                clusters,
                geometry,
                estimated_bytes,
                display_scale,
                width,
                alignment,
                quantize,
                paint_plan,
            }),
        }
    }

    pub fn text(&self) -> &str {
        self.inner.text.as_str()
    }

    pub fn text_snapshot(&self) -> &TextSnapshot {
        &self.inner.text
    }

    pub fn display_scale(&self) -> f32 {
        self.inner.display_scale
    }

    pub fn width(&self) -> f32 {
        self.inner.layout.width() / self.inner.display_scale
    }

    pub fn full_width(&self) -> f32 {
        self.inner.layout.full_width() / self.inner.display_scale
    }

    pub fn height(&self) -> f32 {
        self.inner.layout.height() / self.inner.display_scale
    }

    pub fn line_count(&self) -> usize {
        self.inner.layout.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.text.is_empty()
    }

    pub fn is_rtl(&self) -> bool {
        self.inner.layout.is_rtl()
    }

    pub fn layout_width(&self) -> Option<f32> {
        self.inner.width
    }

    pub fn alignment(&self) -> TextAlignment {
        self.inner.alignment
    }

    pub fn line_snapshots(&self) -> &[TextLineSnapshot] {
        self.inner.geometry.lines()
    }

    pub fn inline_boxes(&self) -> Vec<PositionedInlineBox> {
        let scale = self.inner.display_scale;
        self.inner
            .layout
            .lines()
            .flat_map(|line| line.items())
            .filter_map(|item| match item {
                PositionedLayoutItem::InlineBox(inline_box) => Some(PositionedInlineBox {
                    id: inline_box.id,
                    kind: InlineBoxKind::from_parley(inline_box.kind),
                    x: inline_box.x / scale,
                    y: inline_box.y / scale,
                    width: inline_box.width / scale,
                    height: inline_box.height / scale,
                }),
                PositionedLayoutItem::GlyphRun(_) => None,
            })
            .collect()
    }

    pub fn clusters(&self) -> &[TextClusterSnapshot] {
        &self.inner.clusters
    }

    pub fn cluster_for_byte_offset(&self, byte_offset: usize) -> Option<&TextClusterSnapshot> {
        self.inner
            .clusters
            .iter()
            .find(|cluster| cluster.text_range.contains(&byte_offset))
    }

    pub fn estimated_bytes(&self) -> usize {
        self.inner.estimated_bytes
    }

    pub fn geometry(&self) -> &TextGeometrySnapshot {
        &self.inner.geometry
    }

    pub fn reflow(&self, width: Option<f32>, alignment: TextAlignment) -> Self {
        let mut layout = self.inner.layout.clone();
        layout.break_all_lines(width.map(|width| width.max(0.0) * self.inner.display_scale));
        layout.align(alignment.as_parley(), parley::AlignmentOptions::default());
        Self::new(
            self.inner.text.clone(),
            layout,
            self.inner.display_scale,
            width,
            alignment,
            self.inner.quantize,
        )
    }

    pub(crate) fn layout(&self) -> &Layout<TextBrush> {
        &self.inner.layout
    }

    pub fn paint_plan(&self) -> &TextPaintPlan {
        &self.inner.paint_plan
    }
}

fn collect_cluster_snapshots(layout: &Layout<TextBrush>) -> Arc<[TextClusterSnapshot]> {
    let mut clusters = Vec::new();
    for (line_index, line) in layout.lines().enumerate() {
        for run in line.runs() {
            for (logical_index, cluster) in run.clusters().enumerate() {
                clusters.push(TextClusterSnapshot {
                    text_range: cluster.text_range(),
                    line_index,
                    run_index: run.index(),
                    logical_index,
                    visual_index: run
                        .logical_to_visual(logical_index)
                        .unwrap_or(logical_index),
                    is_rtl: run.is_rtl(),
                    is_word_boundary: cluster.is_word_boundary(),
                    is_emoji: cluster.is_emoji(),
                    is_ligature_start: cluster.is_ligature_start(),
                    is_ligature_continuation: cluster.is_ligature_continuation(),
                });
            }
        }
    }
    clusters.sort_by_key(|cluster| (cluster.text_range.start, cluster.text_range.end));
    clusters.into()
}

fn estimate_layout_bytes(
    text: &TextSnapshot,
    layout: &Layout<TextBrush>,
    clusters: &[TextClusterSnapshot],
) -> usize {
    let mut glyph_count = 0usize;
    let mut inline_box_count = 0usize;
    for line in layout.lines() {
        for item in line.items() {
            match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => {
                    glyph_count = glyph_count.saturating_add(glyph_run.glyphs().count());
                }
                PositionedLayoutItem::InlineBox(_) => {
                    inline_box_count = inline_box_count.saturating_add(1);
                }
            }
        }
    }

    std::mem::size_of::<ParleyLayoutData>()
        .saturating_add(text.estimated_bytes())
        .saturating_add(
            clusters
                .len()
                .saturating_mul(std::mem::size_of::<TextClusterSnapshot>()),
        )
        .saturating_add(
            layout
                .len()
                .saturating_mul(std::mem::size_of::<TextLineSnapshot>()),
        )
        .saturating_add(glyph_count.saturating_mul(std::mem::size_of::<crate::TextPaintGlyph>()))
        .saturating_add(inline_box_count.saturating_mul(std::mem::size_of::<PositionedInlineBox>()))
}
