use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};
use cditor_text::{
    FontFaceKey, FontInstanceKey, FontSynthesisKey, TextFontSlant, TextLayoutInput,
    TextLayoutOptions, TextLayoutSurfaceId, TextLineHeight, TextPaintFont, TextStyleConfig,
    TextTheme, build_text_layout, register_font_data,
};

use super::*;

#[test]
fn mask_conversion_is_premultiplied_bgra() {
    assert_eq!(
        mask_to_premultiplied_bgra(&[0, 128, 255], 0x804020),
        [0, 0, 0, 0, 16, 32, 64, 128, 32, 64, 128, 255]
    );
}

#[test]
fn color_conversion_is_premultiplied_bgra() {
    assert_eq!(
        rgba_to_premultiplied_bgra(vec![128, 64, 32, 128]),
        [16, 32, 64, 128]
    );
}

#[test]
fn real_colrv1_glyph_rasterizes_to_chromatic_pixels() {
    let data = color_font_fixture_data();
    let instance = FontInstanceKey::new(
        FontFaceKey::new(81, data.len(), 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = ExactRasterKey {
        font: instance.clone(),
        glyph_id: 9,
        device_font_size_bits: 64.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0x37352f,
        color: true,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };

    let value = rasterize_uncached(
        &key,
        RasterFontSource {
            data,
            blob_id: 81,
            face_index: 0,
            instance: &instance,
        },
        &mut ScaleContext::new(),
    )
    .expect("COLRv1 glyph should rasterize through its color outline");
    let bytes = cache_value_bytes(&value);

    assert!(!bytes.is_empty());
    assert!(
        bytes
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0 && (pixel[0] != pixel[1] || pixel[1] != pixel[2])),
        "COLRv1 gradient must retain chromatic pixels after BGRA conversion"
    );
}

#[test]
fn synthesized_bold_color_glyph_uses_versioned_raster_embolden() {
    let data = color_font_fixture_data();
    let plain_instance = FontInstanceKey::new(
        FontFaceKey::new(82, data.len(), 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let bold_instance = FontInstanceKey::new(
        FontFaceKey::new(82, data.len(), 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), true, None),
    );
    let key = |instance: &FontInstanceKey| ExactRasterKey {
        font: instance.clone(),
        glyph_id: 9,
        device_font_size_bits: 64.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0x37352f,
        color: true,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };
    let raster = |instance: &FontInstanceKey| {
        rasterize_uncached(
            &key(instance),
            RasterFontSource {
                data,
                blob_id: 82,
                face_index: 0,
                instance,
            },
            &mut ScaleContext::new(),
        )
        .expect("color synthesis must produce a deterministic raster")
    };
    let plain = raster(&plain_instance);
    let bold = raster(&bold_instance);

    assert_ne!(cache_value_bytes(&plain), cache_value_bytes(&bold));
    let plain_placement = cache_value_placement(&plain);
    let bold_placement = cache_value_placement(&bold);
    assert!(bold_placement.width > plain_placement.width);
    assert!(bold_placement.height > plain_placement.height);
}

#[test]
fn cache_evicts_by_entry_and_byte_budget() {
    let mut cache = ExactRasterCache::new(1, 4);
    let instance = FontInstanceKey::new(
        FontFaceKey::new(1, 1, 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = |glyph_id| ExactRasterKey {
        font: instance.clone(),
        glyph_id,
        device_font_size_bits: 16.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };
    assert!(
        cache
            .insert(key(1), ExactRasterCacheValue::Empty)
            .is_empty()
    );
    assert!(
        cache
            .insert(key(2), ExactRasterCacheValue::Empty)
            .is_empty()
    );

    assert!(cache.get(&key(1)).is_none());
    assert!(cache.get(&key(2)).is_some());
    assert_eq!(cache.stats().evictions, 1);
}

#[test]
fn cache_returns_the_evicted_render_image_for_atlas_retirement() {
    let mut cache = ExactRasterCache::new(1, 1024);
    let instance = FontInstanceKey::new(
        FontFaceKey::new(2, 1, 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = |glyph_id| ExactRasterKey {
        font: instance.clone(),
        glyph_id,
        device_font_size_bits: 16.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };
    let image = |width| Arc::new(RenderImage::new([Frame::new(RgbaImage::new(width, 1))]));
    let value = |image: Arc<RenderImage>| {
        ExactRasterCacheValue::Glyph(Arc::new(ExactRasterGlyph {
            image,
            placement: RasterPlacement {
                left: 0,
                top: 0,
                width: 1,
                height: 1,
            },
            estimated_bytes: 4,
        }))
    };
    let first = image(1);
    let second = image(2);

    assert!(cache.insert(key(1), value(first.clone())).is_empty());
    let retired = cache.insert(key(2), value(second));

    assert_eq!(retired.len(), 1);
    assert!(Arc::ptr_eq(&retired[0], &first));
}

#[test]
fn quantization_matches_four_horizontal_subpixels() {
    let quantized = quantize_device_origin(10.26, SUBPIXEL_VARIANTS_X);
    assert_eq!(quantized.integer, 10);
    assert_eq!(quantized.subpixel, 1);
}

#[test]
fn variable_coordinates_change_exact_raster_pixels() {
    clear_exact_raster_cache();
    register_fixture_font();
    let thin = paint_font_fixture("'wght' 100", TextFontSlant::Normal);
    let black = paint_font_fixture("'wght' 900", TextFontSlant::Normal);
    assert_ne!(
        thin.0.instance_key().normalized_coords(),
        black.0.instance_key().normalized_coords()
    );

    let thin_image = raster_fixture(&thin);
    let black_image = raster_fixture(&black);

    assert!(!thin_image.is_empty());
    assert!(!black_image.is_empty());
    assert_ne!(thin_image, black_image);
}

#[test]
fn faux_skew_changes_exact_raster_pixels() {
    clear_exact_raster_cache();
    register_fixture_font();
    let normal = paint_font_fixture("'wght' 450", TextFontSlant::Normal);
    let italic = paint_font_fixture("'wght' 450", TextFontSlant::Italic);
    assert!(
        italic.0.instance_key().synthesis().skew().is_some(),
        "fixture must exercise faux skew"
    );

    assert_ne!(raster_fixture(&normal), raster_fixture(&italic));
}

#[test]
fn generated_ttc_face_index_one_is_rasterized_directly() {
    let ttf = fixture_font_data();
    let ttc = duplicate_ttf_as_ttc(&ttf);
    let face = FontRef::from_index(&ttc, 1).expect("generated collection has face one");
    let glyph_id = face.charmap().map('W');
    assert_ne!(glyph_id, 0);
    let instance = FontInstanceKey::new(
        FontFaceKey::new(77, ttc.len(), 1),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = ExactRasterKey {
        font: instance.clone(),
        glyph_id: u32::from(glyph_id),
        device_font_size_bits: 32.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0x37352f,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };
    let value = rasterize_uncached(
        &key,
        RasterFontSource {
            data: &ttc,
            blob_id: 77,
            face_index: 1,
            instance: &instance,
        },
        &mut ScaleContext::new(),
    )
    .expect("face one rasterizes");

    assert!(!cache_value_bytes(&value).is_empty());
}

#[test]
fn invalid_collection_face_is_explicit_error() {
    let ttf = fixture_font_data();
    let instance = FontInstanceKey::new(
        FontFaceKey::new(78, ttf.len(), 1),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = ExactRasterKey {
        font: instance.clone(),
        glyph_id: 1,
        device_font_size_bits: 16.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };

    assert_eq!(
        rasterize_uncached(
            &key,
            RasterFontSource {
                data: &ttf,
                blob_id: 78,
                face_index: 1,
                instance: &instance,
            },
            &mut ScaleContext::new(),
        )
        .err(),
        Some(ExactRasterError::InvalidFaceIndex(1))
    );
}

#[test]
fn unsupported_nonblank_glyph_is_explicit_error() {
    let ttf = fixture_font_data();
    let instance = FontInstanceKey::new(
        FontFaceKey::new(79, ttf.len(), 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = ExactRasterKey {
        font: instance.clone(),
        glyph_id: u32::from(u16::MAX),
        device_font_size_bits: 16.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };

    assert_eq!(
        rasterize_uncached(
            &key,
            RasterFontSource {
                data: &ttf,
                blob_id: 79,
                face_index: 0,
                instance: &instance,
            },
            &mut ScaleContext::new(),
        )
        .err(),
        Some(ExactRasterError::RasterizationFailed(u32::from(u16::MAX)))
    );
}

#[test]
fn space_glyph_is_a_valid_empty_raster() {
    let ttf = fixture_font_data();
    let font = FontRef::from_index(&ttf, 0).unwrap();
    let space = font.charmap().map(' ');
    let instance = FontInstanceKey::new(
        FontFaceKey::new(80, ttf.len(), 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = ExactRasterKey {
        font: instance.clone(),
        glyph_id: u32::from(space),
        device_font_size_bits: 16.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };

    assert!(matches!(
        rasterize_uncached(
            &key,
            RasterFontSource {
                data: &ttf,
                blob_id: 80,
                face_index: 0,
                instance: &instance,
            },
            &mut ScaleContext::new(),
        )
        .unwrap(),
        ExactRasterCacheValue::Empty
    ));
}

#[test]
fn repeated_exact_raster_uses_bounded_cache() {
    clear_exact_raster_cache();
    register_fixture_font();
    let fixture = paint_font_fixture("'wght' 450", TextFontSlant::Normal);
    let key = raster_key(&fixture);
    let source = raster_source(&fixture);

    let (_, first_hit, first_retired) = cached_or_rasterize(key.clone(), source).unwrap();
    let (_, second_hit, second_retired) = cached_or_rasterize(key, source).unwrap();
    let stats = exact_raster_cache_stats();

    assert!(!first_hit);
    assert!(second_hit);
    assert!(first_retired.is_empty());
    assert!(second_retired.is_empty());
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert!(stats.estimated_bytes > 0);
}

fn register_fixture_font() {
    let families = register_font_data(fixture_font_data()).unwrap();
    assert!(
        families
            .iter()
            .any(|family| family.name == "League Spartan")
    );
}

fn fixture_font_data() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../cditor-text/tests/fixtures/text-layout/v1/fonts/LeagueSpartan[wght].ttf"),
    )
    .unwrap()
}

fn color_font_fixture_data() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../cditor-text/tests/fixtures/text-layout/v1/fonts/COLRv1StaticTestGlyphs.ttf"
    ))
}

fn paint_font_fixture(variations: &str, slant: TextFontSlant) -> (TextPaintFont, u32, f32, u32) {
    let input = TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(90_000),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain("W")],
        width_px: 200.0,
        theme_version: 1,
        font_version: 1,
    };
    let layout = build_text_layout(
        &input,
        TextTheme::default(),
        &TextLayoutOptions {
            width: Some(200.0),
            quantize: false,
            base_text_color: 0x37352f,
            base_style: TextStyleConfig {
                font_family: "League Spartan".to_owned(),
                font_size: 32.0,
                font_weight: 100.0,
                font_variations: variations.to_owned(),
                font_slant: slant,
                line_height: TextLineHeight::Absolute(40.0),
                ..TextStyleConfig::default()
            },
            ..TextLayoutOptions::default()
        },
    );
    let run = layout
        .paint_plan()
        .runs
        .iter()
        .find(|run| !run.glyphs.is_empty())
        .unwrap();
    (
        run.font.clone(),
        run.glyphs[0].id,
        run.font_size,
        run.brush.foreground,
    )
}

fn raster_fixture(fixture: &(TextPaintFont, u32, f32, u32)) -> Vec<u8> {
    let (value, _, _) = cached_or_rasterize(raster_key(fixture), raster_source(fixture)).unwrap();
    cache_value_bytes(&value)
}

fn raster_key(fixture: &(TextPaintFont, u32, f32, u32)) -> ExactRasterKey {
    ExactRasterKey {
        font: fixture.0.instance_key().clone(),
        glyph_id: fixture.1,
        device_font_size_bits: fixture.2.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: fixture.3,
        color: false,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    }
}

fn raster_source(fixture: &(TextPaintFont, u32, f32, u32)) -> RasterFontSource<'_> {
    RasterFontSource {
        data: fixture.0.data(),
        blob_id: fixture.0.blob_id(),
        face_index: fixture.0.face_index(),
        instance: fixture.0.instance_key(),
    }
}

fn cache_value_bytes(value: &ExactRasterCacheValue) -> Vec<u8> {
    match value {
        ExactRasterCacheValue::Glyph(glyph) => glyph.image.as_bytes(0).unwrap().to_vec(),
        ExactRasterCacheValue::Empty => Vec::new(),
    }
}

fn cache_value_placement(value: &ExactRasterCacheValue) -> RasterPlacement {
    match value {
        ExactRasterCacheValue::Glyph(glyph) => glyph.placement,
        ExactRasterCacheValue::Empty => panic!("fixture glyph must produce pixels"),
    }
}

fn duplicate_ttf_as_ttc(ttf: &[u8]) -> Vec<u8> {
    let header_len = 20usize;
    let first_offset = align_four(header_len);
    let first = relocate_ttf(ttf, first_offset);
    let second_offset = align_four(first_offset + first.len());
    let second = relocate_ttf(ttf, second_offset);
    let mut ttc = vec![0; second_offset + second.len()];
    ttc[0..4].copy_from_slice(b"ttcf");
    ttc[4..8].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    ttc[8..12].copy_from_slice(&2u32.to_be_bytes());
    ttc[12..16].copy_from_slice(&(first_offset as u32).to_be_bytes());
    ttc[16..20].copy_from_slice(&(second_offset as u32).to_be_bytes());
    ttc[first_offset..first_offset + first.len()].copy_from_slice(&first);
    ttc[second_offset..second_offset + second.len()].copy_from_slice(&second);
    ttc
}

fn relocate_ttf(ttf: &[u8], base_offset: usize) -> Vec<u8> {
    let mut relocated = ttf.to_vec();
    let table_count = u16::from_be_bytes([relocated[4], relocated[5]]) as usize;
    for table_index in 0..table_count {
        let offset_position = 12 + table_index * 16 + 8;
        let offset = u32::from_be_bytes(
            relocated[offset_position..offset_position + 4]
                .try_into()
                .unwrap(),
        );
        relocated[offset_position..offset_position + 4]
            .copy_from_slice(&(offset + base_offset as u32).to_be_bytes());
    }
    relocated
}

fn align_four(value: usize) -> usize {
    (value + 3) & !3
}

#[cfg(target_os = "macos")]
#[test]
fn apple_color_emoji_glyph_rasterizes_through_color_bitmap_sources() {
    // The editor routes color glyph runs (emoji) through the exact raster
    // atlas because GPUI's macOS font resolution skips Apple Color Emoji
    // (it has no 'm' glyph). This guards the swash color-bitmap path that
    // the routed runs depend on.
    const APPLE_COLOR_EMOJI: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";
    if !std::path::Path::new(APPLE_COLOR_EMOJI).exists() {
        return;
    }
    let data = std::fs::read(APPLE_COLOR_EMOJI).unwrap();
    let instance = FontInstanceKey::new(
        FontFaceKey::new(83, data.len(), 0),
        Vec::new(),
        FontSynthesisKey::new(Vec::new(), false, None),
    );
    let key = ExactRasterKey {
        font: instance.clone(),
        // U+1F600 grinning face resolves to this glyph id on current macOS.
        glyph_id: 2096,
        device_font_size_bits: 64.0f32.to_bits(),
        subpixel_x: 0,
        subpixel_y: 0,
        foreground: 0x37352f,
        color: true,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };
    let value = rasterize_uncached(
        &key,
        RasterFontSource {
            data: &data,
            blob_id: 83,
            face_index: 0,
            instance: &instance,
        },
        &mut ScaleContext::new(),
    )
    .expect("Apple Color Emoji glyph should rasterize through its color bitmap");

    let bytes = cache_value_bytes(&value);
    assert!(!bytes.is_empty(), "emoji raster must not be empty");
    assert!(
        bytes.chunks_exact(4).any(|pixel| pixel[3] > 0),
        "emoji raster must contain visible pixels"
    );
}
