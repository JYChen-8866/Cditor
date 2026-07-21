use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};

use super::*;
use crate::segmented::{SegmentedLayoutConfig, SegmentedTextLayout};

const EPSILON: f32 = 0.01;

fn code_options(width: Option<f32>) -> ParleyLayoutOptions {
    ParleyLayoutOptions {
        width,
        display_scale: 1.0,
        quantize: false,
        base_style: ParleyTextStyleConfig {
            font_size: 14.0,
            line_height: ParleyLineHeight::Absolute(22.0),
            ..ParleyTextStyleConfig::default()
        },
        ..ParleyLayoutOptions::default()
    }
}

fn build_slice(text: &str, width: Option<f32>) -> ParleyLayoutSnapshot {
    let input = TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(9_100),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: f64::from(width.unwrap_or(0.0)),
        theme_version: 1,
        font_version: 1,
    };
    build_parley_layout(&input, theme(), &code_options(width))
}

/// 混合短行与必然软换行的长行的代码样本。
fn code_sample(lines: usize) -> String {
    (0..lines)
        .map(|index| {
            if index % 17 == 5 {
                format!(
                    "    let very_long_line_{index} = compute({}); // {}\n",
                    "argument, ".repeat(18),
                    "wrap ".repeat(12)
                )
            } else {
                format!("    let value_{index} = {index};\n")
            }
        })
        .collect()
}

fn measure_all(layout: &mut SegmentedTextLayout, width: Option<f32>) {
    let all = 0..layout.segment_count();
    layout.measure_segments(all, |slice, _| build_slice(slice, width));
}

#[test]
fn segmented_total_height_matches_whole_layout_without_wrap() {
    let text = code_sample(64);
    let full = build_slice(&text, None);

    let mut segmented = SegmentedTextLayout::new(
        text,
        SegmentedLayoutConfig {
            max_hard_lines_per_segment: 10,
            max_bytes_per_segment: usize::MAX,
            estimated_line_height_px: 22.0,
        },
    );
    measure_all(&mut segmented, None);

    assert!(segmented.segment_count() > 1);
    assert!(
        (segmented.total_height() - full.height()).abs() < EPSILON,
        "segmented {} != full {}",
        segmented.total_height(),
        full.height()
    );
}

#[test]
fn segmented_total_height_matches_whole_layout_with_soft_wrap() {
    let text = code_sample(96);
    let width = Some(420.0);
    let full = build_slice(&text, width);

    let mut segmented = SegmentedTextLayout::new(
        text,
        SegmentedLayoutConfig {
            max_hard_lines_per_segment: 13,
            max_bytes_per_segment: 4 * 1024,
            estimated_line_height_px: 22.0,
        },
    );
    segmented.set_width(width);
    measure_all(&mut segmented, width);

    assert!(segmented.segment_count() > 3);
    assert!(
        (segmented.total_height() - full.height()).abs() < EPSILON,
        "segmented {} != full {} (soft wrap must not cross segment boundary)",
        segmented.total_height(),
        full.height()
    );
    // 存在软换行（总高大于纯硬行高度）。
    let hard_lines = segmented.text().lines().count() as f32;
    assert!(full.height() > hard_lines * 22.0 + EPSILON);
}

#[test]
fn windowed_measure_then_full_measure_converges() {
    let text = code_sample(200);
    let width = Some(500.0);
    let mut segmented = SegmentedTextLayout::new(
        text.clone(),
        SegmentedLayoutConfig {
            max_hard_lines_per_segment: 16,
            max_bytes_per_segment: usize::MAX,
            estimated_line_height_px: 22.0,
        },
    );
    segmented.set_width(width);

    // 只测可见窗口。
    let window = segmented.visible_segments(1_000.0, 800.0);
    assert!(!window.is_empty());
    segmented.measure_segments(window.clone(), |slice, _| build_slice(slice, width));
    assert_eq!(segmented.measured_count(), window.len());

    // 后台补齐全部后与整块布局一致。
    measure_all(&mut segmented, width);
    let full = build_slice(&text, width);
    assert!((segmented.total_height() - full.height()).abs() < EPSILON);
}

#[test]
fn single_segment_edit_only_invalidates_and_rebuilds_locally() {
    let text = code_sample(120);
    let width = Some(480.0);
    let mut segmented = SegmentedTextLayout::new(
        text,
        SegmentedLayoutConfig {
            max_hard_lines_per_segment: 15,
            max_bytes_per_segment: usize::MAX,
            estimated_line_height_px: 22.0,
        },
    );
    segmented.set_width(width);
    measure_all(&mut segmented, width);
    let measured_before = segmented.measured_count();
    assert_eq!(measured_before, segmented.segment_count());

    // 编辑第 3 段内部：只有受影响段失效。
    let target = segmented.segment_byte_range(2).unwrap();
    let insert_at = target.start + 4;
    segmented.replace_range(insert_at..insert_at, "let inserted_line = 42;\n");
    let invalidated = segmented.segment_count() - segmented.measured_count();
    assert!(
        (1..=2).contains(&invalidated),
        "expected 1-2 invalidated segments, got {invalidated}"
    );

    // 只补测失效段，总高与全新整块布局一致。
    let pending: Vec<usize> = (0..segmented.segment_count())
        .filter(|index| !segmented.is_measured(*index))
        .collect();
    for index in pending {
        segmented.measure_segments(index..index + 1, |slice, _| build_slice(slice, width));
    }
    let full = build_slice(segmented.text(), width);
    assert!((segmented.total_height() - full.height()).abs() < EPSILON);
}
