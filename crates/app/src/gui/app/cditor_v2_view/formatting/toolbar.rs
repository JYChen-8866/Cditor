use std::{collections::HashMap, ops::Range};

use gpui::{Bounds, Pixels, Size, point, px};

use crate::gui::app::interaction::geometry::ProjectedBlockRect;
use crate::gui::block::chrome::block_content_left_px;
use crate::gui::document::{DEFAULT_DOCUMENT_PAGE_WIDTH_PX, DEFAULT_DOCUMENT_TOP_INSET_PX};
use crate::gui::overlay::{
    ActiveColor, BlockTransformAction, BlockTransformAvailability, ColorMenuAction,
    FloatingToolbarState, InlineFormatAction, PaletteColor, block_transform_menu_opens_left,
    block_transform_menu_top_offset, color_menu_geometry, floating_toolbar_position,
    left_aligned_floating_toolbar_position,
};
use crate::gui::text::{RichTextPlatformLayout, platform_range_bounds};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, InlineColorTarget, InlineMark, InlineSpan};
use cditor_runtime::DocumentRuntime;

#[cfg(test)]
use super::actions::inline_mark_for_toolbar_action;
use super::color::selected_spans_color;

pub(in crate::gui::app) fn formatting_toolbar_state(
    runtime: Option<&DocumentRuntime>,
    text_layouts: &HashMap<cditor_core::ids::BlockId, RichTextPlatformLayout>,
    readonly: bool,
    conflicting_overlay_open: bool,
    viewport: Size<Pixels>,
    gutter_toolbar_block_id: Option<BlockId>,
    block_transform_menu_open: bool,
    color_menu_open: bool,
    last_color_action: Option<ColorMenuAction>,
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
        let top = (rect.document_top - scroll_top) as f32 + DEFAULT_DOCUMENT_TOP_INSET_PX;
        let bottom = (rect.document_bottom - scroll_top) as f32 + DEFAULT_DOCUMENT_TOP_INSET_PX;
        let block_left = page_left + block_content_left_px(rect.indent_px);
        let (x, y) = left_aligned_floating_toolbar_position(
            block_left,
            top,
            bottom,
            f32::from(viewport.width),
            f32::from(viewport.height),
        );
        let color_geometry =
            color_menu_geometry(x, y, f32::from(viewport.width), f32::from(viewport.height));
        let (bold, italic, underline, strike, code, text_color, background_color, block_transform) =
            runtime
                .block_payload_record(block_id)
                .map(|payload| {
                    let rich_spans = toolbar_spans_for_payload(&payload.payload);
                    let marks = rich_spans
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
                                selected_spans_have_mark(
                                    spans,
                                    range.clone(),
                                    InlineFormatAction::Strike,
                                ),
                                selected_spans_have_mark(
                                    spans,
                                    0..payload.plain_text().len(),
                                    InlineFormatAction::Code,
                                ),
                                active_block_color(
                                    payload.block_id,
                                    runtime,
                                    InlineColorTarget::Text,
                                ),
                                active_block_color(
                                    payload.block_id,
                                    runtime,
                                    InlineColorTarget::Background,
                                ),
                            )
                        })
                        .unwrap_or((
                            false,
                            false,
                            false,
                            false,
                            false,
                            ActiveColor::Default,
                            ActiveColor::Default,
                        ));
                    (
                        marks.0,
                        marks.1,
                        marks.2,
                        marks.3,
                        marks.4,
                        marks.5,
                        marks.6,
                        BlockTransformAction::from_kind(&payload.kind),
                    )
                })
                .unwrap_or((
                    false,
                    false,
                    false,
                    false,
                    false,
                    ActiveColor::Default,
                    ActiveColor::Default,
                    None,
                ));
        let block_transform_availability = BlockTransformAvailability::from_enabled(
            BlockTransformAction::ALL.into_iter().filter(|action| {
                block_transform == Some(*action)
                    || runtime.can_convert_block_kind(block_id, &action.kind())
            }),
        );
        let rich_text_actions_enabled = runtime.supports_block_rich_text_actions(block_id);
        return Some(FloatingToolbarState {
            x,
            y,
            block_id: Some(block_id),
            has_text_selection: false,
            show_inline_format: true,
            show_color: true,
            show_delete: true,
            inline_format_enabled: rich_text_actions_enabled,
            color_enabled: rich_text_actions_enabled,
            ai_enabled: runtime.can_begin_ai_request(),
            delete_enabled: runtime.can_delete_block(block_id),
            bold,
            italic,
            underline,
            strike,
            code,
            block_transform,
            block_transform_availability,
            transform_menu_opens_left: block_transform_menu_opens_left(
                x,
                f32::from(viewport.width),
            ),
            transform_menu_top_offset: block_transform_menu_top_offset(
                y,
                f32::from(viewport.height),
            ),
            block_transform_menu_open,
            text_color,
            background_color,
            color_menu_opens_left: color_geometry.opens_left,
            color_menu_top_offset: color_geometry.top_offset,
            color_menu_height: color_geometry.height,
            color_menu_open,
            last_color_action,
        });
    }
    if runtime.focused_table_cell_offset().is_some() {
        return None;
    }
    if runtime.has_cross_block_text_selection() {
        let fragments = runtime.document_text_selection_fragments()?;
        let bounds = cross_block_selection_bounds(runtime, text_layouts, &fragments)?;
        let (x, y) = floating_toolbar_position(
            f32::from(bounds.left()),
            f32::from(bounds.top()),
            f32::from(bounds.right()),
            f32::from(bounds.bottom()),
            f32::from(viewport.width),
            f32::from(viewport.height),
        );
        return Some(FloatingToolbarState {
            x,
            y,
            block_id: runtime.focused_block_id(),
            has_text_selection: true,
            show_inline_format: true,
            show_color: true,
            show_delete: false,
            inline_format_enabled: false,
            color_enabled: false,
            ai_enabled: runtime.can_begin_ai_request(),
            delete_enabled: false,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
            block_transform: None,
            block_transform_availability: BlockTransformAvailability::default(),
            transform_menu_opens_left: false,
            transform_menu_top_offset: 0.0,
            block_transform_menu_open: false,
            text_color: ActiveColor::Mixed,
            background_color: ActiveColor::Mixed,
            color_menu_opens_left: false,
            color_menu_top_offset: 0.0,
            color_menu_height: 0.0,
            color_menu_open: false,
            last_color_action,
        });
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
    let color_geometry =
        color_menu_geometry(x, y, f32::from(viewport.width), f32::from(viewport.height));
    Some(FloatingToolbarState {
        x,
        y,
        block_id: Some(block_id),
        has_text_selection: true,
        show_inline_format: true,
        show_color: true,
        show_delete: false,
        inline_format_enabled: true,
        color_enabled: true,
        ai_enabled: runtime.can_begin_ai_request(),
        delete_enabled: false,
        bold: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Bold),
        italic: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Italic),
        underline: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Underline),
        strike: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Strike),
        code: selected_spans_have_mark(spans, range.clone(), InlineFormatAction::Code),
        block_transform: None,
        block_transform_availability: BlockTransformAvailability::default(),
        transform_menu_opens_left: false,
        transform_menu_top_offset: 0.0,
        block_transform_menu_open: false,
        text_color: selected_spans_color(spans, range.clone(), InlineColorTarget::Text),
        background_color: selected_spans_color(spans, range, InlineColorTarget::Background),
        color_menu_opens_left: color_geometry.opens_left,
        color_menu_top_offset: color_geometry.top_offset,
        color_menu_height: color_geometry.height,
        color_menu_open,
        last_color_action,
    })
}

fn cross_block_selection_bounds(
    runtime: &DocumentRuntime,
    text_layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    fragments: &[cditor_runtime::DocumentTextSelectionFragment],
) -> Option<Bounds<Pixels>> {
    let mut fragment_bounds = fragments.iter().filter_map(|fragment| {
        let layout = text_layouts.get(&fragment.block_id)?;
        (runtime.block_content_version(fragment.block_id)? == layout.content_version)
            .then(|| platform_range_bounds(layout, fragment.range.clone()))
            .flatten()
    });
    let first = fragment_bounds.next()?;
    Some(fragment_bounds.fold(first, |combined, bounds| {
        Bounds::from_corners(
            point(
                px(f32::from(combined.left()).min(f32::from(bounds.left()))),
                px(f32::from(combined.top()).min(f32::from(bounds.top()))),
            ),
            point(
                px(f32::from(combined.right()).max(f32::from(bounds.right()))),
                px(f32::from(combined.bottom()).max(f32::from(bounds.bottom()))),
            ),
        )
    }))
}

fn toolbar_spans_for_payload(payload: &BlockPayload) -> Option<&[InlineSpan]> {
    match payload {
        BlockPayload::RichText { spans } => Some(spans),
        _ => None,
    }
}

fn active_block_color(
    block_id: BlockId,
    runtime: &DocumentRuntime,
    target: InlineColorTarget,
) -> ActiveColor {
    let attrs = runtime.block_attrs(block_id);
    let value = match target {
        InlineColorTarget::Text => attrs.color.as_deref(),
        InlineColorTarget::Background => attrs.background_color.as_deref(),
    };
    match value {
        None => ActiveColor::Default,
        Some(value) => PaletteColor::from_value(target, value)
            .map(ActiveColor::Palette)
            .unwrap_or(ActiveColor::Mixed),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::overlay::PaletteColor;
    use cditor_core::rich_text::{
        EmbedPayload, FilePayload, ImagePayload, RichBlockKind, TablePayload, WhiteboardPayload,
    };
    use gpui::{point, size};

    #[test]
    fn cross_block_text_selection_keeps_unsupported_actions_visible_but_disabled() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                cditor_core::rich_text::BlockPayloadRecord::rich_text(
                    1,
                    RichBlockKind::Paragraph,
                    "first",
                ),
                cditor_core::rich_text::BlockPayloadRecord::rich_text(
                    2,
                    RichBlockKind::Paragraph,
                    "second",
                ),
            ],
            720.0,
        );
        runtime.set_document_text_selection(1, 1, 2, 2).unwrap();
        let mut layouts = HashMap::new();
        for (block_id, top) in [(1, 100.0), (2, 124.0)] {
            layouts.insert(
                block_id,
                RichTextPlatformLayout {
                    block_id,
                    content_version: runtime.block_content_version(block_id).unwrap(),
                    text: String::new(),
                    lines: Vec::new(),
                    bounds: Bounds::new(point(px(120.0), px(top)), size(px(500.0), px(24.0))),
                    line_height: px(24.0),
                    text_align: gpui::TextAlign::Left,
                    measured_height: 24.0,
                    table_cell_position: None,
                },
            );
        }

        let state = formatting_toolbar_state(
            Some(&runtime),
            &layouts,
            false,
            false,
            size(px(900.0), px(700.0)),
            None,
            false,
            false,
            None,
            &[],
            0.0,
        )
        .expect("cross-block selection should show a toolbar");

        assert!(state.has_text_selection);
        assert!(state.show_inline_format);
        assert!(state.show_color);
        assert!(!state.inline_format_enabled);
        assert!(!state.color_enabled);
        assert!(state.ai_enabled);
        assert!(!state.show_delete);
        assert_eq!(state.block_id, Some(2));
    }

    #[test]
    fn gutter_toolbar_keeps_the_stateful_transform_menu_open_for_clicks() {
        let runtime = DocumentRuntime::demo();
        let rect = ProjectedBlockRect {
            block_id: 1,
            visible_index: 0,
            depth: 0,
            document_top: 0.0,
            document_bottom: 48.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 500.0,
            supports_children: false,
        };

        let state = formatting_toolbar_state(
            Some(&runtime),
            &HashMap::new(),
            false,
            false,
            size(px(900.0), px(700.0)),
            Some(1),
            true,
            false,
            None,
            &[rect],
            0.0,
        )
        .unwrap();

        assert!(state.show_delete);
        assert!(state.show_color);
        assert!(state.inline_format_enabled);
        assert!(state.color_enabled);
        assert!(state.block_transform_menu_open);
    }

    #[test]
    fn gutter_toolbar_color_state_comes_from_block_attrs_not_inline_spans() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                2,
                RichBlockKind::Paragraph,
                "text",
            )],
            720.0,
        );
        runtime
            .set_inline_color_for_range(2, 0..4, InlineColorTarget::Text, Some("#337ea9"))
            .unwrap();
        runtime
            .set_block_color(2, InlineColorTarget::Text, Some("#d44c47"))
            .unwrap();
        runtime
            .set_block_color(2, InlineColorTarget::Background, Some("#fdebec"))
            .unwrap();

        assert_eq!(
            active_block_color(2, &runtime, InlineColorTarget::Text),
            ActiveColor::Palette(PaletteColor::Red)
        );
        assert_eq!(
            active_block_color(2, &runtime, InlineColorTarget::Background),
            ActiveColor::Palette(PaletteColor::Red)
        );
    }

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

    #[test]
    fn selected_color_state_distinguishes_default_uniform_and_mixed_ranges() {
        let spans = vec![
            InlineSpan {
                text: "ab".to_owned(),
                marks: vec![
                    InlineMark::Color("#337ea9".to_owned()),
                    InlineMark::Background("#fbf3db".to_owned()),
                ],
            },
            InlineSpan {
                text: "cd".to_owned(),
                marks: vec![InlineMark::Color("#337ea9".to_owned())],
            },
            InlineSpan::plain("ef"),
        ];

        assert_eq!(
            selected_spans_color(&spans, 0..4, InlineColorTarget::Text),
            ActiveColor::Palette(PaletteColor::Blue)
        );
        assert_eq!(
            selected_spans_color(&spans, 0..4, InlineColorTarget::Background),
            ActiveColor::Mixed
        );
        assert_eq!(
            selected_spans_color(&spans, 4..6, InlineColorTarget::Text),
            ActiveColor::Default
        );
        assert_eq!(
            selected_spans_color(&spans, 2..6, InlineColorTarget::Text),
            ActiveColor::Mixed
        );
    }

    #[test]
    fn custom_or_invalid_color_marks_are_reported_as_mixed() {
        let spans = vec![InlineSpan {
            text: "text".to_owned(),
            marks: vec![InlineMark::Color("#123456".to_owned())],
        }];
        assert_eq!(
            selected_spans_color(&spans, 0..4, InlineColorTarget::Text),
            ActiveColor::Mixed
        );
    }

    #[test]
    fn gutter_code_action_maps_to_inline_code_mark_not_code_block() {
        assert_eq!(
            inline_mark_for_toolbar_action(InlineFormatAction::Code),
            InlineMark::Code
        );
        assert_eq!(
            BlockTransformAction::CodeBlock.kind(),
            RichBlockKind::Code { language: None }
        );
    }
}

#[cfg(test)]
#[path = "toolbar_capability_tests.rs"]
mod capability_tests;
