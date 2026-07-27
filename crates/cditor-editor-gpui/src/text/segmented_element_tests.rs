use super::*;
use crate::text::input::RichTextLayoutSpans;
use crate::text::input::{
    plain_text_materializations_for_tests, reset_plain_text_materializations_for_tests,
};
use cditor_core::{
    ids::SurfaceId,
    rich_text::{InlineSpan, plain_text_from_spans},
};
use cditor_runtime::MainThreadWorkKind;
use cditor_text::{SegmentedLayoutConfig, SegmentedTextLayout};

#[test]
fn changed_range_preserves_utf8_boundaries() {
    let old = "a中c";
    let new = "a文c";
    let (range, replacement) = changed_text_range(old, new);
    assert_eq!(&old[range.clone()], "中");
    assert_eq!(replacement, "文");
    assert!(old.is_char_boundary(range.start));
    assert!(old.is_char_boundary(range.end));
}

#[test]
fn span_slicing_keeps_highlight_marks_and_exact_text() {
    let spans = vec![
        InlineSpan::plain("ab"),
        InlineSpan {
            text: "cdef".to_owned(),
            marks: vec![cditor_core::rich_text::InlineMark::Bold],
        },
    ];
    let sliced = RichTextLayoutSpans::from(spans).slice(1..5);
    assert_eq!(plain_text_from_spans(&sliced), "bcde");
    assert!(
        sliced[1]
            .marks
            .contains(&cditor_core::rich_text::InlineMark::Bold)
    );
}

#[test]
fn ten_mib_surface_sync_builds_only_the_segment_index() {
    let text = "0123456789abcdef\n".repeat((10 * 1024 * 1024) / 17 + 1);
    let input = RichTextLayoutInput {
        block_id: 9_001,
        surface_id: SurfaceId::Block(9_001),
        content_version: 1,
        layout_version: 1,
        kind: cditor_core::rich_text::RichBlockKind::Code { language: None },
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain(&text)].into(),
        width_px: 800.0,
        theme_version: 1,
        font_version: 1,
    };

    let (cached_text, layout) = cached_segmented_surface(&input, 1);
    let layout = layout.borrow();

    assert_eq!(cached_text.len(), text.len());
    assert!(layout.segment_count() > 1_000);
    assert_eq!(
        layout.measured_count(),
        0,
        "cache sync must not invoke the shaping engine"
    );
}

#[test]
fn ten_mib_unbroken_surface_is_indexed_as_bounded_fragments_without_shaping() {
    let text = "界".repeat((10 * 1024 * 1024) / "界".len() + 1);
    let input = RichTextLayoutInput {
        block_id: 9_004,
        surface_id: SurfaceId::Block(9_004),
        content_version: 1,
        layout_version: 1,
        kind: cditor_core::rich_text::RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain(&text)].into(),
        width_px: 800.0,
        theme_version: 1,
        font_version: 1,
    };

    let (cached_text, layout) = cached_segmented_surface(&input, 1);
    let layout = layout.borrow();

    assert_eq!(cached_text.len(), text.len());
    assert!(layout.has_forced_line_fragments());
    assert!(layout.segment_count() > 100);
    assert_eq!(layout.measured_count(), 0);
    assert!((0..layout.segment_count()).all(|index| {
        let range = layout.segment_byte_range(index).unwrap();
        cached_text.is_char_boundary(range.start) && cached_text.is_char_boundary(range.end)
    }));
}

#[test]
fn unchanged_segmented_cache_identity_does_not_materialize_plain_text() {
    let input = RichTextLayoutInput {
        block_id: 9_005,
        surface_id: SurfaceId::Block(9_005),
        content_version: 41,
        layout_version: 7,
        kind: cditor_core::rich_text::RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("first"), InlineSpan::plain(" second")].into(),
        width_px: 800.0,
        theme_version: 3,
        font_version: 5,
    };
    remove_cached_segmented_surface_for_tests(input.surface_id);
    reset_plain_text_materializations_for_tests();
    let (first_text, first_layout) = cached_segmented_surface(&input, 11);
    assert_eq!(plain_text_materializations_for_tests(), 1);

    reset_plain_text_materializations_for_tests();
    let mut same_content = input.clone();
    same_content.layout_version += 1;
    let (second_text, second_layout) = cached_segmented_surface(&same_content, 11);

    assert_eq!(plain_text_materializations_for_tests(), 0);
    assert!(Arc::ptr_eq(&first_text, &second_text));
    assert!(Rc::ptr_eq(&first_layout, &second_layout));
}

#[test]
fn highlight_marks_and_span_boundaries_change_segment_style_identity() {
    let mut input = RichTextLayoutInput {
        block_id: 9_002,
        surface_id: SurfaceId::Block(9_002),
        content_version: 1,
        layout_version: 1,
        kind: cditor_core::rich_text::RichBlockKind::Code { language: None },
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("abcdef")].into(),
        width_px: 800.0,
        theme_version: 1,
        font_version: 1,
    };
    let plain = segmented_style_fingerprint(
        &input,
        GuiTheme::light(),
        None,
        RichTextTypography::default(),
        "system-ui",
        gpui::FontWeight::NORMAL,
        gpui::FontStyle::Normal,
        1.0,
    );
    input.spans = vec![
        InlineSpan::plain("abc"),
        InlineSpan {
            text: "def".to_owned(),
            marks: vec![cditor_core::rich_text::InlineMark::Color(
                "#ff0000".to_owned(),
            )],
        },
    ]
    .into();
    let highlighted = segmented_style_fingerprint(
        &input,
        GuiTheme::light(),
        None,
        RichTextTypography::default(),
        "system-ui",
        gpui::FontWeight::NORMAL,
        gpui::FontStyle::Normal,
        1.0,
    );

    assert_ne!(plain, highlighted);
}

#[test]
fn block_kind_alignment_and_theme_change_segment_style_identity() {
    let mut input = RichTextLayoutInput {
        block_id: 9_006,
        surface_id: SurfaceId::Block(9_006),
        content_version: 1,
        layout_version: 1,
        kind: cditor_core::rich_text::RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain("same text")].into(),
        width_px: 800.0,
        theme_version: 1,
        font_version: 1,
    };
    let fingerprint = |input: &RichTextLayoutInput, theme| {
        segmented_style_fingerprint(
            input,
            theme,
            None,
            RichTextTypography::default(),
            "system-ui",
            gpui::FontWeight::NORMAL,
            gpui::FontStyle::Normal,
            1.0,
        )
    };
    let paragraph = fingerprint(&input, GuiTheme::light());

    input.kind = cditor_core::rich_text::RichBlockKind::Heading { level: 1 };
    let heading = fingerprint(&input, GuiTheme::light());
    input.kind = cditor_core::rich_text::RichBlockKind::Paragraph;
    input.text_align = cditor_core::rich_text::TextAlign::Center;
    let centered = fingerprint(&input, GuiTheme::light());
    let dark = fingerprint(&input, GuiTheme::dark());

    assert_ne!(paragraph, heading);
    assert_ne!(paragraph, centered);
    assert_ne!(centered, dark);
}

#[test]
fn production_segment_dispatch_prioritizes_editing_and_defers_prefetch_while_typing() {
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let layout = SegmentedTextLayout::new(
        text,
        SegmentedLayoutConfig {
            max_hard_lines_per_segment: 10,
            max_bytes_per_segment: usize::MAX,
            estimated_line_height_px: 24.0,
        },
    );
    let editing_offset = layout.segment_byte_range(7).unwrap().start;
    let mut requested_kinds = Vec::new();
    let scheduled = admitted_segments(
        &layout,
        &(0..layout.segment_count()).collect::<Vec<_>>(),
        3..5,
        &[editing_offset],
        false,
        |kind, _cost| {
            requested_kinds.push(kind);
            kind != MainThreadWorkKind::Prefetch
        },
    );

    assert_eq!(scheduled.first(), Some(&7));
    assert!(scheduled.contains(&3));
    assert!(scheduled.contains(&4));
    assert!(
        scheduled
            .iter()
            .all(|index| *index == 7 || (3..5).contains(index)),
        "typing frames must defer overscan prefetch: {scheduled:?}"
    );
    assert_eq!(requested_kinds[0], MainThreadWorkKind::EditingTextShape);
    assert_eq!(requested_kinds[1], MainThreadWorkKind::CurrentWindowMeasure);
    assert_eq!(requested_kinds[2], MainThreadWorkKind::CurrentWindowMeasure);
    assert!(
        requested_kinds[3..]
            .iter()
            .all(|kind| *kind == MainThreadWorkKind::Prefetch)
    );
}
