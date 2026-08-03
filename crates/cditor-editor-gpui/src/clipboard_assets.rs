use std::path::{Path, PathBuf};

use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

use cditor_core::rich_text::ImagePayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImageAsset {
    bytes: Option<Vec<u8>>,
    path: Option<PathBuf>,
    pub name: String,
    pub media_type: Option<String>,
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
    let extension = image_extension(image.format);
    let filename = format!("paste-{:016x}.{extension}", image.id());
    Some(ClipboardImageAsset {
        bytes: Some(image.bytes.to_vec()),
        path: None,
        name: filename,
        media_type: Some(image.format.mime_type().to_string()),
    })
}

fn image_asset_from_path(path: &Path, media_type: Option<String>) -> ClipboardImageAsset {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned);
    ClipboardImageAsset {
        bytes: None,
        path: Some(path.to_path_buf()),
        name: name.unwrap_or_else(|| "image".to_owned()),
        media_type,
    }
}

impl ClipboardImageAsset {
    pub fn into_asset_input(self) -> Result<cditor_sdk::providers::AssetInput, String> {
        let bytes = match (self.bytes, self.path) {
            (Some(bytes), _) => bytes,
            (None, Some(path)) => std::fs::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?,
            (None, None) => return Err("clipboard image has no bytes or local path".to_owned()),
        };
        Ok(cditor_sdk::providers::AssetInput {
            name: self.name,
            media_type: self.media_type,
            bytes,
        })
    }

    pub fn into_fallback_payload(self) -> Option<ImagePayload> {
        let path = match (self.bytes, self.path) {
            (_, Some(path)) => path,
            (Some(bytes), None) => {
                let assets_dir = std::env::temp_dir().join("cditor-assets");
                std::fs::create_dir_all(&assets_dir).ok()?;
                let path = assets_dir.join(&self.name);
                if !path.exists() {
                    std::fs::write(&path, bytes).ok()?;
                }
                path
            }
            (None, None) => return None,
        };
        Some(ImagePayload {
            source: path.to_string_lossy().into_owned(),
            alt: self.name,
            caption: String::new().into(),
            display_width_ratio_milli: None,
        })
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
