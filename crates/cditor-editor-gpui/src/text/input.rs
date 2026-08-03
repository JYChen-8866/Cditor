use std::{borrow::Cow, slice, sync::Arc};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, BlockPayloadView, InlineMark, InlineSpan, RichBlockKind,
    TextAlign,
};
use cditor_runtime::{TableCellSpansSnapshot, TextSurfaceSnapshot, ViewBlockSnapshot};
use cditor_text::{TextLayoutInput, TextLayoutSurfaceId};

#[derive(Debug, Clone, PartialEq)]
pub struct RichTextLayoutSpans {
    storage: RichTextLayoutSpanStorage,
}

#[derive(Debug, Clone, PartialEq)]
enum RichTextLayoutSpanStorage {
    Materialized(Arc<IndexedInlineSpans>),
    SnapshotPayload(Arc<BlockPayloadRecord>),
    TableCell(TableCellSpansSnapshot),
}

#[derive(Debug, PartialEq)]
struct IndexedInlineSpans {
    spans: Arc<Vec<InlineSpan>>,
    end_offsets: Box<[usize]>,
}

impl IndexedInlineSpans {
    fn new(spans: Arc<Vec<InlineSpan>>) -> Self {
        let mut text_len = 0usize;
        let end_offsets = spans
            .iter()
            .map(|span| {
                text_len = text_len.saturating_add(span.text.len());
                text_len
            })
            .collect();
        Self { spans, end_offsets }
    }

    fn text_len(&self) -> usize {
        self.end_offsets.last().copied().unwrap_or(0)
    }

    fn first_overlapping_span(&self, byte_offset: usize) -> usize {
        self.end_offsets.partition_point(|end| *end <= byte_offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichTextLayoutSpanRef<'a> {
    pub text: &'a str,
    pub marks: &'a [InlineMark],
}

pub struct RichTextLayoutSpanIter<'a> {
    inner: RichTextLayoutSpanIterInner<'a>,
}

enum RichTextLayoutSpanIterInner<'a> {
    Materialized(slice::Iter<'a, InlineSpan>),
    Plain(Option<&'a str>),
}

impl<'a> Iterator for RichTextLayoutSpanIter<'a> {
    type Item = RichTextLayoutSpanRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            RichTextLayoutSpanIterInner::Materialized(spans) => {
                spans.next().map(|span| RichTextLayoutSpanRef {
                    text: &span.text,
                    marks: &span.marks,
                })
            }
            RichTextLayoutSpanIterInner::Plain(text) => text
                .take()
                .map(|text| RichTextLayoutSpanRef { text, marks: &[] }),
        }
    }
}

impl RichTextLayoutSpans {
    fn from_snapshot_payload(payload: Arc<BlockPayloadRecord>) -> Option<Self> {
        matches!(
            &payload.payload,
            BlockPayload::RichText { .. } | BlockPayload::Code { .. } | BlockPayload::Html { .. }
        )
        .then_some(Self {
            storage: RichTextLayoutSpanStorage::SnapshotPayload(payload),
        })
    }

    pub fn iter(&self) -> RichTextLayoutSpanIter<'_> {
        let inner = match &self.storage {
            RichTextLayoutSpanStorage::Materialized(spans) => {
                RichTextLayoutSpanIterInner::Materialized(spans.spans.iter())
            }
            RichTextLayoutSpanStorage::SnapshotPayload(payload) => match &payload.payload {
                BlockPayload::RichText { spans } => {
                    RichTextLayoutSpanIterInner::Materialized(spans.iter())
                }
                BlockPayload::Code { text, .. } => RichTextLayoutSpanIterInner::Plain(Some(text)),
                BlockPayload::Html { html, .. } => RichTextLayoutSpanIterInner::Plain(Some(html)),
                _ => RichTextLayoutSpanIterInner::Plain(None),
            },
            RichTextLayoutSpanStorage::TableCell(spans) => {
                RichTextLayoutSpanIterInner::Materialized(spans.iter())
            }
        };
        RichTextLayoutSpanIter { inner }
    }

    pub fn text_len(&self) -> usize {
        match &self.storage {
            RichTextLayoutSpanStorage::Materialized(spans) => spans.text_len(),
            RichTextLayoutSpanStorage::SnapshotPayload(payload) => match &payload.payload {
                BlockPayload::RichText { spans } => spans.iter().map(|span| span.text.len()).sum(),
                BlockPayload::Code { text, .. } => text.len(),
                BlockPayload::Html { html, .. } => html.len(),
                _ => 0,
            },
            RichTextLayoutSpanStorage::TableCell(spans) => {
                spans.iter().map(|span| span.text.len()).sum()
            }
        }
    }

    pub fn is_char_boundary(&self, byte_offset: usize) -> bool {
        if byte_offset > self.text_len() {
            return false;
        }
        let mut consumed = 0usize;
        for span in self.iter() {
            let end = consumed.saturating_add(span.text.len());
            if byte_offset <= end {
                return span.text.is_char_boundary(byte_offset - consumed);
            }
            consumed = end;
        }
        byte_offset == consumed
    }

    pub fn is_empty(&self) -> bool {
        self.text_len() == 0
    }

    pub fn plain_text(&self) -> Cow<'_, str> {
        let mut spans = self.iter();
        let Some(first) = spans.next() else {
            return Cow::Borrowed("");
        };
        let Some(second) = spans.next() else {
            return Cow::Borrowed(first.text);
        };

        record_plain_text_materialization();
        let mut text = String::with_capacity(self.text_len());
        text.push_str(first.text);
        text.push_str(second.text);
        for span in spans {
            text.push_str(span.text);
        }
        Cow::Owned(text)
    }

    pub fn to_vec(&self) -> Vec<InlineSpan> {
        self.iter()
            .map(|span| InlineSpan {
                text: span.text.to_owned(),
                marks: span.marks.to_vec(),
            })
            .collect()
    }

    pub(crate) fn slice(&self, range: std::ops::Range<usize>) -> Vec<InlineSpan> {
        if let RichTextLayoutSpanStorage::Materialized(spans) = &self.storage {
            return slice_materialized_spans(spans, range);
        }

        let mut cursor = 0usize;
        let mut sliced = Vec::new();
        for span in self.iter() {
            let span_range = cursor..cursor + span.text.len();
            cursor = span_range.end;
            if span_range.start >= range.end {
                break;
            }
            let start = range.start.max(span_range.start);
            let end = range.end.min(span_range.end);
            if start >= end {
                continue;
            }
            sliced.push(InlineSpan {
                text: span.text[start - span_range.start..end - span_range.start].to_owned(),
                marks: span.marks.to_vec(),
            });
        }
        if sliced.is_empty() {
            sliced.push(InlineSpan::plain(""));
        }
        sliced
    }

    #[cfg(feature = "code-highlight")]
    pub(crate) fn as_inline_spans(&self) -> Option<&[InlineSpan]> {
        match &self.storage {
            RichTextLayoutSpanStorage::Materialized(spans) => Some(spans.spans.as_slice()),
            RichTextLayoutSpanStorage::SnapshotPayload(payload) => match &payload.payload {
                BlockPayload::RichText { spans } => Some(spans),
                _ => None,
            },
            RichTextLayoutSpanStorage::TableCell(spans) => Some(spans),
        }
    }

    pub(crate) fn cache_identity(&self) -> (u8, usize, usize) {
        match &self.storage {
            RichTextLayoutSpanStorage::Materialized(spans) => {
                (0, Arc::as_ptr(spans) as usize, spans.spans.len())
            }
            RichTextLayoutSpanStorage::SnapshotPayload(payload) => (
                1,
                Arc::as_ptr(payload) as usize,
                payload.content_version as usize,
            ),
            RichTextLayoutSpanStorage::TableCell(spans) => {
                (2, spans.as_ptr() as usize, spans.len())
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn shares_snapshot_payload(&self, payload: &Arc<BlockPayloadRecord>) -> bool {
        matches!(
            &self.storage,
            RichTextLayoutSpanStorage::SnapshotPayload(shared) if Arc::ptr_eq(shared, payload)
        )
    }

    #[cfg(test)]
    pub(crate) fn shares_materialized_spans(&self, spans: &Arc<Vec<InlineSpan>>) -> bool {
        matches!(
            &self.storage,
            RichTextLayoutSpanStorage::Materialized(shared)
                if Arc::ptr_eq(&shared.spans, spans)
        )
    }
}

fn slice_materialized_spans(
    indexed: &IndexedInlineSpans,
    range: std::ops::Range<usize>,
) -> Vec<InlineSpan> {
    let first = indexed.first_overlapping_span(range.start);
    let mut cursor = first
        .checked_sub(1)
        .and_then(|index| indexed.end_offsets.get(index).copied())
        .unwrap_or(0);
    let mut sliced = Vec::new();
    for span in &indexed.spans[first..] {
        let span_range = cursor..cursor + span.text.len();
        cursor = span_range.end;
        if span_range.start >= range.end {
            break;
        }
        let start = range.start.max(span_range.start);
        let end = range.end.min(span_range.end);
        if start >= end {
            continue;
        }
        sliced.push(InlineSpan {
            text: span.text[start - span_range.start..end - span_range.start].to_owned(),
            marks: span.marks.clone(),
        });
    }
    if sliced.is_empty() {
        sliced.push(InlineSpan::plain(""));
    }
    sliced
}

impl From<Vec<InlineSpan>> for RichTextLayoutSpans {
    fn from(spans: Vec<InlineSpan>) -> Self {
        Self::from(Arc::new(spans))
    }
}

impl From<Arc<Vec<InlineSpan>>> for RichTextLayoutSpans {
    fn from(spans: Arc<Vec<InlineSpan>>) -> Self {
        Self {
            storage: RichTextLayoutSpanStorage::Materialized(Arc::new(IndexedInlineSpans::new(
                spans,
            ))),
        }
    }
}

impl From<TableCellSpansSnapshot> for RichTextLayoutSpans {
    fn from(spans: TableCellSpansSnapshot) -> Self {
        Self {
            storage: RichTextLayoutSpanStorage::TableCell(spans),
        }
    }
}

impl<'a> IntoIterator for &'a RichTextLayoutSpans {
    type Item = RichTextLayoutSpanRef<'a>;
    type IntoIter = RichTextLayoutSpanIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
thread_local! {
    static PLAIN_TEXT_MATERIALIZATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_plain_text_materialization() {
    PLAIN_TEXT_MATERIALIZATIONS.with(|count| count.set(count.get() + 1));
}

#[cfg(not(test))]
fn record_plain_text_materialization() {}

#[cfg(test)]
pub(crate) fn reset_plain_text_materializations_for_tests() {
    PLAIN_TEXT_MATERIALIZATIONS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn plain_text_materializations_for_tests() -> usize {
    PLAIN_TEXT_MATERIALIZATIONS.with(std::cell::Cell::get)
}

#[derive(Debug, Clone, PartialEq)]
pub struct RichTextLayoutInput {
    pub block_id: BlockId,
    pub surface_id: TextLayoutSurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
    pub kind: RichBlockKind,
    pub text_align: TextAlign,
    pub spans: RichTextLayoutSpans,
    pub width_px: f64,
    pub theme_version: u64,
    pub font_version: u64,
}

impl RichTextLayoutInput {
    pub fn from_text_surface_snapshot(
        snapshot: TextSurfaceSnapshot,
        layout_version: u64,
        text_align: TextAlign,
        width_px: f64,
        theme_version: u64,
        font_version: u64,
    ) -> Self {
        Self {
            block_id: snapshot.owner_block_id,
            surface_id: snapshot.identity.surface_id,
            content_version: snapshot.identity.content_version,
            layout_version,
            kind: snapshot.kind,
            text_align,
            spans: snapshot.spans.into(),
            width_px,
            theme_version,
            font_version,
        }
    }

    pub fn from_snapshot(
        snapshot: &ViewBlockSnapshot,
        width_px: f64,
        theme_version: u64,
        font_version: u64,
    ) -> Option<Self> {
        let BlockPayloadView::Loaded(payload) = &snapshot.payload else {
            return None;
        };
        let spans = RichTextLayoutSpans::from_snapshot_payload(payload.clone())?;

        Some(Self {
            block_id: snapshot.block_id,
            surface_id: TextLayoutSurfaceId::Block(snapshot.block_id),
            content_version: payload.content_version,
            layout_version: snapshot.layout.layout_version,
            kind: snapshot.kind.clone(),
            text_align: snapshot.attrs.text_align,
            spans,
            width_px,
            theme_version,
            font_version,
        })
    }

    pub(crate) fn to_text_layout_input(&self) -> TextLayoutInput {
        TextLayoutInput {
            surface_id: self.surface_id,
            content_version: self.content_version,
            layout_version: self.layout_version,
            kind: self.kind.clone(),
            text_align: self.text_align,
            spans: self.spans.to_vec(),
            width_px: self.width_px,
            theme_version: self.theme_version,
            font_version: self.font_version,
        }
    }
    pub fn text_len(&self) -> usize {
        self.spans.text_len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub fn plain_text(&self) -> Cow<'_, str> {
        self.spans.plain_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn rich_text_layout_input_from_snapshot() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let snapshot = projection
            .blocks
            .iter()
            .find(|block| matches!(block.kind, RichBlockKind::Paragraph))
            .expect("demo contains paragraph block");

        let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
            .expect("paragraph payload should produce rich text layout input");

        assert_eq!(input.block_id, snapshot.block_id);
        assert_eq!(input.content_version, 1);
        assert_eq!(input.layout_version, snapshot.layout.layout_version);
        assert!(matches!(input.kind, RichBlockKind::Paragraph));
        assert_eq!(input.width_px, 860.0);
        assert_eq!(input.theme_version, 1);
        assert_eq!(input.font_version, 1);
        assert!(!input.spans.is_empty());
        let BlockPayloadView::Loaded(payload) = &snapshot.payload else {
            panic!("demo paragraph payload must be loaded");
        };
        assert!(input.spans.shares_snapshot_payload(payload));
    }

    #[test]
    fn code_payload_produces_editable_text_input() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: "fn main() {}".to_owned(),
                },
            }],
            720.0,
        );
        let projection = runtime.projection_for_window();
        let snapshot = projection.blocks.first().expect("code block is visible");

        let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
            .expect("code payload should produce editable text input");

        assert!(matches!(input.kind, RichBlockKind::Code { .. }));
        assert!(!input.spans.is_empty());
        assert!(input.spans.iter().any(|span| span.text.contains("fn ")));
    }

    #[test]
    fn ten_mib_code_snapshot_stays_shared_until_layout_materialization() {
        let text = "0123456789abcdef\n".repeat((10 * 1024 * 1024) / 17 + 1);
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 77,
                content_version: 9,
                kind: RichBlockKind::Code { language: None },
                payload: BlockPayload::Code {
                    language: None,
                    text,
                },
            }],
            720.0,
        );
        let projection = runtime.projection_for_window();
        let snapshot = projection.blocks.first().expect("code block is visible");
        let BlockPayloadView::Loaded(payload) = &snapshot.payload else {
            panic!("code payload must be loaded");
        };
        let BlockPayload::Code { text, .. } = &payload.payload else {
            panic!("fixture must remain code");
        };

        let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
            .expect("code payload should produce layout input");
        let plain = input.plain_text();

        assert!(input.spans.shares_snapshot_payload(payload));
        assert!(matches!(plain, Cow::Borrowed(_)));
        assert_eq!(plain.len(), text.len());
        assert_eq!(plain.as_ptr(), text.as_ptr());
    }

    #[test]
    fn highlighted_spans_are_shared_across_layout_input_clones() {
        let spans = Arc::new(vec![InlineSpan {
            text: "fn main() {}".to_owned(),
            marks: vec![InlineMark::Bold],
        }]);
        let layout_spans = RichTextLayoutSpans::from(spans.clone());
        let cloned = layout_spans.clone();

        assert!(layout_spans.shares_materialized_spans(&spans));
        assert!(cloned.shares_materialized_spans(&spans));
    }

    #[test]
    fn materialized_span_index_jumps_directly_to_a_deep_slice() {
        let spans = Arc::new(
            (0..10_000)
                .map(|index| InlineSpan {
                    text: "x".to_owned(),
                    marks: (index == 9_998)
                        .then_some(InlineMark::Bold)
                        .into_iter()
                        .collect(),
                })
                .collect::<Vec<_>>(),
        );
        let layout_spans = RichTextLayoutSpans::from(spans);
        let RichTextLayoutSpanStorage::Materialized(indexed) = &layout_spans.storage else {
            panic!("Arc<Vec<InlineSpan>> must build an indexed materialized store");
        };

        assert_eq!(indexed.first_overlapping_span(9_997), 9_997);
        let sliced = layout_spans.slice(9_997..9_999);
        assert_eq!(sliced.len(), 2);
        assert_eq!(
            sliced
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "xx"
        );
        assert!(sliced[1].marks.contains(&InlineMark::Bold));
    }

    #[test]
    fn table_cell_snapshot_keeps_text_allocation_when_entering_layout_input() {
        let spans = TableCellSpansSnapshot::from(vec![InlineSpan::plain("shared cell")]);
        let text_ptr = spans[0].text.as_ptr();

        let layout_spans = RichTextLayoutSpans::from(spans);
        let layout_span = layout_spans
            .iter()
            .next()
            .expect("table cell fixture contains one span");

        assert_eq!(layout_span.text.as_ptr(), text_ptr);
    }

    #[test]
    fn auxiliary_surface_layout_identity_keeps_content_and_layout_versions_distinct() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord {
                block_id: 7,
                content_version: 9,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
                    caption: "caption".into(),
                    ..Default::default()
                }),
            }],
            720.0,
        );
        let snapshot = runtime
            .text_surface_snapshot(cditor_core::ids::SurfaceId::ImageCaption { block_id: 7 })
            .unwrap();

        let input = RichTextLayoutInput::from_text_surface_snapshot(
            snapshot,
            41,
            TextAlign::Center,
            560.0,
            3,
            5,
        );

        assert_eq!(input.content_version, 9);
        assert_eq!(input.layout_version, 41);
        assert_eq!(
            input.surface_id,
            cditor_core::ids::SurfaceId::ImageCaption { block_id: 7 }
        );
    }

    #[test]
    fn paragraph_heading_list_and_code_share_the_same_layout_input_pipeline() {
        let fixtures = [
            (
                1,
                RichBlockKind::Paragraph,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("paragraph")],
                },
            ),
            (
                2,
                RichBlockKind::Heading { level: 2 },
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("heading")],
                },
            ),
            (
                3,
                RichBlockKind::BulletedList,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("list")],
                },
            ),
            (
                4,
                RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: "fn main() {}".to_owned(),
                },
            ),
        ];
        let runtime = DocumentRuntime::from_payloads(
            1,
            fixtures
                .iter()
                .map(
                    |(block_id, kind, payload)| cditor_core::rich_text::BlockPayloadRecord {
                        block_id: *block_id,
                        content_version: 1,
                        kind: kind.clone(),
                        payload: payload.clone(),
                    },
                )
                .collect(),
            720.0,
        );
        let projection = runtime.projection_for_window();

        for (block_id, kind, _) in fixtures {
            let snapshot = projection
                .blocks
                .iter()
                .find(|block| block.block_id == block_id)
                .expect("fixture block is visible");
            let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
                .expect("primary text kind must use the shared layout input");

            assert_eq!(input.kind, kind);
            assert_eq!(input.surface_id, TextLayoutSurfaceId::Block(block_id));
            assert!(!input.to_text_layout_input().plain_text().is_empty());
        }
    }

    #[test]
    fn app_input_converts_to_framework_independent_text_input() {
        let input = RichTextLayoutInput {
            block_id: 9,
            surface_id: TextLayoutSurfaceId::Block(9),
            content_version: 2,
            layout_version: 3,
            kind: RichBlockKind::Paragraph,
            text_align: TextAlign::Center,
            spans: vec![InlineSpan::plain("centered")].into(),
            width_px: 640.0,
            theme_version: 4,
            font_version: 5,
        };

        let text_input = input.to_text_layout_input();

        assert_eq!(text_input.surface_id.block_id(), Some(input.block_id));
        assert_eq!(text_input.kind, input.kind);
        assert_eq!(text_input.text_align, TextAlign::Center);
        assert_eq!(text_input.plain_text(), "centered");
        assert_eq!(text_input.theme_version, 4);
        assert_eq!(text_input.font_version, 5);
    }
}
