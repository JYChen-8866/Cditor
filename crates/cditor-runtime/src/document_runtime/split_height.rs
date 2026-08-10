//! Height retention for text splits and same-kind payload replacements.
//!
//! A block that soft-wraps into several visual lines carries a `measured_height`
//! obtained at the real render width. Replacing its payload (an Enter split, a
//! formatting rewrite) used to discard that measurement and re-estimate at the
//! synthetic `layout_width_for_kind` width, which shifts every following block
//! for a frame or two until the GUI re-measures. These helpers scale the old
//! measurement by line count instead, so the first post-edit frame stays within
//! one line height of the truth. The result is still only an *estimate*: callers
//! keep `measured_height = None` and `dirty = true` so a real measurement lands
//! on the next painted frame.

use cditor_core::layout::text_line_height_for_kind;
use cditor_core::rich_text::{BlockPayload, RichBlockKind};

/// Scales a measured block height to a text part of `part_len` bytes out of
/// `total_len`, removing (or adding) whole lines while preserving the block's
/// chrome padding exactly. Returns `None` when the kind has no meaningful line
/// grid or the inputs cannot support a proportional model.
pub(super) fn scaled_split_height_estimate(
    kind: &RichBlockKind,
    measured_height: f64,
    total_len: usize,
    part_len: usize,
) -> Option<f64> {
    if !measured_height.is_finite() || measured_height <= 0.0 || total_len == 0 {
        return None;
    }
    if !matches!(
        cditor_core::layout::block_metrics::height_rule_for_kind(kind),
        cditor_core::layout::BlockHeightRule::TextLike(_)
    ) {
        return None;
    }
    let line_height = text_line_height_for_kind(kind);
    if !(line_height > 0.0) {
        return None;
    }
    let total_lines = (measured_height / line_height).round().max(1.0);
    let ratio = part_len as f64 / total_len as f64;
    let part_lines = (total_lines * ratio).ceil().max(1.0);
    let scaled = measured_height - (total_lines - part_lines) * line_height;
    Some(scaled.max(1.0))
}

/// Height estimate for a same-kind payload replacement that keeps the block's
/// real measured height as the reference instead of a synthetic-width estimate.
/// Falls back to `None` (caller uses the synthetic estimate) when the kinds
/// differ or either payload has no editable text.
pub(super) fn replacement_height_estimate(
    before_kind: &RichBlockKind,
    after_kind: &RichBlockKind,
    before_payload: &BlockPayload,
    after_payload: &BlockPayload,
    measured_height: Option<f64>,
) -> Option<f64> {
    if before_kind != after_kind {
        return None;
    }
    let measured = measured_height?;
    let before_len = super::editable_text_len_for_payload(before_payload)?;
    let after_len = super::editable_text_len_for_payload(after_payload)?;
    scaled_split_height_estimate(after_kind, measured, before_len, after_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::InlineSpan;

    fn paragraph(text: &str) -> BlockPayload {
        BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        }
    }

    #[test]
    fn identical_length_replacement_keeps_the_measured_height_exactly() {
        let kind = RichBlockKind::Paragraph;
        let line_height = text_line_height_for_kind(&kind);
        let measured = 3.0 * line_height + 8.0;
        assert_eq!(
            scaled_split_height_estimate(&kind, measured, 120, 120),
            Some(measured)
        );
    }

    #[test]
    fn split_halves_sum_to_the_original_within_one_line_height() {
        let kind = RichBlockKind::Paragraph;
        let line_height = text_line_height_for_kind(&kind);
        let measured = 5.0 * line_height + 8.0;
        for caret in [1usize, 40, 100, 199] {
            let prefix = scaled_split_height_estimate(&kind, measured, 200, caret).unwrap();
            let suffix = scaled_split_height_estimate(&kind, measured, 200, 200 - caret).unwrap();
            let sum = prefix + suffix;
            // The split duplicates the chrome padding and may add one wrap
            // boundary, so the sum sits within [measured, measured + chrome +
            // 2 lines) — well under the "one line height per block" target.
            assert!(
                sum >= measured && sum <= measured + 8.0 + 2.0 * line_height,
                "caret={caret} sum={sum} measured={measured}"
            );
            assert!(prefix >= 1.0 && suffix >= 1.0);
        }
    }

    #[test]
    fn growth_beyond_the_original_length_adds_lines() {
        let kind = RichBlockKind::Paragraph;
        let line_height = text_line_height_for_kind(&kind);
        let measured = 2.0 * line_height;
        let grown = scaled_split_height_estimate(&kind, measured, 100, 300).unwrap();
        assert!(grown > measured + 3.0 * line_height - f64::EPSILON);
    }

    #[test]
    fn non_text_kinds_and_empty_sources_fall_back() {
        assert_eq!(
            scaled_split_height_estimate(&RichBlockKind::Image, 180.0, 100, 50),
            None
        );
        assert_eq!(
            scaled_split_height_estimate(&RichBlockKind::Paragraph, 72.0, 0, 0),
            None
        );
        assert_eq!(
            scaled_split_height_estimate(&RichBlockKind::Paragraph, f64::NAN, 10, 5),
            None
        );
    }

    #[test]
    fn replacement_estimate_requires_same_kind_text_payloads_and_a_measurement() {
        let before = paragraph("a".repeat(200).as_str());
        let after = paragraph("a".repeat(100).as_str());
        assert!(
            replacement_height_estimate(
                &RichBlockKind::Paragraph,
                &RichBlockKind::Paragraph,
                &before,
                &after,
                Some(120.0),
            )
            .is_some()
        );
        assert_eq!(
            replacement_height_estimate(
                &RichBlockKind::Paragraph,
                &RichBlockKind::Heading { level: 1 },
                &before,
                &after,
                Some(120.0),
            ),
            None
        );
        assert_eq!(
            replacement_height_estimate(
                &RichBlockKind::Paragraph,
                &RichBlockKind::Paragraph,
                &before,
                &after,
                None,
            ),
            None
        );
    }
}
