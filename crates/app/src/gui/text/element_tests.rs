use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind};

use super::*;

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
    assert_eq!(rect.height, 22.0);
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
    assert_eq!(rect.height, 22.0);
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
