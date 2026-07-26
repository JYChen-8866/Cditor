use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use cditor_core::edit::TextAffinity;
use cditor_core::rich_text::{InlineSpan, RichBlockKind};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use skrifa::{FontRef, MetadataProvider};

use super::*;

const FIXTURE_SCHEMA_VERSION: u32 = 1;
const REQUIRED_CASES: [&str; 6] = [
    "cjk",
    "emoji_zwj",
    "combining",
    "arabic",
    "hebrew",
    "mixed_bidi",
];

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    schema_version: u32,
    corpus_id: String,
    cases: Vec<FixtureCase>,
    variable_font: VariableFontFixture,
    color_font: ColorFontFixture,
    visual_golden: VisualGoldenFixture,
}

#[derive(Debug, Deserialize)]
struct FixtureCase {
    id: String,
    kind: String,
    text_file: String,
    min_graphemes: usize,
    min_clusters: usize,
    direction: FixtureDirection,
    contains_emoji: bool,
    contains_combining: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FixtureDirection {
    Ltr,
    Rtl,
    Mixed,
}

#[derive(Debug, Deserialize)]
struct VariableFontFixture {
    text_file: String,
    font_file: String,
    license_file: String,
    sha256: String,
    family: String,
    axis: VariableAxis,
    samples: Vec<VariationSample>,
}

#[derive(Debug, Deserialize)]
struct ColorFontFixture {
    font_file: String,
    notice_file: String,
    sha256: String,
    family: String,
    color_glyph_id: u32,
    non_color_glyph_id: u32,
}

#[derive(Debug, Deserialize)]
struct VariableAxis {
    tag: String,
    min: f32,
    default: f32,
    max: f32,
}

#[derive(Debug, Deserialize)]
struct VariationSample {
    name: String,
    settings: String,
}

#[derive(Debug, Deserialize)]
struct VisualGoldenFixture {
    schema_version: u32,
    snapshot_file: String,
    coordinate_units_per_device_pixel: u16,
    scales: Vec<f32>,
    dimensions: Vec<String>,
}

#[test]
fn fixture_manifest_is_versioned_complete_and_path_safe() {
    let manifest = load_manifest();
    assert_eq!(manifest.schema_version, FIXTURE_SCHEMA_VERSION);
    assert_eq!(manifest.corpus_id, "cditor-parley-text-layout");

    let mut ids = HashSet::new();
    let mut kinds = HashSet::new();
    for case in &manifest.cases {
        assert!(
            ids.insert(case.id.as_str()),
            "duplicate fixture id: {}",
            case.id
        );
        kinds.insert(case.kind.as_str());
        assert_safe_fixture_path(&case.text_file);
        let text = read_fixture_text(&case.text_file);
        assert!(!text.is_empty(), "fixture {} must not be empty", case.id);
        assert!(!text.contains('\r'), "fixture {} must use LF", case.id);
    }
    assert_eq!(kinds.len(), REQUIRED_CASES.len());
    for required in REQUIRED_CASES {
        assert!(kinds.contains(required), "missing fixture kind: {required}");
    }

    let variable = &manifest.variable_font;
    assert_safe_fixture_path(&variable.text_file);
    assert_safe_fixture_path(&variable.font_file);
    assert_safe_fixture_path(&variable.license_file);
    assert!(fixture_root().join(&variable.license_file).is_file());
    assert_eq!(variable.samples.len(), 2);
    assert_ne!(variable.samples[0].name, variable.samples[1].name);

    let color = &manifest.color_font;
    assert_safe_fixture_path(&color.font_file);
    assert_safe_fixture_path(&color.notice_file);
    assert!(fixture_root().join(&color.font_file).is_file());
    assert!(fixture_root().join(&color.notice_file).is_file());
    assert_ne!(color.color_glyph_id, color.non_color_glyph_id);

    let visual = &manifest.visual_golden;
    assert_eq!(visual.schema_version, 1);
    assert_safe_fixture_path(&visual.snapshot_file);
    assert!(fixture_root().join(&visual.snapshot_file).is_file());
    assert_eq!(visual.coordinate_units_per_device_pixel, 64);
    assert_eq!(visual.scales, [1.0, 1.25, 2.0]);
    assert_eq!(
        visual.dimensions,
        ["line_break", "glyph", "caret", "selection", "underline"]
    );
}

#[test]
fn multilingual_fixture_corpus_shapes_clusters_bidi_and_geometry() {
    let manifest = load_manifest();
    for (index, case) in manifest.cases.iter().enumerate() {
        let text = read_fixture_text(&case.text_file);
        let snapshot = TextSnapshot::new(text.as_str());
        let layout = build_text_layout(
            &layout_input(index as u64 + 1, &text),
            fixture_theme(),
            &fixture_options(),
        );

        assert_eq!(layout.text(), text, "fixture {} text changed", case.id);
        assert!(
            snapshot.grapheme_count() >= case.min_graphemes,
            "{}",
            case.id
        );
        assert!(layout.clusters().len() >= case.min_clusters, "{}", case.id);
        assert!(!layout.paint_plan().runs.is_empty(), "{}", case.id);
        assert!(
            layout
                .clusters()
                .iter()
                .all(|cluster| cluster.text_range.start < cluster.text_range.end
                    && text.is_char_boundary(cluster.text_range.start)
                    && text.is_char_boundary(cluster.text_range.end)),
            "fixture {} produced an invalid cluster range",
            case.id
        );

        let has_rtl = layout.clusters().iter().any(|cluster| cluster.is_rtl);
        let has_ltr = layout.clusters().iter().any(|cluster| !cluster.is_rtl);
        match case.direction {
            FixtureDirection::Ltr => assert!(has_ltr, "{}", case.id),
            FixtureDirection::Rtl => assert!(has_rtl, "{}", case.id),
            FixtureDirection::Mixed => {
                assert!(has_ltr, "{} lacks LTR clusters", case.id);
                assert!(has_rtl, "{} lacks RTL clusters", case.id);
            }
        }
        assert_eq!(
            layout.clusters().iter().any(|cluster| cluster.is_emoji),
            case.contains_emoji,
            "fixture {} emoji classification changed",
            case.id
        );
        if case.contains_combining {
            assert!(
                text.chars().count() > snapshot.grapheme_count(),
                "fixture {} must contain non-standalone scalars",
                case.id
            );
        }

        let full_selection = layout.selection_rects(TextLayoutSelection {
            anchor: TextLayoutPosition::downstream(0),
            focus: TextLayoutPosition {
                offset: text.len(),
                affinity: TextAffinity::Upstream,
            },
        });
        assert!(!full_selection.is_empty(), "{}", case.id);
        for grapheme_index in 0..=snapshot.grapheme_count() {
            let offset = snapshot.grapheme_to_byte(grapheme_index).unwrap();
            let rect = layout.caret_rect(TextLayoutPosition::downstream(offset), 1.0);
            assert!(rect.x.is_finite() && rect.y.is_finite(), "{}", case.id);
            assert!(rect.height > 0.0, "{}", case.id);
        }
    }
}

#[test]
fn variable_font_fixture_registers_exact_data_and_changes_normalized_axis_coords() {
    let manifest = load_manifest();
    let fixture = &manifest.variable_font;
    let font_data = fs::read(fixture_root().join(&fixture.font_file)).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&font_data)), fixture.sha256);

    let font = FontRef::new(&font_data).expect("fixture must be a readable OpenType font");
    let axis = font
        .axes()
        .iter()
        .find(|axis| axis.tag().to_string() == fixture.axis.tag)
        .expect("manifest axis must exist in the fixture font");
    assert_eq!(axis.min_value(), fixture.axis.min);
    assert_eq!(axis.default_value(), fixture.axis.default);
    assert_eq!(axis.max_value(), fixture.axis.max);

    let cache_probe = layout_input(699, "font registration cache probe");
    let cached = cached_text_layout(&cache_probe, fixture_theme(), &fixture_options());
    assert!(!cached.cache_hit);
    assert!(text_layout_cache_stats().entries > 0);

    let registered = register_font_data(font_data.clone()).unwrap();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].name, fixture.family);
    assert_eq!(registered[0].face_count, 1);
    assert_eq!(text_layout_cache_stats().entries, 0);

    let text = read_fixture_text(&fixture.text_file);
    let input = layout_input(700, &text);
    let plans = fixture
        .samples
        .iter()
        .map(|sample| {
            let mut options = fixture_options();
            options.base_style.font_family = fixture.family.clone();
            options.base_style.font_weight = fixture.axis.default;
            options.base_style.font_variations = sample.settings.clone();
            let layout = build_text_layout(&input, fixture_theme(), &options);
            let plan = layout.paint_plan();
            assert!(!plan.runs.is_empty(), "sample {}", sample.name);
            assert!(
                plan.runs
                    .iter()
                    .all(|run| run.font.family == fixture.family)
            );
            assert!(plan.runs.iter().all(|run| run.font.face_index() == 0));
            assert!(plan.runs.iter().all(|run| run.font.data() == font_data));
            assert!(plan.runs.iter().all(|run| !run.font.synthesized));
            assert!(plan.runs.iter().all(|run| {
                run.font.blob_digest().to_hex() == fixture.sha256
                    && run.font.instance_key().face().blob_id() == run.font.blob_id()
                    && run.font.instance_key().face().blob_len() == font_data.len()
                    && run.font.instance_key().face().face_index() == 0
                    && run.font.instance_key().normalized_coords()
                        == run.font.normalized_coords.as_slice()
                    && !run.font.instance_key().synthesis().any()
            }));
            plan.runs[0].font.normalized_coords.clone()
        })
        .collect::<Vec<_>>();
    assert_ne!(plans[0], plans[1]);
    assert!(plans.iter().any(|coordinates| !coordinates.is_empty()));
}

#[test]
fn color_font_fixture_identifies_individual_colrv1_glyphs() {
    let fixture = load_manifest().color_font;
    let font_data = fs::read(fixture_root().join(&fixture.font_file)).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&font_data)), fixture.sha256);

    let font = FontRef::new(&font_data).expect("fixture must be a readable OpenType font");
    assert_eq!(
        font.localized_strings(skrifa::string::StringId::FAMILY_NAME)
            .english_or_first()
            .map(|name| name.to_string())
            .as_deref(),
        Some(fixture.family.as_str())
    );
    assert!(super::super::paint_plan::font_ref_has_color_glyph(
        &font,
        fixture.color_glyph_id,
    ));
    assert!(!super::super::paint_plan::font_ref_has_color_glyph(
        &font,
        fixture.non_color_glyph_id,
    ));
}

#[test]
fn font_registration_rejects_empty_and_invalid_data() {
    assert_eq!(
        register_font_data(Vec::new()),
        Err(FontRegistrationError::EmptyData)
    );
    assert_eq!(
        register_font_data(vec![0, 1, 2, 3]),
        Err(FontRegistrationError::NoFontFaces)
    );
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-layout/v1")
}

fn load_manifest() -> FixtureManifest {
    let manifest = fs::read_to_string(fixture_root().join("manifest.json")).unwrap();
    serde_json::from_str(&manifest).expect("fixture manifest must match schema v1")
}

fn read_fixture_text(relative_path: &str) -> String {
    assert_safe_fixture_path(relative_path);
    fs::read_to_string(fixture_root().join(relative_path))
        .unwrap()
        .trim_end_matches('\n')
        .to_owned()
}

fn assert_safe_fixture_path(path: &str) {
    let path = Path::new(path);
    assert!(!path.is_absolute(), "fixture path must be relative");
    assert!(
        path.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "fixture path may not escape its version directory: {}",
        path.display()
    );
}

fn layout_input(block_id: u64, text: &str) -> TextLayoutInput {
    TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(block_id),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: 280.0,
        theme_version: 1,
        font_version: 1,
    }
}

fn fixture_options() -> TextLayoutOptions {
    TextLayoutOptions {
        width: Some(280.0),
        quantize: false,
        base_style: TextStyleConfig {
            font_size: 18.0,
            line_height: TextLineHeight::Absolute(28.0),
            ..TextStyleConfig::default()
        },
        ..TextLayoutOptions::default()
    }
}

fn fixture_theme() -> TextTheme {
    TextTheme {
        link_text: 0x0057ff,
        inline_code_text: 0xd1242f,
        inline_code_background: 0xf2f2f2,
    }
}
