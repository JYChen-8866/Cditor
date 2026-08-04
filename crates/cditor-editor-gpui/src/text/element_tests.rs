use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind};

use super::background::text_selection_background;
use super::element::{
    base_font_weight_for_kind, is_completed_todo, line_height_for_kind, text_color_for_kind,
    text_size_for_kind,
};
use super::*;
use crate::theme::GuiTheme;
use gpui::{FontWeight, px};
use std::sync::Arc;

#[test]
fn text_selection_uses_translucent_accent_so_applied_background_remains_visible() {
    let theme = GuiTheme::light();
    assert_eq!(
        text_selection_background(theme),
        (theme.focused << 8) | 0x26
    );
}

#[test]
fn rich_text_element_paints_spans() {
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![
            InlineSpan::plain("hello "),
            InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            },
        ]
        .into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };

    let element = RichTextElement::new(input.clone(), GuiTheme::light())
        .with_caret(Some(6))
        .with_marked_range(Some(0..5));
    let _paintable = element.render();
}

#[test]
fn auxiliary_text_shapes_on_first_frame_unless_viewport_prewarm_is_explicit() {
    let input = RichTextLayoutInput {
        block_id: 2,
        surface_id: TextLayoutSurfaceId::TableCell {
            block_id: 2,
            row: 0,
            column: 0,
        },
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("first frame")].into(),
        width_px: 200.0,
        theme_version: 1,
        font_version: 1,
    };

    let element = RichTextElement::new(input, GuiTheme::light());
    assert!(!element.require_prewarmed_layout);
    assert!(element.with_prewarmed_layout().require_prewarmed_layout);
}

#[test]
fn rich_text_element_candidate_rect_tracks_caret_geometry() {
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("abcd")].into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input, GuiTheme::light()).with_caret(Some(2));

    let rect = element.candidate_rect_for_caret().unwrap();

    assert!(rect.x > 0.0);
    assert_eq!(rect.y, 0.0);
    assert_eq!(rect.height, 24.0);
}

#[test]
fn empty_rich_text_element_keeps_visible_nonzero_caret_geometry() {
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("")].into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input, GuiTheme::light()).with_caret(Some(0));

    let rect = element.candidate_rect_for_caret().unwrap();

    assert_eq!(rect.x, 0.0);
    assert_eq!(rect.y, 0.0);
    assert!(rect.height > 0.0);
}

#[test]
fn rich_text_element_candidate_rect_tracks_multiline_table_cell_caret() {
    let text = "first\nsecond\nthird";
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::TableCell {
            block_id: 1,
            row: 0,
            column: 0,
        },
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain(text)].into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input, GuiTheme::light()).with_caret(Some(text.len()));

    let rect = element.candidate_rect_for_caret().unwrap();

    assert!(
        rect.y >= 48.0,
        "candidate rect should move to the third row: {rect:?}"
    );
    assert_eq!(rect.height, 24.0);
}

#[test]
fn rich_text_element_hides_custom_caret_while_ime_marked_range_is_active() {
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("ab中cd")].into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input.clone(), GuiTheme::light())
        .with_caret(Some("ab中".len()))
        .with_marked_range(Some(2.."ab中".len()));
    assert!(element.marked_range.is_some());
    let _paintable = element.render();
}

#[test]
fn rich_text_element_hit_test() {
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("abcd")].into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input, GuiTheme::light());

    assert_eq!(element.hit_test(TextHitPoint { x: 0.0, y: 0.0 }), 0);
    assert_eq!(element.hit_test(TextHitPoint { x: 1_000.0, y: 0.0 }), 4);
}

#[test]
fn inline_box_configuration_reaches_the_shared_text_layout_layout() {
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("ab")].into(),
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input, GuiTheme::light()).with_inline_boxes(
        vec![InlineBoxSpec {
            id: 42,
            kind: InlineBoxKind::InFlow,
            index: 1,
            width: 48.0,
            height: 18.0,
        }],
        Arc::new(|_, _, _, _| {}),
    );

    let boxes = element.positioned_inline_boxes();

    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].id, 42);
    assert_eq!(boxes[0].kind, InlineBoxKind::InFlow);
    assert_eq!(boxes[0].width, 48.0);
    assert_eq!(boxes[0].height, 18.0);
}

#[test]
fn document_link_text_soft_wraps_inside_narrow_viewports_without_inline_icons() {
    let width = 120.0_f32;
    let document_link = |text: &str, href: &str| InlineSpan {
        text: text.to_owned(),
        marks: vec![InlineMark::DocumentLink {
            href: href.to_owned(),
        }],
    };
    let cases = [
        vec![
            InlineSpan::plain("Before "),
            document_link(
                "A long document title with spaces 以及需要自动换行的中文标题",
                "aurin://doc/1",
            ),
            InlineSpan::plain(" after"),
        ],
        vec![document_link(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "aurin://doc/2",
        )],
        vec![
            InlineSpan::plain("Text that nearly fills "),
            document_link("First", "aurin://doc/3"),
            InlineSpan::plain(" "),
            document_link("SecondLongDocumentTitle", "aurin://doc/4"),
        ],
    ];

    for (case_index, spans) in cases.into_iter().enumerate() {
        let block_id = 10 + case_index as u64;
        let input = RichTextLayoutInput {
            block_id,
            surface_id: TextLayoutSurfaceId::Block(block_id),
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            text_align: cditor_core::rich_text::TextAlign::Start,
            spans: spans.into(),
            width_px: f64::from(width),
            theme_version: 1,
            font_version: 1,
        };
        let layout = RichTextElement::new(input, GuiTheme::light()).layout_snapshot();

        assert!(layout.inline_boxes().is_empty());
        assert!(layout.line_count() > 1);
        for line in layout.line_snapshots() {
            assert!(line.text_range.start <= line.text_range.end);
            assert!(line.text_range.end <= layout.text().len());
            let line_text = &layout.text()[line.text_range.clone()];
            if line_text.chars().last().is_some_and(char::is_whitespace) {
                continue;
            }
            assert!(
                line.logical_right <= width + 0.5,
                "line escaped viewport: case={case_index}, width={width}, line={line:?}"
            );
        }
    }
}

#[test]
fn notion_text_sizes_and_line_heights_are_stable() {
    assert_eq!(text_size_for_kind(&RichBlockKind::Paragraph), px(16.0));
    assert_eq!(
        line_height_for_kind(&RichBlockKind::Paragraph, px(16.0)),
        px(24.0)
    );
    assert_eq!(
        text_size_for_kind(&RichBlockKind::Heading { level: 1 }),
        px(30.0)
    );
    assert_eq!(
        line_height_for_kind(&RichBlockKind::Heading { level: 1 }, px(30.0)),
        px(39.0)
    );
    assert_eq!(
        line_height_for_kind(&RichBlockKind::Heading { level: 2 }, px(24.0)),
        px(32.0)
    );
    assert_eq!(
        line_height_for_kind(&RichBlockKind::Heading { level: 3 }, px(20.0)),
        px(26.0)
    );
    assert_eq!(
        base_font_weight_for_kind(&RichBlockKind::Heading { level: 1 }, FontWeight::NORMAL),
        FontWeight::SEMIBOLD
    );
    assert_eq!(
        base_font_weight_for_kind(&RichBlockKind::Heading { level: 1 }, FontWeight::BOLD),
        FontWeight::BOLD
    );
    assert_eq!(
        text_size_for_kind(&RichBlockKind::FootnoteDefinition),
        px(14.0)
    );
    assert_eq!(
        line_height_for_kind(&RichBlockKind::FootnoteDefinition, px(14.0)),
        px(20.0)
    );
}

#[test]
fn completed_todo_uses_muted_struck_text_style() {
    let theme = GuiTheme::light();

    assert!(is_completed_todo(&RichBlockKind::Todo { checked: true }));
    assert!(!is_completed_todo(&RichBlockKind::Todo { checked: false }));
    assert_eq!(
        text_color_for_kind(&RichBlockKind::Todo { checked: true }, theme),
        theme.muted,
    );
}
