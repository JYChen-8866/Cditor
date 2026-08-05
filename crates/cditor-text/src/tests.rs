use super::*;

mod fixture_corpus;
mod geometry_properties;
mod geometry_snapshot_consistency;
mod segmented_consistency;
mod visual_regression;
use crate::bundled_fonts::{DOCUMENT_BODY_FONT_FAMILY, DOCUMENT_BODY_FONT_REGULAR};
use cditor_core::{
    edit::TextAffinity,
    rich_text::{InlineMark, InlineSpan, RichBlockKind},
};

const BASE_TEXT_COLOR: u32 = 0x37352f;

fn theme() -> TextTheme {
    TextTheme {
        link_text: 0x2383e2,
        document_link_text: 0x9065b0,
        inline_code_text: 0xeb5757,
        inline_code_background: 0xf1f1ef,
    }
}

fn input(spans: Vec<InlineSpan>, width: f32) -> TextLayoutInput {
    TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(1),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans,
        width_px: width as f64,
        theme_version: 1,
        font_version: 1,
    }
}

fn options(width: f32) -> TextLayoutOptions {
    TextLayoutOptions {
        width: Some(width),
        base_text_color: BASE_TEXT_COLOR,
        base_style: TextStyleConfig {
            line_height: TextLineHeight::Absolute(24.0),
            ..TextStyleConfig::default()
        },
        ..TextLayoutOptions::default()
    }
}

#[test]
fn style_runs_cover_text_and_map_all_current_inline_marks() {
    let theme = theme();
    let spans = vec![
        InlineSpan::plain("plain"),
        InlineSpan {
            text: "rich".to_owned(),
            marks: vec![
                InlineMark::Bold,
                InlineMark::Italic,
                InlineMark::Underline,
                InlineMark::Strike,
                InlineMark::Code,
                InlineMark::Link {
                    href: "https://example.com".to_owned(),
                },
                InlineMark::Color("#123456".to_owned()),
                InlineMark::Background("#abcdef".to_owned()),
            ],
        },
    ];
    let base = TextStyleConfig::default();
    let runs = text_style_runs(
        &spans,
        &RichBlockKind::Paragraph,
        theme,
        BASE_TEXT_COLOR,
        &base,
        "Cditor Mono",
    );

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].range, 0..5);
    assert_eq!(runs[1].range, 5..9);
    assert_eq!(runs[1].style.font_family, "Cditor Mono");
    assert_eq!(runs[1].style.font_weight, 700.0);
    assert_eq!(runs[1].style.font_slant, TextFontSlant::Italic);
    assert!(runs[1].style.underline);
    assert!(runs[1].style.strikethrough);
    assert_eq!(runs[1].style.brush.foreground, 0x123456);
    assert_eq!(runs[1].style.brush.background, Some(0xabcdef));
    assert_eq!(runs[1].style.brush.background_padding_x, 3);
    assert_eq!(runs[1].style.brush.background_padding_y, 1);
    assert_eq!(runs[1].style.brush.background_radius, 3);
    assert!(runs[1].style.font_features.contains("'liga' 0"));
}

#[test]
fn external_and_document_links_use_distinct_theme_colors() {
    let theme = theme();
    let spans = vec![
        InlineSpan {
            text: "external".to_owned(),
            marks: vec![InlineMark::Link {
                href: "https://example.com".to_owned(),
            }],
        },
        InlineSpan {
            text: "block".to_owned(),
            marks: vec![InlineMark::DocumentLink {
                href: "cditor://document/1/block/2".to_owned(),
            }],
        },
    ];
    let runs = text_style_runs(
        &spans,
        &RichBlockKind::Paragraph,
        theme,
        BASE_TEXT_COLOR,
        &TextStyleConfig::default(),
        "Cditor Mono",
    );
    assert_eq!(runs[0].style.brush.foreground, theme.link_text);
    assert_eq!(runs[1].style.brush.foreground, theme.document_link_text);
    assert_ne!(
        runs[0].style.brush.foreground,
        runs[1].style.brush.foreground
    );
}

#[test]
fn parley_layout_shapes_cjk_combining_and_emoji_clusters() {
    let text = "A中e\u{301}👨‍👩‍👧‍👦Z";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));

    assert_eq!(layout.text(), text);
    assert_eq!(layout.line_count(), 1);
    let mut position = TextLayoutPosition::downstream(0);
    for _ in 0..16 {
        let next = layout
            .move_selection(
                TextLayoutSelection {
                    anchor: position,
                    focus: position,
                },
                TextLayoutMoveCommand::NextVisual,
                false,
            )
            .focus;
        assert!(text.is_char_boundary(next.offset));
        if next == position {
            break;
        }
        position = next;
    }
    assert_eq!(position.offset, text.len());
}

#[test]
fn bundled_document_font_shapes_latin_and_chinese_without_system_fallback() {
    let text = "Alibaba PuHuiTi 正文混排";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let mut options = options(500.0);
    options.base_style.font_family = DOCUMENT_BODY_FONT_FAMILY.to_owned();

    let layout = build_text_layout(&input, theme(), &options);
    let runs = &layout.paint_plan().runs;

    assert!(!runs.is_empty());
    assert!(
        runs.iter()
            .all(|run| run.font.family == DOCUMENT_BODY_FONT_FAMILY)
    );
    assert!(
        runs.iter()
            .all(|run| run.font.data() == DOCUMENT_BODY_FONT_REGULAR)
    );
}

#[test]
fn layout_snapshot_owns_unicode_offset_and_shaping_cluster_maps() {
    let text = "Ae\u{301}中 👨‍👩‍👧‍👦 אב";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));
    let family_start = text.find('👨').expect("fixture contains family emoji");
    let family_end = family_start + "👨‍👩‍👧‍👦".len();

    assert_eq!(layout.text_snapshot().as_str(), text);
    assert_eq!(
        layout
            .text_snapshot()
            .byte_range_to_utf16(family_start..family_end)
            .and_then(|range| layout.text_snapshot().utf16_range_to_byte(range)),
        Some(family_start..family_end)
    );
    assert!(!layout.clusters().is_empty());
    assert!(layout.clusters().iter().all(|cluster| {
        text.is_char_boundary(cluster.text_range.start)
            && text.is_char_boundary(cluster.text_range.end)
            && cluster.text_range.start < cluster.text_range.end
    }));

    let family_cluster = layout
        .cluster_for_byte_offset(family_start)
        .expect("emoji byte belongs to a shaping cluster");
    assert!(family_cluster.text_range.start >= family_start);
    assert!(family_cluster.text_range.end <= family_end);
    assert!(family_cluster.is_emoji);
    let family_grapheme = layout
        .text_snapshot()
        .byte_to_grapheme(family_start)
        .expect("emoji starts at a grapheme boundary");
    assert_eq!(
        layout.text_snapshot().grapheme_to_byte(family_grapheme + 1),
        Some(family_end)
    );
    assert!(
        layout
            .clusters()
            .iter()
            .filter(|cluster| {
                cluster.text_range.start >= family_start && cluster.text_range.end <= family_end
            })
            .count()
            > 1
    );
    assert!(layout.clusters().iter().any(|cluster| cluster.is_rtl));
}

#[test]
fn parley_layout_reflows_without_rebuilding_text_or_styles() {
    let text = "one two three four five six seven eight";
    let input = input(vec![InlineSpan::plain(text)], 90.0);
    let narrow = build_text_layout(&input, theme(), &options(90.0));
    let wide = narrow.reflow(Some(600.0), TextAlignment::Center);

    assert!(narrow.line_count() > wide.line_count());
    assert_eq!(wide.text(), narrow.text());
    assert_eq!(wide.alignment(), TextAlignment::Center);
    assert_eq!(wide.layout_width(), Some(600.0));
}

#[test]
fn parley_layout_cache_hits_and_reflows_only_for_width_changes() {
    super::cache::reset_text_layout_cache_for_tests();
    let input = input(vec![InlineSpan::plain("cache me across frames")], 90.0);
    let narrow_options = options(90.0);

    let first = cached_text_layout(&input, theme(), &narrow_options);
    let second = cached_text_layout(&input, theme(), &narrow_options);
    let wide = cached_text_layout(&input, theme(), &options(600.0));

    assert!(!first.cache_hit);
    assert!(!first.reflowed);
    assert!(second.cache_hit);
    assert!(!second.reflowed);
    assert!(!wide.cache_hit);
    assert!(wide.reflowed);
    assert_eq!(wide.layout.text(), first.layout.text());
    assert!(wide.layout.line_count() < first.layout.line_count());
}

#[test]
fn parley_layout_key_invalidates_content_style_theme_font_and_scale() {
    let base_input = input(vec![InlineSpan::plain("key")], 200.0);
    let base_options = options(200.0);
    let base = TextLayoutKey::from_input(&base_input, &base_options);

    let mut changed = base_input.clone();
    changed.content_version += 1;
    assert_ne!(base, TextLayoutKey::from_input(&changed, &base_options));

    let mut changed = base_input.clone();
    changed.spans[0].marks.push(InlineMark::Bold);
    assert_ne!(base, TextLayoutKey::from_input(&changed, &base_options));

    let mut changed = base_input.clone();
    changed.theme_version += 1;
    assert_ne!(base, TextLayoutKey::from_input(&changed, &base_options));

    let mut changed = base_input.clone();
    changed.font_version += 1;
    assert_ne!(base, TextLayoutKey::from_input(&changed, &base_options));

    let mut changed_options = base_options.clone();
    changed_options.display_scale = 2.0;
    assert_ne!(
        base,
        TextLayoutKey::from_input(&base_input, &changed_options)
    );
}

#[test]
fn parley_geometry_preserves_soft_wrap_affinity() {
    let text = "alpha beta gamma delta";
    let input = input(vec![InlineSpan::plain(text)], 75.0);
    let layout = build_text_layout(&input, theme(), &options(75.0));
    assert!(layout.line_count() > 1);

    let first_line = &layout.line_snapshots()[0];
    let boundary = first_line.text_range.end;
    let upstream = layout.caret_rect(
        TextLayoutPosition {
            offset: boundary,
            affinity: TextAffinity::Upstream,
        },
        1.0,
    );
    let downstream = layout.caret_rect(
        TextLayoutPosition {
            offset: boundary,
            affinity: TextAffinity::Downstream,
        },
        1.0,
    );

    assert!(upstream.y != downstream.y || upstream.x != downstream.x);
}

#[test]
fn parley_mixed_bidi_uses_visual_cursor_movement_and_split_selection_geometry() {
    let text = "abc אבג 123";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));

    let start = TextLayoutSelection {
        anchor: TextLayoutPosition::downstream(0),
        focus: TextLayoutPosition::downstream(0),
    };
    let moved = layout.move_selection(start, TextLayoutMoveCommand::NextVisual, false);
    assert_ne!(moved.focus, start.focus);
    assert!(text.is_char_boundary(moved.focus.offset));

    let hebrew_start = text.find('א').unwrap();
    let hebrew_end = text[hebrew_start..]
        .find(' ')
        .map(|offset| hebrew_start + offset)
        .unwrap_or(text.len());
    let rects = layout.selection_rects(TextLayoutSelection {
        anchor: TextLayoutPosition::downstream(hebrew_start),
        focus: TextLayoutPosition {
            offset: hebrew_end,
            affinity: TextAffinity::Upstream,
        },
    });
    assert!(!rects.is_empty());
    assert!(rects.iter().all(|rect| rect.width >= 0.0));
}

#[test]
fn range_geometry_is_normalized_and_owned_by_the_text_snapshot() {
    let text = "ab中cd";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));

    let cjk = layout.range_rects(2..5);
    assert!(!cjk.is_empty());
    assert!(
        cjk.iter()
            .all(|rect| rect.width >= 0.0 && rect.height > 0.0)
    );
    assert_eq!(layout.range_rects(3..3), Vec::new());
    assert_eq!(
        layout.range_rects(text.len() + 10..text.len() + 20),
        Vec::new()
    );
    let reversed = std::ops::Range { start: 5, end: 2 };
    assert_eq!(layout.range_rects(reversed), Vec::new());

    let split_scalar = layout.range_rects(3..4);
    assert!(!split_scalar.is_empty());
}

#[test]
fn parley_word_line_and_hard_line_selection_are_available() {
    let text = "first word\nsecond line";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));

    let word = layout.selection_at_point(50.0, 10.0, TextLayoutSelectionKind::Word);
    let line = layout.selection_at_point(50.0, 10.0, TextLayoutSelectionKind::Line);
    let hard_line = layout.selection_at_point(50.0, 10.0, TextLayoutSelectionKind::HardLine);

    assert!(!word.text_range().is_empty());
    assert!(line.text_range().len() >= word.text_range().len());
    assert!(hard_line.text_range().len() >= word.text_range().len());
}

#[test]
fn vertical_movement_preserves_preferred_x_across_short_lines() {
    let text = "abcdefghij\nx\nabcdefghij";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));
    let start = TextLayoutSelection {
        anchor: TextLayoutPosition::downstream(8),
        focus: TextLayoutPosition::downstream(8),
    };

    let (middle, preferred_x) =
        layout.move_selection_with_preferred_x(start, TextLayoutMoveCommand::NextLine, false, None);
    let (last, retained_x) = layout.move_selection_with_preferred_x(
        middle,
        TextLayoutMoveCommand::NextLine,
        false,
        preferred_x,
    );
    assert_eq!(retained_x, preferred_x);
    assert!(last.focus.offset >= 20);

    let (_, cleared_x) = layout.move_selection_with_preferred_x(
        last,
        TextLayoutMoveCommand::PreviousVisual,
        false,
        retained_x,
    );
    assert_eq!(cleared_x, None);
}

#[test]
fn parley_inline_boxes_participate_in_layout_and_report_positions() {
    let text = "before after";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(
        &input,
        theme(),
        &TextLayoutOptions {
            inline_boxes: vec![InlineBoxSpec {
                id: 42,
                kind: InlineBoxKind::InFlow,
                index: "before".len(),
                width: 48.0,
                height: 18.0,
            }],
            ..options(500.0)
        },
    );

    let boxes = layout.inline_boxes();
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].id, 42);
    assert_eq!(boxes[0].kind, InlineBoxKind::InFlow);
    assert_eq!(boxes[0].width, 48.0);
    assert_eq!(boxes[0].height, 18.0);
}

#[test]
fn focused_parley_layout_builds_accesskit_runs_and_selection() {
    let text = "abc אבג";
    let input = input(vec![InlineSpan::plain(text)], 500.0);
    let layout = build_text_layout(&input, theme(), &options(500.0));
    let projection = build_text_accessibility_projection(
        &layout,
        accesskit::NodeId(10),
        accesskit::NodeId(11),
        20.0,
        30.0,
        Some(TextLayoutSelection {
            anchor: TextLayoutPosition::downstream(0),
            focus: TextLayoutPosition {
                offset: text.len(),
                affinity: TextAffinity::Upstream,
            },
        }),
    );

    let parent = projection.parent_node().expect("parent node is projected");
    assert!(!parent.children().is_empty());
    assert!(parent.text_selection().is_some());
    assert!(projection.update.nodes.len() >= 2);
}

#[test]
fn paint_plan_keeps_exact_font_blob_face_and_glyph_data_outside_gpui() {
    let input = input(
        vec![InlineSpan {
            text: "paint".to_owned(),
            marks: vec![InlineMark::Code],
        }],
        500.0,
    );
    let layout = build_text_layout(&input, theme(), &options(500.0));
    let plan = layout.paint_plan();

    assert!(!plan.runs.is_empty());
    assert!(!plan.backgrounds.is_empty());
    assert!(plan.runs.iter().any(|run| !run.glyphs.is_empty()));
    assert!(plan.runs.iter().all(|run| !run.font.data().is_empty()));
    assert!(
        plan.runs
            .iter()
            .all(|run| run.font.blob_id() != 0 || !run.font.data().is_empty())
    );
    assert!(plan.runs.iter().all(|run| {
        run.text_range.start <= run.text_range.end
            && layout.text().is_char_boundary(run.text_range.start)
            && layout.text().is_char_boundary(run.text_range.end)
            && run.font.instance_key().face().face_index() == run.font.face_index()
            && run.font.instance_key().face().blob_id() == run.font.blob_id()
            && run.font.instance_key().face().blob_len() == run.font.data().len()
            && run.font.instance_key().normalized_coords() == run.font.normalized_coords
            && run.font.instance_key().synthesis().any() == run.font.synthesized
    }));
}

#[test]
fn empty_text_uses_base_font_metrics_for_visible_caret() {
    let input = input(vec![InlineSpan::plain("")], 320.0);
    let layout = build_text_layout(&input, theme(), &options(320.0));
    let caret = layout.caret_rect(TextLayoutPosition::downstream(0), 1.5);

    assert!(layout.is_empty());
    assert!(caret.width > 0.0);
    assert!(caret.height > 0.0);
    assert!(layout.height() >= caret.height);
}

#[test]
fn soft_wrapped_lines_keep_one_base_font_size_through_the_last_line() {
    let mut layout_options = options(96.0);
    layout_options.base_style.font_size = 19.0;
    let input = input(
        vec![InlineSpan::plain(
            "soft wrapping must preserve typography on every visual line",
        )],
        96.0,
    );
    let layout = build_text_layout(&input, theme(), &layout_options);
    let lines = layout.line_snapshots();
    let run_sizes = layout
        .paint_plan()
        .runs
        .iter()
        .map(|run| run.font_size)
        .collect::<Vec<_>>();

    assert!(lines.len() >= 2);
    assert!(!run_sizes.is_empty());
    assert!(run_sizes.iter().all(|size| (*size - 19.0).abs() < 0.001));
    assert_eq!(lines.last().unwrap().text_range.end, layout.text().len());
}

#[test]
fn soft_wrap_reflow_preserves_the_single_line_font_instance_and_size() {
    let text = "the same font instance must survive every visual line after wrapping";
    let mut single_line_options = options(1_000.0);
    single_line_options.base_style.font_size = 17.0;
    let mut wrapped_options = single_line_options.clone();
    wrapped_options.width = Some(92.0);
    let single_line = build_text_layout(
        &input(vec![InlineSpan::plain(text)], 1_000.0),
        theme(),
        &single_line_options,
    );
    let wrapped = build_text_layout(
        &input(vec![InlineSpan::plain(text)], 92.0),
        theme(),
        &wrapped_options,
    );
    let expected = single_line
        .paint_plan()
        .runs
        .first()
        .map(|run| (run.font.instance_key().clone(), run.font_size))
        .expect("single-line fixture must shape at least one run");

    assert_eq!(single_line.line_snapshots().len(), 1);
    assert!(wrapped.line_snapshots().len() > 1);
    assert!(wrapped.paint_plan().runs.iter().all(|run| {
        run.font.instance_key() == &expected.0 && (run.font_size - expected.1).abs() < 0.001
    }));
}
