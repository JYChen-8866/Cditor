use cditor_core::{
    edit::TextAffinity,
    rich_text::{InlineSpan, RichBlockKind, TextAlign},
};
use proptest::prelude::*;

use super::*;

const PROPERTY_CASES: u32 = 96;
const MAX_DEVICE_PIXEL_DRIFT: f32 = 1.0;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPERTY_CASES))]

    #[test]
    fn point_index_caret_bounds_stay_visually_stable(
        tokens in prop::collection::vec(text_token(), 1..24),
        width_px in 72u16..520,
        scale_index in 0usize..3,
        x_fraction in 0u16..=1000,
        y_fraction in 0u16..=1000,
    ) {
        let text = tokens.concat();
        let scale = [1.0, 1.25, 2.0][scale_index];
        let layout = layout(&text, f32::from(width_px), scale);
        let x = layout.width().max(1.0) * f32::from(x_fraction) / 1000.0;
        let y = layout.height().max(1.0) * f32::from(y_fraction) / 1000.0;

        let hit = layout.position_for_point(x, y);
        prop_assert!(hit.offset <= text.len());
        prop_assert!(text.is_char_boundary(hit.offset));

        let caret = layout.caret_rect(hit, 1.0);
        prop_assert!(rect_is_finite(caret));
        prop_assert!(caret.height > 0.0);

        let repeated_hit = layout.position_for_point(x, y);
        prop_assert_eq!(repeated_hit, hit);
        let repeated_caret = layout.caret_rect(repeated_hit, 1.0);
        prop_assert!(rect_is_finite(repeated_caret));
        prop_assert!(
            same_visual_caret(caret, repeated_caret, scale),
            "text={text:?} point=({x},{y}) hit={hit:?} caret={caret:?} repeated_hit={repeated_hit:?} repeated_caret={repeated_caret:?} scale={scale}"
        );
    }

    #[test]
    fn every_generated_grapheme_boundary_has_valid_caret_geometry(
        tokens in prop::collection::vec(text_token(), 1..20),
        width_px in 64u16..420,
        scale_index in 0usize..3,
    ) {
        let text = tokens.concat();
        let scale = [1.0, 1.5, 2.0][scale_index];
        let layout = layout(&text, f32::from(width_px), scale);
        let snapshot = TextSnapshot::new(text.as_str());

        for grapheme_index in 0..=snapshot.grapheme_count() {
            let offset = snapshot.grapheme_to_byte(grapheme_index).unwrap();
            let position = TextLayoutPosition {
                offset,
                affinity: TextAffinity::Downstream,
            };
            let caret = layout.caret_rect(position, 1.0);
            prop_assert!(rect_is_finite(caret), "text={text:?} offset={offset}");
            prop_assert!(caret.height > 0.0, "text={text:?} offset={offset}");
            prop_assert!(caret.y >= -MAX_DEVICE_PIXEL_DRIFT / scale);
            prop_assert!(
                caret.y + caret.height <= layout.height() + MAX_DEVICE_PIXEL_DRIFT / scale,
                "text={text:?} offset={offset} caret={caret:?} height={} scale={scale}",
                layout.height()
            );
        }
    }
}

fn text_token() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("a"),
        Just("Word"),
        Just(" "),
        Just("\n"),
        Just("中"),
        Just("日本"),
        Just("한"),
        Just("مرحبا"),
        Just("שלום"),
        Just("e\u{301}"),
        Just("A\u{30a}"),
        Just("👩‍💻"),
        Just("👨‍👩‍👧‍👦"),
        Just("🇨🇳"),
        Just("123"),
        Just("،"),
        Just("."),
    ]
}

fn layout(text: &str, width: f32, scale: f32) -> TextLayoutSnapshot {
    let input = TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(900),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: f64::from(width),
        theme_version: 1,
        font_version: 1,
    };
    build_text_layout(
        &input,
        TextTheme {
            link_text: 0x0057ff,
            document_link_text: 0x9065b0,
            inline_code_text: 0xd1242f,
            inline_code_background: 0xf2f2f2,
        },
        &TextLayoutOptions {
            width: Some(width),
            display_scale: scale,
            quantize: false,
            base_style: TextStyleConfig {
                font_size: 17.0,
                line_height: TextLineHeight::Absolute(26.0),
                ..TextStyleConfig::default()
            },
            ..TextLayoutOptions::default()
        },
    )
}

fn rect_is_finite(rect: TextLayoutRect) -> bool {
    rect.x.is_finite() && rect.y.is_finite() && rect.width.is_finite() && rect.height.is_finite()
}

fn same_visual_caret(first: TextLayoutRect, second: TextLayoutRect, scale: f32) -> bool {
    device_pixel_difference(first.x, second.x, scale) <= MAX_DEVICE_PIXEL_DRIFT
        && device_pixel_difference(first.y, second.y, scale) <= MAX_DEVICE_PIXEL_DRIFT
        && device_pixel_difference(first.height, second.height, scale) <= MAX_DEVICE_PIXEL_DRIFT
}

fn device_pixel_difference(first: f32, second: f32, scale: f32) -> f32 {
    (first - second).abs() * scale
}
