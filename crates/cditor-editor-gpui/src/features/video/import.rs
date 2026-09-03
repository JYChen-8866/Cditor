use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use cditor_core::{ids::BlockId, rich_text::VideoPayload};
use cditor_editor_protocol::command::{CditorCommand, CommandSource};
use cditor_sdk::providers::{AssetError, AssetFileInput, AssetProvider};
use gpui::{App, Entity, ExternalPaths, Pixels, Point};

use crate::{editor_view::CditorV2View, interaction::geometry::ProjectedBlockRect};

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi"];
pub(super) const MAX_VIDEO_IMPORT_BYTES: u64 = 512 * 1024 * 1024;

struct ImportedVideo {
    payload: VideoPayload,
    asset: Option<cditor_core::edit::AssetSnapshot>,
}

pub(crate) fn accepts_external_video_paths(paths: &ExternalPaths) -> bool {
    paths.paths().iter().any(|path| is_video_path(path))
}

impl CditorV2View {
    pub(crate) fn handle_external_video_drop(
        &mut self,
        paths: &ExternalPaths,
        position: Point<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        let paths = paths
            .paths()
            .iter()
            .filter(|path| is_video_path(path))
            .cloned()
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return;
        }
        let provider = self.features.asset_provider.clone();
        let document_y = self
            .document_viewport_origin()
            .map(|origin| f64::from(position.y) - origin.y + self.interaction.presented_scroll_top);
        let mut after_block_id = document_y
            .and_then(|document_y| {
                drop_anchor_at_document_y(&self.interaction.projected_block_rects, document_y)
            })
            .or_else(|| {
                self.ready_session()
                    .and_then(|session| session.document_snapshot().ok())
                    .and_then(|snapshot| snapshot.focused_block_id)
            });
        cx.spawn(async move |view, cx| {
            for path in paths {
                let imported = import_video(path, provider.clone()).await;
                let next_anchor = view
                    .update(cx, |view, cx| match imported {
                        Ok(imported) => match view.dispatch_command(
                            CditorCommand::InsertVideoAsset {
                                payload: imported.payload,
                                asset: imported.asset,
                                after_block_id,
                            },
                            CommandSource::Import,
                            cx,
                        ) {
                            Ok(outcome) => outcome.affected_blocks.last().copied(),
                            Err(error) => {
                                show_video_import_error(view, error.to_string(), cx);
                                None
                            }
                        },
                        Err(error) => {
                            show_video_import_error(view, error.to_string(), cx);
                            None
                        }
                    })
                    .ok()
                    .flatten();
                if next_anchor.is_some() {
                    after_block_id = next_anchor;
                }
            }
        })
        .detach();
    }
}

pub(super) fn replace_video_from_path(
    view: Entity<CditorV2View>,
    provider: Option<Arc<dyn AssetProvider>>,
    block_id: BlockId,
    path: PathBuf,
    cx: &mut App,
) {
    if let Err(error) = validate_video_path(&path) {
        view.update(cx, |view, cx| show_video_import_error(view, error, cx));
        return;
    }
    view.update(cx, |view, cx| {
        view.cache
            .video_playbacks
            .set_import_status(block_id, Some("正在导入视频…".into()));
        cx.notify();
    });
    cx.spawn(async move |cx| {
        let imported = import_video(path, provider).await;
        let _ = view.update(cx, |view, cx| match imported {
            Ok(imported) => {
                view.cache.video_playbacks.set_import_status(block_id, None);
                if let Err(error) = view.dispatch_command(
                    CditorCommand::SetVideoSource {
                        block_id,
                        source: imported.payload.source,
                        title: imported.payload.title,
                        media_type: imported.payload.media_type,
                        asset: imported.asset,
                    },
                    CommandSource::Toolbar,
                    cx,
                ) {
                    show_video_import_error(view, error.to_string(), cx);
                }
            }
            Err(error) => {
                view.cache
                    .video_playbacks
                    .set_import_status(block_id, Some("视频导入失败，点击重试".into()));
                show_video_import_error(view, error.to_string(), cx);
            }
        });
    })
    .detach();
}

async fn import_video(
    path: PathBuf,
    provider: Option<Arc<dyn AssetProvider>>,
) -> Result<ImportedVideo, AssetError> {
    validate_video_path(&path).map_err(|message| AssetError { message })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("video.mp4")
        .to_owned();
    let media_type = video_media_type_for_name(&file_name);
    let Some(provider) = provider else {
        return Ok(ImportedVideo {
            payload: VideoPayload {
                source: path.to_string_lossy().into_owned(),
                title: file_name,
                media_type: Some(media_type),
                ..Default::default()
            },
            asset: None,
        });
    };
    let imported = crate::provider_io::import_asset_file(
        provider,
        AssetFileInput {
            name: file_name.clone(),
            media_type: Some(media_type.clone()),
            path,
        },
    )
    .await?;
    Ok(ImportedVideo {
        payload: VideoPayload {
            source: imported.reference.source,
            title: imported.reference.name.unwrap_or(file_name),
            media_type: Some(imported.reference.media_type.unwrap_or(media_type)),
            ..Default::default()
        },
        asset: Some(imported.snapshot),
    })
}

fn validate_video_path(path: &Path) -> Result<(), String> {
    if !is_video_path(path) {
        return Err("请选择视频文件（mp4、mov、webm、mkv、avi）".into());
    }
    let metadata = std::fs::metadata(path).map_err(|error| format!("无法读取视频文件：{error}"))?;
    if !metadata.is_file() {
        return Err("拖入的路径不是视频文件".into());
    }
    if metadata.len() > MAX_VIDEO_IMPORT_BYTES {
        return Err("视频文件不能超过 512 MiB".into());
    }
    Ok(())
}

fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|item| extension.eq_ignore_ascii_case(item))
        })
}

fn video_media_type_for_name(name: &str) -> String {
    match Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        _ => "video/*",
    }
    .to_owned()
}

fn drop_anchor_at_document_y(rects: &[ProjectedBlockRect], document_y: f64) -> Option<BlockId> {
    rects
        .iter()
        .rev()
        .find(|rect| document_y >= rect.document_top)
        .or_else(|| rects.first())
        .map(|rect| rect.block_id)
}

fn show_video_import_error(
    view: &mut CditorV2View,
    error: String,
    cx: &mut gpui::Context<CditorV2View>,
) {
    crate::overlays::show_toast(
        view,
        format!("视频导入失败：{error}"),
        Duration::from_secs(5),
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(block_id: BlockId, top: f64, bottom: f64) -> ProjectedBlockRect {
        ProjectedBlockRect {
            block_id,
            document_top: top,
            document_bottom: bottom,
            ..Default::default()
        }
    }

    #[test]
    fn video_drop_filter_is_case_insensitive_and_rejects_non_video_files() {
        assert!(is_video_path(Path::new("demo.MP4")));
        assert!(is_video_path(Path::new("clip.webm")));
        assert!(is_video_path(Path::new("capture.mkv")));
        assert!(!is_video_path(Path::new("poster.png")));
        assert!(!is_video_path(Path::new("video")));
    }

    #[test]
    fn video_extensions_map_to_specific_media_types() {
        assert_eq!(video_media_type_for_name("demo.mp4"), "video/mp4");
        assert_eq!(video_media_type_for_name("demo.mov"), "video/quicktime");
        assert_eq!(video_media_type_for_name("demo.webm"), "video/webm");
        assert_eq!(video_media_type_for_name("demo.mkv"), "video/x-matroska");
        assert_eq!(video_media_type_for_name("demo.avi"), "video/x-msvideo");
    }

    #[test]
    fn drop_anchor_tracks_the_block_at_or_before_the_pointer() {
        let rects = [rect(10, 100.0, 130.0), rect(20, 150.0, 180.0)];
        assert_eq!(drop_anchor_at_document_y(&rects, 90.0), Some(10));
        assert_eq!(drop_anchor_at_document_y(&rects, 120.0), Some(10));
        assert_eq!(drop_anchor_at_document_y(&rects, 145.0), Some(10));
        assert_eq!(drop_anchor_at_document_y(&rects, 160.0), Some(20));
        assert_eq!(drop_anchor_at_document_y(&rects, 300.0), Some(20));
    }
}
