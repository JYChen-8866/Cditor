#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_font_parses_and_lays_out() {
        let f = Font::default();
        let l = f.layout("A", 20.0);
        assert!(l.width > 0.0, "width {}", l.width);
        assert!(l.line_height > 0.0);
        assert!(
            l.segs.iter().any(|s| matches!(s, Seg::Move(_))),
            "expected contours"
        );
        // JetBrains Mono is monospace: "MM" is twice the advance of "M".
        let one = f.measure("M", 20.0).0;
        let two = f.measure("MM", 20.0).0;
        assert!(one > 0.0 && (two - 2.0 * one).abs() < 0.5, "{one} {two}");
    }

    #[test]
    fn newlines_stack_lines_and_place_the_caret() {
        let f = Font::default();
        let one = f.measure("x", 20.0).1;
        let two = f.measure("x\ny", 20.0).1;
        assert!((two - 2.0 * one).abs() < 0.5, "{one} {two}");
        // The caret of a two-line block sits on the lower line.
        let l = f.layout("x\ny", 20.0);
        assert!(l.caret[1] > 0.0, "caret {:?}", l.caret);
    }

    #[test]
    fn empty_content_has_no_segments_but_a_caret() {
        let f = Font::default();
        let l = f.layout("", 20.0);
        assert!(l.segs.is_empty());
        assert_eq!(l.caret, [0.0, 0.0]);
        assert!(l.height > 0.0);
    }

    #[test]
    fn bad_bytes_are_rejected() {
        assert!(Font::from_bytes(vec![0, 1, 2, 3], 0).is_none());
    }

    #[test]
    fn caret_pos_walks_per_char_and_across_lines() {
        let f = Font::default();
        let a = f.measure("M", 20.0).0; // monospace advance
        let lh = f.layout("M", 20.0).line_height;
        // One stop per boundary on a line.
        assert_eq!(f.caret_pos("MMM", 20.0, 0), [0.0, 0.0]);
        assert!((f.caret_pos("MMM", 20.0, 1)[0] - a).abs() < 0.5);
        assert!((f.caret_pos("MMM", 20.0, 3)[0] - 3.0 * a).abs() < 0.5);
        // Byte 2 of "M\nM" is the start of the second line (col 0, lower row).
        let c = f.caret_pos("M\nM", 20.0, 2);
        assert!(c[0].abs() < 0.5 && (c[1] - lh).abs() < 0.5, "{c:?}");
        // Byte 3 is one advance into that second line.
        let c = f.caret_pos("M\nM", 20.0, 3);
        assert!((c[0] - a).abs() < 0.5 && (c[1] - lh).abs() < 0.5, "{c:?}");
    }

    #[test]
    fn index_at_picks_the_nearest_boundary() {
        let f = Font::default();
        let a = f.measure("M", 20.0).0;
        let lh = f.layout("M", 20.0).line_height;
        // Just past the first glyph's midpoint rounds to the next boundary.
        assert_eq!(f.index_at("MMM", 20.0, [0.4 * a, 0.0]), 0);
        assert_eq!(f.index_at("MMM", 20.0, [0.6 * a, 0.0]), 1);
        // Way off to the right clamps to the line end; clicking the lower line
        // (by y) lands on it.
        assert_eq!(f.index_at("MMM", 20.0, [99.0 * a, 0.0]), 3);
        assert_eq!(f.index_at("M\nMM", 20.0, [0.0, lh]), 2); // start of line 2
    }

    #[test]
    fn selection_rects_span_lines() {
        let f = Font::default();
        let a = f.measure("M", 20.0).0;
        // Single line, two chars selected → one rect ~2 advances wide.
        let r = f.selection_rects("MMMM", 20.0, 1, 3);
        assert_eq!(r.len(), 1);
        assert!(
            (r[0][0] - a).abs() < 0.5 && (r[0][2] - 2.0 * a).abs() < 0.5,
            "{r:?}"
        );
        // Across a newline → one rect per line.
        let r = f.selection_rects("M\nMM", 20.0, 0, 4);
        assert_eq!(r.len(), 2, "{r:?}");
        // Empty selection → nothing.
        assert!(f.selection_rects("MM", 20.0, 2, 2).is_empty());
    }

    #[test]
    fn wrap_breaks_long_lines_at_spaces() {
        let f = Font::default();
        let lh = f.measure("x", 20.0).1;
        let full = f.measure("hello world", 20.0).0;
        // Wrapping below the phrase width forces a second line.
        let (w, h) = f.measure_wrapped("hello world", 20.0, Some(full * 0.6));
        assert!(
            (h - 2.0 * lh).abs() < 0.5,
            "expected 2 lines: h={h} lh={lh}"
        );
        assert!(w <= full * 0.6 + 0.5, "lines stay within the width: {w}");
        // A width wider than the text wraps nothing — identical to no wrap.
        assert_eq!(
            f.measure_wrapped("hello world", 20.0, Some(10_000.0)),
            f.measure("hello world", 20.0),
        );
    }

    #[test]
    fn wrap_keeps_byte_offsets_contiguous() {
        let f = Font::default();
        let s = "alpha beta gamma";
        let full = f.measure(s, 20.0).0;
        // The end-of-content caret is still reachable (no byte dropped at a soft
        // break) and sits lower than the single-line end (more visual lines).
        let end = f.caret_pos_wrapped(s, 20.0, Some(full * 0.5), s.len());
        let plain = f.caret_pos(s, 20.0, s.len());
        assert!(end[1] > plain[1], "wrapped end lower: {end:?} vs {plain:?}");
    }

    #[test]
    fn fit_size_shrinks_for_a_small_box() {
        let f = Font::default();
        let big = f.fit_size("hello world", 1000.0, 1000.0, 40.0);
        let small = f.fit_size("hello world", 60.0, 60.0, 40.0);
        assert_eq!(big, 40.0, "fits at max in a big box");
        assert!(small < big, "shrinks in a small box: {small} vs {big}");
        // The shrunk label actually fits inside its box.
        let (w, h) = f.measure_wrapped("hello world", small, Some(60.0));
        assert!(w <= 60.5 && h <= 60.5, "fits: {w}x{h}");
    }

    #[test]
    fn styled_layout_bolds_and_decorates() {
        let f = Font::default();
        let plain = f.layout_styled("Ab", 20.0, None, |_| GlyphStyle::default());
        let bold = f.layout_styled("Ab", 20.0, None, |_| GlyphStyle {
            bold: true,
            ..Default::default()
        });
        assert!(
            !bold.bold_segs.is_empty() && plain.bold_segs.is_empty(),
            "bold collects outlines to stroke"
        );
        assert!(plain.decorations.is_empty());
        let und = f.layout_styled("Ab", 20.0, None, |_| GlyphStyle {
            underline: true,
            ..Default::default()
        });
        assert!(
            und.decorations
                .iter()
                .any(|d| d.kind == DecoKind::Underline)
        );
        let hi = f.layout_styled("Ab", 20.0, None, |_| GlyphStyle {
            highlight: Some(0xffff00ff),
            ..Default::default()
        });
        assert!(
            hi.decorations
                .iter()
                .any(|d| d.kind == DecoKind::Highlight(0xffff00ff))
        );
    }

    #[test]
    fn styled_layout_decoration_runs_split() {
        let f = Font::default();
        // Underline only the first of two chars → one underline run ~1 advance wide.
        let l = f.layout_styled("MM", 20.0, None, |b| GlyphStyle {
            underline: b == 0,
            ..Default::default()
        });
        let unders: Vec<_> = l
            .decorations
            .iter()
            .filter(|d| d.kind == DecoKind::Underline)
            .collect();
        assert_eq!(unders.len(), 1, "{:?}", l.decorations);
        let adv = f.measure("M", 20.0).0;
        assert!(
            (unders[0].rect[2] - adv).abs() < 0.5,
            "≈1 advance: {}",
            unders[0].rect[2]
        );
    }
}
