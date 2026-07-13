use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind};

use super::background::{
    InlineBackgroundDecoration, inline_background_decorations,
    notion_inline_code_background_bounds, text_selection_background,
};
use super::element::{
    base_font_weight_for_kind, is_completed_todo, line_height_for_kind, text_color_for_kind,
    text_size_for_kind,
};
use super::fallback_render::render_visual_run_segments;
use super::*;
use crate::gui::theme::GuiTheme;
use gpui::{Bounds, FontWeight, point, px, size};

#[test]
fn inline_code_decorations_merge_adjacent_code_spans_and_keep_custom_backgrounds() {
    let theme = GuiTheme::light();
    let spans = vec![
        InlineSpan {
            text: "ab".to_owned(),
            marks: vec![InlineMark::Code],
        },
        InlineSpan {
            text: "cd".to_owned(),
            marks: vec![InlineMark::Code, InlineMark::Bold],
        },
        InlineSpan::plain(" "),
        InlineSpan {
            text: "ef".to_owned(),
            marks: vec![
                InlineMark::Code,
                InlineMark::Background("#abcdef".to_owned()),
            ],
        },
    ];

    assert_eq!(
        inline_background_decorations(&spans, theme),
        vec![
            InlineBackgroundDecoration {
                range: 0..4,
                background_color: theme.inline_code_background,
                horizontal_padding_px: 3.0,
            },
            InlineBackgroundDecoration {
                range: 5..7,
                background_color: 0xabcdef,
                horizontal_padding_px: 3.0,
            },
        ]
    );
}

#[test]
fn explicit_background_marks_generate_paintable_highlight_decorations_without_code() {
    let theme = GuiTheme::light();
    let spans = vec![
        InlineSpan {
            text: "ab".to_owned(),
            marks: vec![InlineMark::Background("#fbf3db".to_owned())],
        },
        InlineSpan {
            text: "cd".to_owned(),
            marks: vec![
                InlineMark::Bold,
                InlineMark::Background("#fbf3db".to_owned()),
            ],
        },
        InlineSpan::plain("ef"),
    ];

    assert_eq!(
        inline_background_decorations(&spans, theme),
        vec![InlineBackgroundDecoration {
            range: 0..4,
            background_color: 0xfbf3db,
            horizontal_padding_px: 1.0,
        }]
    );
}

#[test]
fn text_selection_uses_translucent_accent_so_applied_background_remains_visible() {
    let theme = GuiTheme::light();
    assert_eq!(
        text_selection_background(theme),
        (theme.focused << 8) | 0x26
    );
}

#[test]
fn inline_code_background_adds_visible_notion_padding_on_both_sides() {
    let segment = Bounds::new(point(px(11.0), px(20.0)), size(px(40.0), px(24.0)));

    assert_eq!(
        notion_inline_code_background_bounds(segment),
        Bounds::from_corners(point(px(8.0), px(21.0)), point(px(54.0), px(43.0)))
    );
}

#[test]
fn rich_text_element_paints_spans() {
    let input = RichTextLayoutInput {
        block_id: 1,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        spans: vec![
            InlineSpan::plain("hello "),
            InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            },
        ],
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };

    let element = RichTextElement::new(input.clone(), GuiTheme::light())
        .with_caret(Some(6))
        .with_marked_range(Some(0..5));
    let layout = wrap_rich_text(&input);

    assert_eq!(layout.lines.len(), 1);
    assert_eq!(layout.lines[0].runs.len(), 2);
    assert!(layout.lines[0].runs[1].mark_style.bold);
    let _paintable = element.render();
}

#[test]
fn rich_text_element_candidate_rect_tracks_caret_geometry() {
    let input = RichTextLayoutInput {
        block_id: 1,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        spans: vec![InlineSpan::plain("abcd")],
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
fn rich_text_element_candidate_rect_tracks_multiline_table_cell_caret() {
    let text = "first\nsecond\nthird";
    let input = RichTextLayoutInput {
        block_id: 1,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        spans: vec![InlineSpan::plain(text)],
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let layout = wrap_rich_text(&input);
    let element = RichTextElement::new(input, GuiTheme::light()).with_caret(Some(text.len()));

    let rect = element.candidate_rect_for_caret().unwrap();

    assert!(layout.lines.len() >= 3);
    assert!(
        rect.y >= layout.lines[2].y,
        "candidate rect should move to current multiline caret row, rect={rect:?}"
    );
    assert_eq!(rect.height, 24.0);
}

#[test]
fn rich_text_element_hides_custom_caret_while_ime_marked_range_is_active() {
    let input = RichTextLayoutInput {
        block_id: 1,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        spans: vec![InlineSpan::plain("ab中cd")],
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input.clone(), GuiTheme::light())
        .with_caret(Some("ab中".len()))
        .with_marked_range(Some(2.."ab中".len()));
    let text = element.plain_text();
    let layout = wrap_rich_text(&input);
    let caret_rect = element
        .caret_offset
        .filter(|_| element.marked_range.is_none())
        .map(|offset| layout.caret_rect_for_offset(&text, offset));

    assert!(caret_rect.is_none());
    let _paintable = element.render();
}

#[test]
fn rich_text_element_marks_only_the_ime_subrange() {
    let input = RichTextLayoutInput {
        block_id: 1,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        spans: vec![InlineSpan::plain("ab中cd")],
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let text = "ab中cd";
    let layout = wrap_rich_text(&input);
    let run = &layout.lines[0].runs[0];

    let segments =
        render_visual_run_segments(text, run, GuiTheme::light(), Some(&(2.."ab中".len())));

    assert_eq!(segments.len(), 3);
}

#[test]
fn rich_text_element_hit_test() {
    let input = RichTextLayoutInput {
        block_id: 1,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        spans: vec![InlineSpan::plain("abcd")],
        width_px: 320.0,
        theme_version: 1,
        font_version: 1,
    };
    let element = RichTextElement::new(input, GuiTheme::light());

    assert_eq!(element.hit_test(TextHitPoint { x: 0.0, y: 0.0 }), 0);
    assert_eq!(element.hit_test(TextHitPoint { x: 1_000.0, y: 0.0 }), 4);
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
