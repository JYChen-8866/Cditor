use std::sync::Arc;

#[cfg(target_os = "macos")]
use std::sync::OnceLock;

use parley::{FontContext, fontique::FontInfoOverride};
use peniko::Blob;

pub(super) fn register_outline_fonts(font_cx: &mut FontContext, allow_system_hanzipen: bool) {
    let registered = font_cx.collection.register_fonts(
        Blob::new(Arc::new(crate::font::VIRGIL)),
        Some(FontInfoOverride {
            family_name: Some(crate::font::OUTLINE_VIRGIL_FAMILY),
            weight: Some(parley::FontWeight::NORMAL),
            ..Default::default()
        }),
    );
    debug_assert!(!registered.is_empty(), "failed to register Virgil");

    #[cfg(target_os = "macos")]
    if allow_system_hanzipen {
        register_macos_hanzipen_w5(font_cx);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = allow_system_hanzipen;

}

#[cfg(target_os = "macos")]
fn register_macos_hanzipen_w5(font_cx: &mut FontContext) -> bool {
    const W5_POSTSCRIPT_NAME: &str = "HanziPenSC-W5";

    static HANZIPEN_W5: OnceLock<Option<Arc<memmap2::Mmap>>> = OnceLock::new();
    let font_data = HANZIPEN_W5
        .get_or_init(|| {
            let (path, source_index) = discover_macos_hanzipen_w5(font_cx)?;
            isolate_ttc_face(&path, source_index, W5_POSTSCRIPT_NAME).map(Arc::new)
        })
        .clone();
    let Some(font_data) = font_data else {
        trace_registration("HanziPenSC-W5 unavailable; using bundled CJK fallback");
        return false;
    };
    let registered = font_cx.collection.register_fonts(
        Blob::new(font_data),
        Some(FontInfoOverride {
            family_name: Some(crate::font::OUTLINE_HANZIPEN_FAMILY),
            weight: Some(parley::FontWeight::NORMAL),
            ..Default::default()
        }),
    );
    let exact_single_face = registered.len() == 1
        && registered
            .first()
            .is_some_and(|(_, fonts)| fonts.len() == 1 && fonts[0].index() == 0);
    if exact_single_face {
        trace_registration("registered HanziPenSC-W5 as exact optical regular");
    } else {
        trace_registration("HanziPenSC-W5 registration was not a single face");
    }
    exact_single_face
}

#[cfg(target_os = "macos")]
fn discover_macos_hanzipen_w5(font_cx: &mut FontContext) -> Option<(std::path::PathBuf, u32)> {
    use parley::fontique::SourceKind;

    const SOURCE_FAMILY: &str = "HanziPen SC";
    const W5_POSTSCRIPT_NAME: &str = "HanziPenSC-W5";

    if let Some(family) = font_cx.collection.family_by_name(SOURCE_FAMILY) {
        for font in family.fonts() {
            let SourceKind::Path(path) = font.source().kind() else {
                continue;
            };
            let Some(data) = font.load(Some(&mut font_cx.source_cache)) else {
                continue;
            };
            let Ok(face) = ttf_parser::Face::parse(data.as_ref(), font.index()) else {
                continue;
            };
            if super::font_face_name(&face) == W5_POSTSCRIPT_NAME {
                trace_discovered_face(path, font.index(), "active system family");
                return Some((path.to_path_buf(), font.index()));
            }
        }
    }

    for path in mobile_asset_hanzipen_paths() {
        if let Some(index) = face_index_by_postscript_name(&path, W5_POSTSCRIPT_NAME) {
            trace_discovered_face(&path, index, "macOS MobileAsset");
            return Some((path, index));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn mobile_asset_hanzipen_paths() -> Vec<std::path::PathBuf> {
    const ASSETS_ROOT: &str = "/System/Library/AssetsV2";

    let Ok(categories) = std::fs::read_dir(ASSETS_ROOT) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for category in categories.filter_map(Result::ok) {
        if !category
            .file_name()
            .to_string_lossy()
            .starts_with("com_apple_MobileAsset_Font")
        {
            continue;
        }
        let Ok(assets) = std::fs::read_dir(category.path()) else {
            continue;
        };
        for asset in assets.filter_map(Result::ok) {
            let asset_data = asset.path().join("AssetData");
            let Ok(fonts) = std::fs::read_dir(asset_data) else {
                continue;
            };
            for font in fonts.filter_map(Result::ok) {
                if font
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("Hanzipen.ttc")
                {
                    paths.push(font.path());
                }
            }
        }
    }
    paths.sort();
    paths
}

#[cfg(target_os = "macos")]
fn face_index_by_postscript_name(path: &std::path::Path, expected: &str) -> Option<u32> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: this is a read-only map of a macOS system font used only while
    // discovering its face indices.
    let mapped = unsafe { memmap2::MmapOptions::new().map(&file).ok()? };
    let face_count = ttf_parser::fonts_in_collection(&mapped).unwrap_or(1);
    (0..face_count).find(|index| {
        ttf_parser::Face::parse(&mapped, *index)
            .is_ok_and(|face| super::font_face_name(&face) == expected)
    })
}

#[cfg(target_os = "macos")]
fn trace_discovered_face(path: &std::path::Path, index: u32, source: &str) {
    if super::font_trace_enabled() {
        eprintln!(
            "[cditor][whiteboard][font-registration] discovered HanziPenSC-W5 source={source:?} index={index} path={}",
            path.display(),
        );
    }
}

#[cfg(target_os = "macos")]
fn isolate_ttc_face(
    path: &std::path::Path,
    source_index: u32,
    expected_postscript_name: &str,
) -> Option<memmap2::Mmap> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: the source is a read-only macOS system font. map_copy creates a
    // private COW mapping, so the TTC header edits below never reach the file.
    let mut mapped = unsafe { memmap2::MmapOptions::new().map_copy(&file).ok()? };
    let source_face = ttf_parser::Face::parse(&mapped, source_index).ok()?;
    if super::font_face_name(&source_face) != expected_postscript_name {
        return None;
    }

    select_ttc_face(&mut mapped, source_index)?;
    let mapped = mapped.make_read_only().ok()?;
    if ttf_parser::fonts_in_collection(&mapped) != Some(1) {
        return None;
    }
    let isolated_face = ttf_parser::Face::parse(&mapped, 0).ok()?;
    (super::font_face_name(&isolated_face) == expected_postscript_name).then_some(mapped)
}

#[cfg(target_os = "macos")]
fn select_ttc_face(data: &mut [u8], source_index: u32) -> Option<()> {
    if data.get(0..4)? != b"ttcf" {
        return None;
    }
    let version = u32::from_be_bytes(data.get(4..8)?.try_into().ok()?);
    if !matches!(version, 0x0001_0000 | 0x0002_0000) {
        return None;
    }
    let face_count = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?);
    if source_index >= face_count {
        return None;
    }
    let offsets_end = 12_usize.checked_add((face_count as usize).checked_mul(4)?)?;
    let header_end = offsets_end.checked_add(usize::from(version == 0x0002_0000) * 12)?;
    if header_end > data.len() {
        return None;
    }
    let selected_start = 12_usize.checked_add((source_index as usize).checked_mul(4)?)?;
    let selected_offset: [u8; 4] = data
        .get(selected_start..selected_start + 4)?
        .try_into()
        .ok()?;
    if u32::from_be_bytes(selected_offset) as usize >= data.len() {
        return None;
    }

    data[8..12].copy_from_slice(&1_u32.to_be_bytes());
    data[12..16].copy_from_slice(&selected_offset);
    if version == 0x0002_0000 {
        // With one face the TTC v2 DSIG fields begin immediately at byte 16.
        data[16..28].fill(0);
    }
    Some(())
}

#[cfg(target_os = "macos")]
fn trace_registration(message: &str) {
    if super::font_trace_enabled() {
        eprintln!("[cditor][whiteboard][font-registration] {message}");
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn ttc_header(version: u32) -> Vec<u8> {
        let mut data = vec![0_u8; 512];
        data[0..4].copy_from_slice(b"ttcf");
        data[4..8].copy_from_slice(&version.to_be_bytes());
        data[8..12].copy_from_slice(&3_u32.to_be_bytes());
        data[12..16].copy_from_slice(&128_u32.to_be_bytes());
        data[16..20].copy_from_slice(&256_u32.to_be_bytes());
        data[20..24].copy_from_slice(&384_u32.to_be_bytes());
        data
    }

    #[test]
    fn selects_requested_face_from_ttc_v1_header() {
        let mut data = ttc_header(0x0001_0000);
        select_ttc_face(&mut data, 2).expect("select TTC face");
        assert_eq!(&data[8..12], &1_u32.to_be_bytes());
        assert_eq!(&data[12..16], &384_u32.to_be_bytes());
    }

    #[test]
    fn moves_ttc_v2_dsig_fields_after_single_face_offset() {
        let mut data = ttc_header(0x0002_0000);
        select_ttc_face(&mut data, 1).expect("select TTC face");
        assert_eq!(&data[12..16], &256_u32.to_be_bytes());
        assert_eq!(&data[16..28], &[0; 12]);
    }

    #[test]
    fn rejects_invalid_ttc_headers_and_face_indices() {
        assert!(select_ttc_face(&mut [0; 32], 0).is_none());
        let mut data = ttc_header(0x0001_0000);
        assert!(select_ttc_face(&mut data, 3).is_none());
    }
}
