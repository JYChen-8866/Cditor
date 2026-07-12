use std::{collections::HashMap, ops::Range};

use gpui::{Pixels, Size};

use crate::gui::app::interaction::geometry::ProjectedBlockRect;
use crate::gui::document::DEFAULT_DOCUMENT_PAGE_WIDTH_PX;
use crate::gui::overlay::{FloatingToolbarState, InlineFormatAction, floating_toolbar_position};
use crate::gui::text::{RichTextPlatformLayout, platform_range_bounds};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, InlineMark, InlineSpan, RichBlockKind};
use cditor_runtime::DocumentRuntime;

use super::CditorV2View;

pub(in crate::gui::app) fn formatting_toolbar_state(
    runtime: Option<&DocumentRuntime>,
    text_layouts: &HashMap<cditor_core::ids::BlockId, RichTextPlatformLayout>,
    readonly: bool,
    conflicting_overlay_open: bool,
    viewport: Size<Pixels>,
    gutter_toolbar_block_id: Option<BlockId>,
    projected_block_rects: &[ProjectedBlockRect],
    scroll_top: f64,
) -> Option<FloatingToolbarState> {
    if readonly || conflicting_overlay_open {
        return None;
    }
    let runtime = runtime?;
    if runtime.ai_session_snapshot().is_some() {
        return None;
    }
    if let Some(block_id) = gutter_toolbar_block_id {
        let rect = projected_block_rects
            .iter()
            .find(|rect| rect.block_id == block_id)?;
        let page_left =
            ((f32::from(viewport.width) - DEFAULT_DOCUMENT_PAGE_WIDTH_PX) / 2.0).max(0.0);
        let top = (rect.document_top - scroll_top) as f32;
        let bottom = (rect.document_bottom - scroll_top) as f32;
        let left = page_left + rect.text_origin_x_in_block_px as f32;
        let right = left + rect.text_width_px as f32;
        let (x, y) = floating_toolbar_position(
            left,
            top,
            right,
            bottom,
            f32::from(viewport.width),
            f32::from(viewport.height),
        );
        let (bold, italic, underline, strike, code) = runtime
            .block_payload_record(block_id)
            .map(|payload| {
                let code_block = matches!(payload.kind, RichBlockKind::Code { .. });
                let marks = toolbar_spans_for_payload(&payload.payload)
                    .map(|spans| {
                        let range = 0..payload.plain_text().len();
                        (
                            selected_spans_have_mark(
                                spans,
                                range.clone(),
                                InlineFormatAction::Bold,
                            ),
                            selected_spans_have_mark(
                                spans,
                                range.clone(),
                                InlineFormatAction::Italic,
                            ),
                            selected_spans_have_mark(
                                spans,
                                range.clone(),
                                InlineFormatAction::Underline,
                            ),
                            selected_spans_have_mark(spans, range, InlineFormatAction::Strike),
                        )
                    })
                    .unwrap_or((false, false, false, false));
                (marks.0, marks.1, marks.2, marks.3, code_block)
            })
            .unwrap_or((false, false, false, false, false));
        return Some(FloatingToolbarState {
            x,
            y,
            block_id: Some(block_id),
            has_text_selection: false,
            show_delete: true,
            bold,
            italic,
            underline,
            strike,
            code,
        });
    }
    if runtime.focused_table_cell_offset().is_some() {
        return None;
    }
    let block_id = runtime.focused_block_id()?;
    let range = runtime.input_session_selected_range()?;
    if range.is_empty() {
        return None;
    }
    let payload = runtime.block_payload_record(block_id)?;
    let spans = toolbar_spans_for_payload(&payload.payload)?;
    let layout = text_layouts.get(&block_id)?;
    if runtime.block_content_version(block_id)? != layout.content_version {
        return None;
    }
    let bounds = platform_range_bounds(layout, range.clone())?;
    let (x, y) = floating_toolbar_position(
        f32::from(bounds.left()),
        f32::from(bounds.top()),
        f32::from(bounds.right()),
        f32::from(bounds.bottom()),
        f32::from(viewport.width),
        f32::from(viewport.height),
    );
    Some(FloatingToolbarState {
        x,
        y,
        block_id: Some(block_id),
        has_text_selection: true,
        show_delete: false,
        bold: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Bold),
        italic: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Italic),
        underline: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Underline),
        strike: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Strike),
        code: selected_spans_have_mark(spans, range, InlineFormatAction::Code),
    })
}

fn toolbar_spans_for_payload(payload: &BlockPayload) -> Option<&[InlineSpan]> {
    match payload {
        BlockPayload::RichText { spans } => Some(spans),
        _ => None,
    }
}

fn selected_spans_have_mark(
    spans: &[InlineSpan],
    range: Range<usize>,
    action: InlineFormatAction,
) -> bool {
    let mut offset = 0usize;
    let mut saw_selected_text = false;
    for span in spans {
        let span_range = offset..offset + span.text.len();
        offset = span_range.end;
        if span_range.start >= range.end || span_range.end <= range.start {
            continue;
        }
        saw_selected_text = true;
        if !span
            .marks
            .iter()
            .any(|mark| mark_matches_action(mark, action))
        {
            return false;
        }
    }
    saw_selected_text
}

fn mark_matches_action(mark: &InlineMark, action: InlineFormatAction) -> bool {
    matches!(
        (mark, action),
        (InlineMark::Bold, InlineFormatAction::Bold)
            | (InlineMark::Italic, InlineFormatAction::Italic)
            | (InlineMark::Underline, InlineFormatAction::Underline)
            | (InlineMark::Strike, InlineFormatAction::Strike)
            | (InlineMark::Code, InlineFormatAction::Code)
    )
}

impl CditorV2View {
    pub(crate) fn apply_inline_format_from_toolbar(
        &mut self,
        action: InlineFormatAction,
        has_text_selection: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let mark = match action {
            InlineFormatAction::Bold => InlineMark::Bold,
            InlineFormatAction::Italic => InlineMark::Italic,
            InlineFormatAction::Underline => InlineMark::Underline,
            InlineFormatAction::Strike => InlineMark::Strike,
            InlineFormatAction::Code => InlineMark::Code,
        };
        let gutter_block_id = (!has_text_selection).then_some(self.gutter_toolbar_block_id);
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                let Some(block_id) = gutter_block_id.flatten() else {
                    return runtime.toggle_inline_mark_on_selection(mark).ok();
                };
                if action == InlineFormatAction::Code {
                    return runtime
                        .convert_focused_block_kind(RichBlockKind::Code { language: None })
                        .ok();
                }
                let text_len = runtime
                    .block_payload_record(block_id)
                    .map(|payload| payload.plain_text().len())?;
                if text_len == 0 {
                    return Some(false);
                }
                runtime
                    .set_document_text_selection(block_id, 0, block_id, text_len)
                    .ok()?;
                runtime.toggle_inline_mark_on_selection(mark).ok()
            })
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{
        EmbedPayload, FilePayload, ImagePayload, TablePayload, WhiteboardPayload,
    };

    #[test]
    fn toolbar_requires_a_rich_text_payload() {
        let rich_text = BlockPayload::RichText {
            spans: vec![InlineSpan::plain("text")],
        };
        assert!(toolbar_spans_for_payload(&rich_text).is_some());

        let unsupported = [
            BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
            BlockPayload::Image(ImagePayload::default()),
            BlockPayload::Table(TablePayload::default()),
            BlockPayload::File(FilePayload::default()),
            BlockPayload::Whiteboard(WhiteboardPayload::default()),
            BlockPayload::Embed(EmbedPayload::default()),
            BlockPayload::Html {
                html: "<p>text</p>".to_owned(),
                sanitized: true,
            },
            BlockPayload::Empty,
        ];
        for payload in &unsupported {
            assert!(toolbar_spans_for_payload(payload).is_none());
        }
    }

    #[test]
    fn selected_mark_state_requires_every_overlapping_span() {
        let spans = vec![
            InlineSpan {
                text: "ab".to_owned(),
                marks: vec![InlineMark::Bold],
            },
            InlineSpan {
                text: "cd".to_owned(),
                marks: vec![InlineMark::Bold, InlineMark::Italic],
            },
            InlineSpan::plain("ef"),
        ];

        assert!(selected_spans_have_mark(
            &spans,
            0..4,
            InlineFormatAction::Bold
        ));
        assert!(!selected_spans_have_mark(
            &spans,
            0..4,
            InlineFormatAction::Italic
        ));
        assert!(selected_spans_have_mark(
            &spans,
            2..4,
            InlineFormatAction::Italic
        ));
        assert!(!selected_spans_have_mark(
            &spans,
            3..6,
            InlineFormatAction::Bold
        ));
    }
}
