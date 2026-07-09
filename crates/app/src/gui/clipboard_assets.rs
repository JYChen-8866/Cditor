use std::path::Path;

use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

use cditor_core::rich_text::ImagePayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImageAsset {
    pub payload: ImagePayload,
    pub name: Option<String>,
    pub media_type: Option<String>,
    pub size_bytes: Option<u64>,
}

pub fn image_asset_from_clipboard_item(item: &ClipboardItem) -> Option<ClipboardImageAsset> {
    for entry in &item.entries {
        match entry {
            ClipboardEntry::Image(image) => return write_clipboard_image_asset(image),
            ClipboardEntry::ExternalPaths(paths) => {
                for path in &paths.0 {
                    let media_type = media_type_for_path(path);
                    if media_type
                        .as_deref()
                        .is_some_and(|media_type| media_type.starts_with("image/"))
                    {
                        return Some(image_asset_from_path(path, media_type));
                    }
                }
            }
            ClipboardEntry::String(_) => {}
        }
    }
    None
}

fn write_clipboard_image_asset(image: &Image) -> Option<ClipboardImageAsset> {
    let assets_dir = std::env::temp_dir().join("cditor-assets");
    std::fs::create_dir_all(&assets_dir).ok()?;
    let extension = image_extension(image.format);
    let filename = format!("paste-{:016x}.{extension}", image.id());
    let path = assets_dir.join(&filename);
    if !path.exists() {
        std::fs::write(&path, &image.bytes).ok()?;
    }
    Some(ClipboardImageAsset {
        payload: ImagePayload {
            source: path.to_string_lossy().to_string(),
            alt: filename.clone(),
            caption: String::new(),
            display_width_ratio_milli: None,
        },
        name: Some(filename),
        media_type: Some(image.format.mime_type().to_string()),
        size_bytes: Some(image.bytes.len() as u64),
    })
}

fn image_asset_from_path(path: &Path, media_type: Option<String>) -> ClipboardImageAsset {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned);
    let size_bytes = std::fs::metadata(path).ok().map(|metadata| metadata.len());
    ClipboardImageAsset {
        payload: ImagePayload {
            source: path.to_string_lossy().to_string(),
            alt: name.clone().unwrap_or_default(),
            caption: String::new(),
            display_width_ratio_milli: None,
        },
        name,
        media_type,
        size_bytes,
    }
}

fn image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

fn media_type_for_path(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let media_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/vnd.microsoft.icon",
        _ => return None,
    };
    Some(media_type.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_type_recognizes_images() {
        assert_eq!(
            media_type_for_path(Path::new("a.png")),
            Some("image/png".to_owned())
        );
        assert_eq!(media_type_for_path(Path::new("a.txt")), None);
    }
}
