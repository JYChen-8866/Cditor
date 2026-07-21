use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{Mutex, OnceLock},
};

use cditor_text::{ParleyLayoutSnapshot, ParleyPaintFont, ParleyPaintFontStyle, ParleyPaintRun};
use gpui::{
    Bounds, FontId, FontStyle as GpuiFontStyle, FontWeight as GpuiFontWeight, GlyphId, Hsla,
    PaintQuad, Pixels, Point, StrikethroughStyle, TextRun, UnderlineStyle, Window, fill, font,
    point, px, rgb, size,
};

use super::exact_raster::{ExactRasterErrorKind, exact_raster_cache_stats, paint_exact_glyph};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ParleyPaintReport {
    pub glyphs_painted: usize,
    pub glyph_errors: usize,
    pub font_registration_errors: usize,
    pub synthesized_runs: usize,
    pub variable_runs: usize,
    pub collection_face_runs: usize,
    pub exact_candidate_runs: usize,
    pub inexact_font_runs: usize,
    pub glyph_validation_matches: usize,
    pub glyph_validation_mismatches: usize,
    pub glyph_validation_skipped: usize,
    pub exact_raster_runs: usize,
    pub exact_raster_glyphs: usize,
    pub exact_raster_errors: usize,
    pub exact_raster_cache_hits: usize,
    pub exact_raster_cache_misses: usize,
    pub exact_raster_cache_entries: usize,
    pub exact_raster_cache_bytes: usize,
    pub first_exact_raster_error_kind: Option<ExactRasterErrorKind>,
    pub first_exact_raster_error_blob_id: Option<u64>,
    pub first_exact_raster_error_face_index: Option<u32>,
    pub first_exact_raster_error_glyph_id: Option<u32>,
}

impl ParleyPaintReport {
    fn record_exact_raster_error(
        &mut self,
        kind: ExactRasterErrorKind,
        blob_id: u64,
        face_index: u32,
        glyph_id: u32,
    ) {
        if self.first_exact_raster_error_kind.is_some() {
            return;
        }
        self.first_exact_raster_error_kind = Some(kind);
        self.first_exact_raster_error_blob_id = Some(blob_id);
        self.first_exact_raster_error_face_index = Some(face_index);
        self.first_exact_raster_error_glyph_id = Some(glyph_id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GpuiFontBridgeStatus {
    ExactCandidate,
    CollectionFaceUnsupported,
    VariableInstanceUnsupported,
    SynthesisUnsupported,
    FamilyResolutionUnverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlyphPaintPath {
    GpuiGlyphAtlas,
    ExactRasterImageAtlas,
}

pub(crate) fn parley_background_quads(
    snapshot: &ParleyLayoutSnapshot,
    origin: Point<Pixels>,
) -> Vec<PaintQuad> {
    snapshot
        .paint_plan()
        .backgrounds
        .iter()
        .map(|background| {
            fill(
                Bounds::new(
                    point(
                        origin.x + px(background.rect.x),
                        origin.y + px(background.rect.y),
                    ),
                    size(px(background.rect.width), px(background.rect.height)),
                ),
                rgb(background.color),
            )
            .corner_radii(px(background.radius))
        })
        .collect()
}

pub(crate) fn paint_parley_layout(
    snapshot: &ParleyLayoutSnapshot,
    origin: Point<Pixels>,
    window: &mut Window,
) -> ParleyPaintReport {
    let mut report = ParleyPaintReport::default();
    for run in &snapshot.paint_plan().runs {
        if run.font.synthesized {
            report.synthesized_runs += 1;
        }
        if !run.font.normalized_coords.is_empty() {
            report.variable_runs += 1;
        }
        if run.font.face_index() != 0 {
            report.collection_face_runs += 1;
        }
        let can_try_gpui_exact = run.font.face_index() == 0
            && run.font.instance_key().normalized_coords().is_empty()
            && !run.font.instance_key().synthesis().any();
        let exact_blob_registered = can_try_gpui_exact
            && match ensure_font_available(&run.font, window) {
                Ok(exact_blob_registered) => exact_blob_registered,
                Err(_) => {
                    report.font_registration_errors += 1;
                    false
                }
            };
        let bridge_status = gpui_font_bridge_status(
            run.font.face_index(),
            run.font.instance_key().normalized_coords(),
            run.font.instance_key().synthesis().any(),
            exact_blob_registered,
        );
        if bridge_status == GpuiFontBridgeStatus::ExactCandidate {
            report.exact_candidate_runs += 1;
        } else {
            report.inexact_font_runs += 1;
        }
        let (font_id, validation) = if bridge_status == GpuiFontBridgeStatus::ExactCandidate {
            let font_id = resolve_gpui_font(&run.font, window);
            (
                Some(font_id),
                validate_gpui_glyph_ids(snapshot, run, font_id, window, bridge_status),
            )
        } else {
            (None, None)
        };
        match validation {
            Some(true) => report.glyph_validation_matches += 1,
            Some(false) => report.glyph_validation_mismatches += 1,
            None => report.glyph_validation_skipped += 1,
        }
        match glyph_paint_path(bridge_status, validation) {
            GlyphPaintPath::GpuiGlyphAtlas => {
                paint_gpui_glyph_run(
                    run,
                    origin,
                    font_id.expect("validated GPUI path must have a resolved font"),
                    window,
                    &mut report,
                );
            }
            GlyphPaintPath::ExactRasterImageAtlas => {
                paint_exact_raster_run(run, origin, window, &mut report);
            }
        }
        paint_decorations(run, origin, window);
    }
    let raster_stats = exact_raster_cache_stats();
    report.exact_raster_cache_entries = raster_stats.entries;
    report.exact_raster_cache_bytes = raster_stats.estimated_bytes;
    report
}

fn glyph_paint_path(
    bridge_status: GpuiFontBridgeStatus,
    glyph_validation: Option<bool>,
) -> GlyphPaintPath {
    if bridge_status == GpuiFontBridgeStatus::ExactCandidate && glyph_validation == Some(true) {
        GlyphPaintPath::GpuiGlyphAtlas
    } else {
        GlyphPaintPath::ExactRasterImageAtlas
    }
}

fn paint_gpui_glyph_run(
    run: &ParleyPaintRun,
    origin: Point<Pixels>,
    font_id: FontId,
    window: &mut Window,
    report: &mut ParleyPaintReport,
) {
    let color = Hsla::from(rgb(run.brush.foreground));
    for glyph in &run.glyphs {
        let glyph_origin = point(origin.x + px(glyph.x), origin.y + px(glyph.y));
        let result = if glyph.color {
            window.paint_emoji(glyph_origin, font_id, GlyphId(glyph.id), px(run.font_size))
        } else {
            window.paint_glyph(
                glyph_origin,
                font_id,
                GlyphId(glyph.id),
                px(run.font_size),
                color,
            )
        };
        if result.is_ok() {
            report.glyphs_painted += 1;
        } else {
            report.glyph_errors += 1;
        }
    }
}

fn paint_exact_raster_run(
    run: &ParleyPaintRun,
    origin: Point<Pixels>,
    window: &mut Window,
    report: &mut ParleyPaintReport,
) {
    report.exact_raster_runs += 1;
    for glyph in &run.glyphs {
        let baseline_origin = point(origin.x + px(glyph.x), origin.y + px(glyph.y));
        match paint_exact_glyph(
            &run.font,
            glyph.id,
            glyph.color,
            run.font_size,
            run.brush.foreground,
            baseline_origin,
            window,
        ) {
            Ok(result) => {
                if result.painted {
                    report.glyphs_painted += 1;
                    report.exact_raster_glyphs += 1;
                }
                if result.cache_hit {
                    report.exact_raster_cache_hits += 1;
                } else {
                    report.exact_raster_cache_misses += 1;
                }
            }
            Err(error) => {
                report.glyph_errors += 1;
                report.exact_raster_errors += 1;
                report.record_exact_raster_error(
                    error.kind(),
                    run.font.blob_id(),
                    run.font.face_index(),
                    glyph.id,
                );
            }
        }
    }
}

fn paint_decorations(run: &ParleyPaintRun, origin: Point<Pixels>, window: &mut Window) {
    if let Some(underline) = run.underline {
        window.paint_underline(
            point(
                origin.x + px(run.decoration_x),
                origin.y + px(run.baseline + underline.offset),
            ),
            px(run.decoration_width),
            &UnderlineStyle {
                color: Some(Hsla::from(rgb(underline.color))),
                thickness: px(underline.size.max(1.0)),
                wavy: false,
            },
        );
    }
    if let Some(strikethrough) = run.strikethrough {
        window.paint_strikethrough(
            point(
                origin.x + px(run.decoration_x),
                origin.y + px(run.baseline + strikethrough.offset),
            ),
            px(run.decoration_width),
            &StrikethroughStyle {
                color: Some(Hsla::from(rgb(strikethrough.color))),
                thickness: px(strikethrough.size.max(1.0)),
            },
        );
    }
}

fn resolve_gpui_font(font_info: &ParleyPaintFont, window: &Window) -> FontId {
    window
        .text_system()
        .resolve_font(&gpui_font_descriptor(font_info))
}

fn gpui_font_descriptor(font_info: &ParleyPaintFont) -> gpui::Font {
    let family = if font_info.family == "system-ui" {
        ".SystemUIFont"
    } else {
        font_info.family.as_str()
    };
    let mut descriptor = font(family);
    descriptor.weight = GpuiFontWeight(font_info.weight);
    descriptor.style = match font_info.style {
        ParleyPaintFontStyle::Normal => GpuiFontStyle::Normal,
        ParleyPaintFontStyle::Italic => GpuiFontStyle::Italic,
        ParleyPaintFontStyle::Oblique => GpuiFontStyle::Oblique,
    };
    descriptor
}

fn ensure_font_available(font_info: &ParleyPaintFont, window: &Window) -> gpui::Result<bool> {
    // GPUI's public add_fonts API accepts a complete file but not a TTC face index.
    // Register face zero before resolving the descriptor. The text-system identity is
    // part of the cache key so a newly created window cannot inherit another window's
    // registration record.
    if font_info.face_index() != 0 {
        return Err(std::io::Error::other(format!(
            "GPUI cannot register collection face index {} for {}",
            font_info.face_index(),
            font_info.family
        ))
        .into());
    }
    static REGISTERED_BLOBS: OnceLock<Mutex<HashSet<(usize, u64)>>> = OnceLock::new();
    let text_system_id = std::ptr::from_ref(window.text_system()) as usize;
    let blob_id = font_info.blob_id();
    let registered = REGISTERED_BLOBS.get_or_init(|| Mutex::new(HashSet::new()));
    if registered
        .lock()
        .expect("registered font lock poisoned")
        .contains(&(text_system_id, blob_id))
    {
        return Ok(true);
    }

    static KNOWN_FAMILIES: OnceLock<Mutex<HashMap<usize, HashSet<String>>>> = OnceLock::new();
    let known_families = KNOWN_FAMILIES.get_or_init(|| Mutex::new(HashMap::new()));
    let family_is_known = {
        let mut known_families = known_families
            .lock()
            .expect("known font family lock poisoned");
        let families = known_families.entry(text_system_id).or_insert_with(|| {
            window
                .text_system()
                .all_font_names()
                .into_iter()
                .map(|name| name.to_lowercase())
                .collect()
        });
        families.contains(&font_info.family.to_lowercase())
    };
    if family_is_known {
        return Ok(false);
    }

    window
        .text_system()
        .add_fonts(vec![Cow::Owned(font_info.data().to_vec())])?;
    registered
        .lock()
        .expect("registered font lock poisoned")
        .insert((text_system_id, blob_id));
    known_families
        .lock()
        .expect("known font family lock poisoned")
        .entry(text_system_id)
        .or_default()
        .insert(font_info.family.to_lowercase());
    Ok(true)
}

fn gpui_font_bridge_status(
    face_index: u32,
    normalized_coords: &[i16],
    synthesized: bool,
    exact_blob_registered: bool,
) -> GpuiFontBridgeStatus {
    if face_index != 0 {
        GpuiFontBridgeStatus::CollectionFaceUnsupported
    } else if synthesized {
        GpuiFontBridgeStatus::SynthesisUnsupported
    } else if !normalized_coords.is_empty() {
        GpuiFontBridgeStatus::VariableInstanceUnsupported
    } else if !exact_blob_registered {
        GpuiFontBridgeStatus::FamilyResolutionUnverified
    } else {
        GpuiFontBridgeStatus::ExactCandidate
    }
}

fn validate_gpui_glyph_ids(
    snapshot: &ParleyLayoutSnapshot,
    run: &ParleyPaintRun,
    font_id: FontId,
    window: &Window,
    bridge_status: GpuiFontBridgeStatus,
) -> Option<bool> {
    if bridge_status != GpuiFontBridgeStatus::ExactCandidate || run.glyphs.is_empty() {
        return None;
    }
    let text = snapshot.text().get(run.text_range.clone())?;
    if text.contains('\n') {
        return None;
    }

    let key = glyph_validation_key(window, run, text);
    static RESULTS: OnceLock<Mutex<HashMap<u64, bool>>> = OnceLock::new();
    let results = RESULTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(result) = results
        .lock()
        .expect("glyph validation cache lock poisoned")
        .get(&key)
        .copied()
    {
        return Some(result);
    }

    let layout = window.text_system().layout_line(
        text,
        px(run.font_size),
        &[TextRun {
            len: text.len(),
            font: gpui_font_descriptor(&run.font),
            color: Hsla::default(),
            ..TextRun::default()
        }],
        None,
    );
    let actual = layout
        .runs
        .iter()
        .flat_map(|shaped_run| shaped_run.glyphs.iter().map(|glyph| glyph.id.0));
    let expected = run.glyphs.iter().map(|glyph| glyph.id);
    let result = font_id == layout.runs.first()?.font_id && glyph_ids_match(expected, actual);
    results
        .lock()
        .expect("glyph validation cache lock poisoned")
        .insert(key, result);
    Some(result)
}

fn glyph_validation_key(window: &Window, run: &ParleyPaintRun, text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    (std::ptr::from_ref(window.text_system()) as usize).hash(&mut hasher);
    run.font.instance_key().hash(&mut hasher);
    run.font_size.to_bits().hash(&mut hasher);
    run.is_rtl.hash(&mut hasher);
    text.hash(&mut hasher);
    for glyph in &run.glyphs {
        glyph.id.hash(&mut hasher);
    }
    hasher.finish()
}

fn glyph_ids_match(
    expected: impl IntoIterator<Item = u32>,
    actual: impl IntoIterator<Item = u32>,
) -> bool {
    expected.into_iter().eq(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_status_never_calls_variable_collection_or_synthesis_exact() {
        assert_eq!(
            gpui_font_bridge_status(0, &[], false, false),
            GpuiFontBridgeStatus::FamilyResolutionUnverified
        );
        assert_eq!(
            gpui_font_bridge_status(0, &[], false, true),
            GpuiFontBridgeStatus::ExactCandidate
        );
        assert_eq!(
            gpui_font_bridge_status(1, &[], false, true),
            GpuiFontBridgeStatus::CollectionFaceUnsupported
        );
        assert_eq!(
            gpui_font_bridge_status(0, &[1], false, true),
            GpuiFontBridgeStatus::VariableInstanceUnsupported
        );
        assert_eq!(
            gpui_font_bridge_status(0, &[], true, true),
            GpuiFontBridgeStatus::SynthesisUnsupported
        );
    }

    #[test]
    fn glyph_validation_requires_exact_order_and_count() {
        assert!(glyph_ids_match([1, 2, 3], [1, 2, 3]));
        assert!(!glyph_ids_match([1, 2, 3], [1, 3, 2]));
        assert!(!glyph_ids_match([1, 2], [1, 2, 3]));
    }

    #[test]
    fn only_validated_exact_candidate_uses_gpui_glyph_atlas() {
        assert_eq!(
            glyph_paint_path(GpuiFontBridgeStatus::ExactCandidate, Some(true)),
            GlyphPaintPath::GpuiGlyphAtlas
        );
        for validation in [Some(false), None] {
            assert_eq!(
                glyph_paint_path(GpuiFontBridgeStatus::ExactCandidate, validation),
                GlyphPaintPath::ExactRasterImageAtlas
            );
        }
        for status in [
            GpuiFontBridgeStatus::CollectionFaceUnsupported,
            GpuiFontBridgeStatus::VariableInstanceUnsupported,
            GpuiFontBridgeStatus::SynthesisUnsupported,
            GpuiFontBridgeStatus::FamilyResolutionUnverified,
        ] {
            assert_eq!(
                glyph_paint_path(status, Some(true)),
                GlyphPaintPath::ExactRasterImageAtlas
            );
        }
    }

    #[test]
    fn exact_raster_report_preserves_first_failing_font_identity() {
        let mut report = ParleyPaintReport::default();
        report.record_exact_raster_error(ExactRasterErrorKind::InvalidFaceIndex, 41, 2, 77);
        report.record_exact_raster_error(ExactRasterErrorKind::RasterizationFailed, 42, 3, 78);

        assert_eq!(
            report.first_exact_raster_error_kind,
            Some(ExactRasterErrorKind::InvalidFaceIndex)
        );
        assert_eq!(report.first_exact_raster_error_blob_id, Some(41));
        assert_eq!(report.first_exact_raster_error_face_index, Some(2));
        assert_eq!(report.first_exact_raster_error_glyph_id, Some(77));
    }
}
