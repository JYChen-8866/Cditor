use std::{
    fs,
    path::{Path, PathBuf},
};

use cditor_core::{
    edit::TextAffinity,
    rich_text::{InlineMark, InlineSpan, RichBlockKind, TextAlign},
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::*;

const GOLDEN_SCHEMA_VERSION: u32 = 1;
const UNITS_PER_DEVICE_PIXEL: f32 = 64.0;
const FIXTURE_FONT_FAMILY: &str = "League Spartan";
const UPDATE_ENV: &str = "CDITOR_UPDATE_TEXT_VISUAL_GOLDEN";

#[derive(Debug, Serialize)]
struct VisualGolden {
    schema_version: u32,
    corpus_id: &'static str,
    parley_version: &'static str,
    coordinate_system: CoordinateSystem,
    font_sha256: String,
    cases: Vec<VisualCase>,
}

#[derive(Debug, Serialize)]
struct CoordinateSystem {
    unit: &'static str,
    units_per_device_pixel: u16,
}

#[derive(Debug, Serialize)]
struct VisualCase {
    id: &'static str,
    display_scale_milli: i32,
    requested_width: i32,
    layout_width: i32,
    full_width: i32,
    height: i32,
    lines: Vec<GoldenLine>,
    glyph_runs: Vec<GoldenGlyphRun>,
    carets: Vec<GoldenCaret>,
    selections: Vec<GoldenSelection>,
    backgrounds: Vec<GoldenBackground>,
}

#[derive(Debug, Serialize)]
struct GoldenLine {
    index: usize,
    text_range: [usize; 2],
    baseline: i32,
    top: i32,
    bottom: i32,
    advance: i32,
    offset: i32,
}

#[derive(Debug, Serialize)]
struct GoldenGlyphRun {
    family: String,
    font_sha256: String,
    face_index: u32,
    font_size: i32,
    weight_milli: i32,
    synthesized: bool,
    normalized_coords: Vec<i16>,
    foreground: u32,
    decoration_x: i32,
    decoration_width: i32,
    baseline: i32,
    glyphs: Vec<GoldenGlyph>,
    underline: Option<GoldenDecoration>,
}

#[derive(Debug, Serialize)]
struct GoldenGlyph {
    id: u32,
    x: i32,
    y: i32,
    color: bool,
}

#[derive(Debug, Serialize)]
struct GoldenDecoration {
    color: u32,
    offset: i32,
    size: i32,
}

#[derive(Debug, Serialize)]
struct GoldenCaret {
    label: &'static str,
    offset: usize,
    affinity: &'static str,
    rect: GoldenRect,
}

#[derive(Debug, Serialize)]
struct GoldenSelection {
    label: &'static str,
    anchor: GoldenPosition,
    focus: GoldenPosition,
    rects: Vec<GoldenRect>,
}

#[derive(Debug, Serialize)]
struct GoldenPosition {
    offset: usize,
    affinity: &'static str,
}

#[derive(Debug, Serialize)]
struct GoldenBackground {
    rect: GoldenRect,
    color: u32,
    radius: i32,
}

#[derive(Debug, Serialize)]
struct GoldenRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

struct VisualCaseSpec {
    id: &'static str,
    spans: Vec<InlineSpan>,
    width: f32,
    display_scale: f32,
    alignment: TextAlignment,
    carets: Vec<CaretSpec>,
    selections: Vec<SelectionSpec>,
}

struct CaretSpec {
    label: &'static str,
    position: TextLayoutPosition,
}

struct SelectionSpec {
    label: &'static str,
    selection: TextLayoutSelection,
}

#[test]
fn visual_layout_matches_versioned_golden() {
    let golden = build_visual_golden();
    assert_visual_coverage(&golden);
    let actual = format!("{}\n", serde_json::to_string_pretty(&golden).unwrap());
    let path = golden_path();

    if std::env::var_os(UPDATE_ENV).is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &actual).unwrap();
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "visual golden is missing at {}: {error}; regenerate with {UPDATE_ENV}=1",
            path.display()
        )
    });
    assert_eq!(
        expected, actual,
        "Parley visual output changed; inspect the five visual dimensions before regenerating \
         with {UPDATE_ENV}=1"
    );
}

fn build_visual_golden() -> VisualGolden {
    let font_data = fs::read(fixture_font_path()).unwrap();
    let font_sha256 = sha256(&font_data);
    let registered = register_font_data(font_data).unwrap();
    assert!(
        registered
            .iter()
            .any(|family| family.name == FIXTURE_FONT_FAMILY)
    );

    VisualGolden {
        schema_version: GOLDEN_SCHEMA_VERSION,
        corpus_id: "cditor-parley-visual-layout",
        parley_version: "0.11.0",
        coordinate_system: CoordinateSystem {
            unit: "1/64 device pixel",
            units_per_device_pixel: UNITS_PER_DEVICE_PIXEL as u16,
        },
        font_sha256,
        cases: visual_case_specs()
            .into_iter()
            .enumerate()
            .map(|(index, spec)| snapshot_case(index as u64 + 2_000, spec))
            .collect(),
    }
}

fn snapshot_case(block_id: u64, spec: VisualCaseSpec) -> VisualCase {
    let input = TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(block_id),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: TextAlign::Start,
        spans: spec.spans,
        width_px: f64::from(spec.width),
        theme_version: 1,
        font_version: 1,
    };
    let options = TextLayoutOptions {
        width: Some(spec.width),
        display_scale: spec.display_scale,
        quantize: true,
        alignment: spec.alignment,
        base_text_color: 0x37352f,
        mono_font_family: FIXTURE_FONT_FAMILY.to_owned(),
        base_style: TextStyleConfig {
            font_family: FIXTURE_FONT_FAMILY.to_owned(),
            font_size: 18.0,
            font_weight: 100.0,
            font_variations: "'wght' 450".to_owned(),
            line_height: TextLineHeight::Absolute(26.0),
            ..TextStyleConfig::default()
        },
        ..TextLayoutOptions::default()
    };
    let layout = build_text_layout(&input, visual_theme(), &options);
    let paint_plan = layout.paint_plan();

    VisualCase {
        id: spec.id,
        display_scale_milli: (spec.display_scale * 1_000.0).round() as i32,
        requested_width: to_units(spec.width, spec.display_scale),
        layout_width: to_units(layout.width(), spec.display_scale),
        full_width: to_units(layout.full_width(), spec.display_scale),
        height: to_units(layout.height(), spec.display_scale),
        lines: layout
            .line_snapshots()
            .iter()
            .map(|line| GoldenLine {
                index: line.index,
                text_range: [line.text_range.start, line.text_range.end],
                baseline: to_units(line.baseline, spec.display_scale),
                top: to_units(line.top, spec.display_scale),
                bottom: to_units(line.bottom, spec.display_scale),
                advance: to_units(line.advance, spec.display_scale),
                offset: to_units(line.offset, spec.display_scale),
            })
            .collect(),
        glyph_runs: paint_plan
            .runs
            .iter()
            .map(|run| GoldenGlyphRun {
                family: run.font.family.clone(),
                font_sha256: sha256(run.font.data()),
                face_index: run.font.face_index(),
                font_size: to_units(run.font_size, spec.display_scale),
                weight_milli: (run.font.weight * 1_000.0).round() as i32,
                synthesized: run.font.synthesized,
                normalized_coords: run.font.normalized_coords.clone(),
                foreground: run.brush.foreground,
                decoration_x: to_units(run.decoration_x, spec.display_scale),
                decoration_width: to_units(run.decoration_width, spec.display_scale),
                baseline: to_units(run.baseline, spec.display_scale),
                glyphs: run
                    .glyphs
                    .iter()
                    .map(|glyph| GoldenGlyph {
                        id: glyph.id,
                        x: to_units(glyph.x, spec.display_scale),
                        y: to_units(glyph.y, spec.display_scale),
                        color: glyph.color,
                    })
                    .collect(),
                underline: run.underline.map(|underline| GoldenDecoration {
                    color: underline.color,
                    offset: to_units(underline.offset, spec.display_scale),
                    size: to_units(underline.size, spec.display_scale),
                }),
            })
            .collect(),
        carets: spec
            .carets
            .into_iter()
            .map(|caret| GoldenCaret {
                label: caret.label,
                offset: caret.position.offset,
                affinity: affinity_name(caret.position.affinity),
                rect: rect_snapshot(layout.caret_rect(caret.position, 1.0), spec.display_scale),
            })
            .collect(),
        selections: spec
            .selections
            .into_iter()
            .map(|selection| GoldenSelection {
                label: selection.label,
                anchor: position_snapshot(selection.selection.anchor),
                focus: position_snapshot(selection.selection.focus),
                rects: layout
                    .selection_rects(selection.selection)
                    .into_iter()
                    .map(|rect| rect_snapshot(rect, spec.display_scale))
                    .collect(),
            })
            .collect(),
        backgrounds: paint_plan
            .backgrounds
            .iter()
            .map(|background| GoldenBackground {
                rect: rect_snapshot(background.rect, spec.display_scale),
                color: background.color,
                radius: to_units(background.radius, spec.display_scale),
            })
            .collect(),
    }
}

fn visual_case_specs() -> Vec<VisualCaseSpec> {
    vec![wrapped_case(), decorated_case(), hard_line_case()]
}

fn wrapped_case() -> VisualCaseSpec {
    let text = "Parley keeps wrapped lines stable.";
    VisualCaseSpec {
        id: "wrapped-latin-1x",
        spans: vec![InlineSpan::plain(text)],
        width: 142.0,
        display_scale: 1.0,
        alignment: TextAlignment::Start,
        carets: vec![
            caret("start", 0, TextAffinity::Downstream),
            caret("soft-wrap-upstream", 19, TextAffinity::Upstream),
            caret("soft-wrap-downstream", 19, TextAffinity::Downstream),
            caret("end", text.len(), TextAffinity::Upstream),
        ],
        selections: vec![selection("wrapped-lines", 13, 26)],
    }
}

fn decorated_case() -> VisualCaseSpec {
    let spans = vec![
        InlineSpan::plain("Links "),
        marked_span(
            "stay underlined",
            vec![InlineMark::Link {
                href: "https://cditor.dev".to_owned(),
            }],
        ),
        InlineSpan::plain(" and "),
        marked_span(
            "marks remain",
            vec![
                InlineMark::Underline,
                InlineMark::Color("#9b51e0".to_owned()),
                InlineMark::Background("#f2f2f2".to_owned()),
            ],
        ),
    ];
    let text_len = spans.iter().map(|span| span.text.len()).sum();
    VisualCaseSpec {
        id: "decorations-1_25x",
        spans,
        width: 210.0,
        display_scale: 1.25,
        alignment: TextAlignment::Center,
        carets: vec![
            caret("link-start", 6, TextAffinity::Downstream),
            caret("mark-start", 26, TextAffinity::Downstream),
            caret("end", text_len, TextAffinity::Upstream),
        ],
        selections: vec![selection("cross-style-runs", 3, 31)],
    }
}

fn hard_line_case() -> VisualCaseSpec {
    let text = "office affine\nsecond line";
    VisualCaseSpec {
        id: "hard-line-ligatures-2x",
        spans: vec![InlineSpan::plain(text)],
        width: 240.0,
        display_scale: 2.0,
        alignment: TextAlignment::End,
        carets: vec![
            caret("ligature-interior", 2, TextAffinity::Downstream),
            caret("hard-line-upstream", 14, TextAffinity::Upstream),
            caret("hard-line-downstream", 14, TextAffinity::Downstream),
        ],
        selections: vec![selection("across-hard-line", 7, 21)],
    }
}

fn marked_span(text: &str, marks: Vec<InlineMark>) -> InlineSpan {
    InlineSpan {
        text: text.to_owned(),
        marks,
    }
}

fn visual_theme() -> TextTheme {
    TextTheme {
        link_text: 0x2383e2,
        inline_code_text: 0xeb5757,
        inline_code_background: 0xf1f1ef,
    }
}

fn caret(label: &'static str, offset: usize, affinity: TextAffinity) -> CaretSpec {
    CaretSpec {
        label,
        position: TextLayoutPosition { offset, affinity },
    }
}

fn selection(label: &'static str, start: usize, end: usize) -> SelectionSpec {
    SelectionSpec {
        label,
        selection: TextLayoutSelection {
            anchor: TextLayoutPosition::downstream(start),
            focus: TextLayoutPosition {
                offset: end,
                affinity: TextAffinity::Upstream,
            },
        },
    }
}

fn assert_visual_coverage(golden: &VisualGolden) {
    assert_eq!(golden.schema_version, GOLDEN_SCHEMA_VERSION);
    assert_eq!(
        golden
            .cases
            .iter()
            .map(|case| case.display_scale_milli)
            .collect::<Vec<_>>(),
        [1_000, 1_250, 2_000]
    );
    assert!(golden.cases.iter().any(|case| case.lines.len() > 1));
    for run in golden.cases.iter().flat_map(|case| &case.glyph_runs) {
        assert_eq!(run.family, FIXTURE_FONT_FAMILY, "{run:?}");
        assert_eq!(run.font_sha256, golden.font_sha256, "{run:?}");
        assert!(!run.synthesized, "{run:?}");
    }
    assert!(
        golden
            .cases
            .iter()
            .all(|case| case.glyph_runs.iter().any(|run| !run.glyphs.is_empty()))
    );
    assert!(golden.cases.iter().all(|case| !case.carets.is_empty()));
    assert!(golden.cases.iter().all(|case| {
        case.selections
            .iter()
            .all(|selection| !selection.rects.is_empty())
    }));
    assert!(
        golden
            .cases
            .iter()
            .flat_map(|case| &case.glyph_runs)
            .any(|run| run.underline.is_some())
    );
}

fn position_snapshot(position: TextLayoutPosition) -> GoldenPosition {
    GoldenPosition {
        offset: position.offset,
        affinity: affinity_name(position.affinity),
    }
}

fn affinity_name(affinity: TextAffinity) -> &'static str {
    match affinity {
        TextAffinity::Upstream => "upstream",
        TextAffinity::Downstream => "downstream",
    }
}

fn rect_snapshot(rect: TextLayoutRect, scale: f32) -> GoldenRect {
    GoldenRect {
        x: to_units(rect.x, scale),
        y: to_units(rect.y, scale),
        width: to_units(rect.width, scale),
        height: to_units(rect.height, scale),
    }
}

fn to_units(logical_px: f32, scale: f32) -> i32 {
    (logical_px * scale * UNITS_PER_DEVICE_PIXEL).round() as i32
}

fn sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-layout/v1")
}

fn fixture_font_path() -> PathBuf {
    fixture_root().join("fonts/LeagueSpartan[wght].ttf")
}

fn golden_path() -> PathBuf {
    fixture_root().join("goldens/visual-layout-v1.json")
}
