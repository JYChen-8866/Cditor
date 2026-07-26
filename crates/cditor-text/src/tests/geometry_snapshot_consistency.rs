use cditor_core::{edit::TextAffinity, rich_text::InlineSpan};
use parley::{Affinity, Cursor, Selection};

use super::*;

const EPSILON: f32 = 0.001;

#[test]
fn materialized_caret_stops_match_parley_for_every_utf8_boundary() {
    for (text, width) in corpus() {
        let layout = build(text, width);
        assert_eq!(
            layout.geometry().caret_stops().len(),
            utf8_boundaries(text).len() * 2
        );
        for offset in utf8_boundaries(text) {
            for affinity in [TextAffinity::Upstream, TextAffinity::Downstream] {
                let actual = layout.caret_rect(TextLayoutPosition { offset, affinity }, 0.0);
                let cursor = Cursor::from_byte_index(
                    layout.layout(),
                    offset,
                    match affinity {
                        TextAffinity::Upstream => Affinity::Upstream,
                        TextAffinity::Downstream => Affinity::Downstream,
                    },
                );
                let expected = rect(
                    cursor.geometry(layout.layout(), 0.0),
                    layout.display_scale(),
                );
                let stop = layout
                    .geometry()
                    .caret_stops()
                    .iter()
                    .find(|stop| stop.requested == TextLayoutPosition { offset, affinity })
                    .expect("every UTF-8 boundary and affinity must be materialized");
                assert_eq!(stop.resolved.offset, cursor.index());
                assert_eq!(stop.resolved.affinity, text_affinity(cursor.affinity()));
                assert_rect_eq(actual, expected, text, offset);
            }
        }
    }
}

#[test]
fn materialized_point_hit_matches_parley_across_lines_and_bidi_runs() {
    for (text, width) in corpus() {
        let layout = build(text, width);
        for line in layout.line_snapshots() {
            for step in 0..=24 {
                let x = -12.0 + (layout.full_width() + 24.0) * step as f32 / 24.0;
                let y = (line.top + line.bottom) * 0.5;
                let actual = layout.position_for_point(x, y);
                let expected = Cursor::from_point(
                    layout.layout(),
                    x * layout.display_scale(),
                    y * layout.display_scale(),
                );
                assert_eq!(actual.offset, expected.index(), "text={text:?} x={x} y={y}");
                assert_eq!(
                    actual.affinity,
                    text_affinity(expected.affinity()),
                    "text={text:?} x={x} y={y}"
                );
            }
        }
    }
}

#[test]
fn materialized_selection_rects_match_parley_visual_geometry() {
    for (text, width) in corpus() {
        let layout = build(text, width);
        let boundaries = utf8_boundaries(text);
        for (start_index, start) in boundaries.iter().enumerate() {
            for end in boundaries.iter().skip(start_index + 1) {
                let selection = TextLayoutSelection {
                    anchor: TextLayoutPosition::downstream(*start),
                    focus: TextLayoutPosition {
                        offset: *end,
                        affinity: TextAffinity::Upstream,
                    },
                };
                let actual = layout.selection_rects(selection);
                let expected = Selection::new(
                    Cursor::from_byte_index(layout.layout(), *start, Affinity::Downstream),
                    Cursor::from_byte_index(layout.layout(), *end, Affinity::Upstream),
                )
                .geometry(layout.layout())
                .into_iter()
                .map(|(bounds, _)| rect(bounds, layout.display_scale()))
                .collect::<Vec<_>>();
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "text={text:?} range={start}..{end}"
                );
                for (actual, expected) in actual.into_iter().zip(expected) {
                    assert_rect_eq(actual, expected, text, *start);
                }
            }
        }
    }
}

#[test]
fn selection_preserves_the_second_line_at_an_explicit_break() {
    let text = "Cditor\n阿赛";
    let layout = build(text, 718.0);
    let second_line_start = "Cditor\n".len();

    assert_eq!(layout.line_count(), 2);
    let second_line = layout.range_rects(second_line_start..text.len());
    assert_eq!(second_line.len(), 1);
    assert!(second_line[0].y > 0.0);

    let whole_title = layout.range_rects(0..text.len());
    assert_eq!(whole_title.len(), 2);
    assert!(whole_title[1].y > whole_title[0].y);
}

#[test]
fn materialized_geometry_preserves_in_flow_inline_box_offsets() {
    let text = "before after";
    let width = 500.0;
    let layout = build_text_layout(
        &input(vec![InlineSpan::plain(text)], width),
        theme(),
        &TextLayoutOptions {
            inline_boxes: vec![InlineBoxSpec {
                id: 7,
                kind: InlineBoxKind::InFlow,
                index: "before".len(),
                width: 48.0,
                height: 18.0,
            }],
            ..options(width)
        },
    );

    for offset in utf8_boundaries(text) {
        let actual = layout.caret_rect(TextLayoutPosition::downstream(offset), 0.0);
        let expected = Cursor::from_byte_index(layout.layout(), offset, Affinity::Downstream);
        assert_rect_eq(
            actual,
            rect(
                expected.geometry(layout.layout(), 0.0),
                layout.display_scale(),
            ),
            text,
            offset,
        );
    }
}

fn corpus() -> [(&'static str, f32); 5] {
    [
        ("", 100.0),
        ("alpha beta gamma delta", 75.0),
        ("第一行中文和 emoji 👩‍💻\nsecond line", 120.0),
        ("abc אבג 123 مرحبا xyz", 130.0),
        ("e\u{301} 👨‍👩‍👧‍👦 end", 90.0),
    ]
}

fn build(text: &str, width: f32) -> TextLayoutSnapshot {
    build_text_layout(
        &input(vec![InlineSpan::plain(text)], width),
        theme(),
        &options(width),
    )
}

fn utf8_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    if boundaries.last().copied() != Some(text.len()) {
        boundaries.push(text.len());
    }
    boundaries
}

fn text_affinity(value: Affinity) -> TextAffinity {
    match value {
        Affinity::Upstream => TextAffinity::Upstream,
        Affinity::Downstream => TextAffinity::Downstream,
    }
}

fn rect(bounds: parley::BoundingBox, scale: f32) -> TextLayoutRect {
    TextLayoutRect {
        x: bounds.x0 as f32 / scale,
        y: bounds.y0 as f32 / scale,
        width: (bounds.x1 - bounds.x0) as f32 / scale,
        height: (bounds.y1 - bounds.y0) as f32 / scale,
    }
}

fn assert_rect_eq(actual: TextLayoutRect, expected: TextLayoutRect, text: &str, offset: usize) {
    assert!(
        (actual.x - expected.x).abs() <= EPSILON
            && (actual.y - expected.y).abs() <= EPSILON
            && (actual.width - expected.width).abs() <= EPSILON
            && (actual.height - expected.height).abs() <= EPSILON,
        "text={text:?} offset={offset} actual={actual:?} expected={expected:?}"
    );
}
