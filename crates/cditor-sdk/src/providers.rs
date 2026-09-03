use std::{fmt, fs::File, io::Read, path::PathBuf};

use async_trait::async_trait;
pub use cditor_ai::{AiProvider, AiProviderError, AiProviderRequest as AiRequest, AiTaskKind};
use cditor_core::edit::AssetSnapshot;
pub use cditor_core::rich_text::AssetRef;

use super::command::{CommandDescriptor, SlashItem, ToolbarItem};

pub type AiRequestId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetInput {
    pub name: String,
    pub media_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// A file-backed asset import request. Providers that can stream from disk
/// should override [`AssetProvider::import_file`] so large media does not have
/// to be materialized as a second `Vec<u8>` in the editor process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFileInput {
    pub name: String,
    pub media_type: Option<String>,
    pub path: PathBuf,
}

const DEFAULT_ASSET_FILE_IMPORT_LIMIT: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    pub reference: AssetRef,
    pub local_path: Option<PathBuf>,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedAsset {
    pub reference: AssetRef,
    pub snapshot: AssetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDescriptor {
    pub reference: AssetRef,
    pub block_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetError {
    pub message: String,
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AssetError {}

#[async_trait]
pub trait AssetProvider: Send + Sync {
    async fn import(&self, input: AssetInput) -> Result<ImportedAsset, AssetError>;

    /// Import an asset directly from a local file when possible. The default
    /// implementation preserves compatibility for providers that only accept
    /// byte buffers; production providers should override it for large media.
    async fn import_file(&self, input: AssetFileInput) -> Result<ImportedAsset, AssetError> {
        let metadata = std::fs::metadata(&input.path).map_err(|error| AssetError {
            message: error.to_string(),
        })?;
        if !metadata.is_file() {
            return Err(AssetError {
                message: "asset path is not a file".into(),
            });
        }
        if metadata.len() > DEFAULT_ASSET_FILE_IMPORT_LIMIT {
            return Err(AssetError {
                message: "asset exceeds the 512 MiB limit".into(),
            });
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&input.path)
            .map_err(|error| AssetError {
                message: error.to_string(),
            })?
            .take(DEFAULT_ASSET_FILE_IMPORT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| AssetError {
                message: error.to_string(),
            })?;
        if bytes.len() as u64 > DEFAULT_ASSET_FILE_IMPORT_LIMIT {
            return Err(AssetError {
                message: "asset exceeds the 512 MiB limit".into(),
            });
        }
        self.import(AssetInput {
            name: input.name,
            media_type: input.media_type,
            bytes,
        })
        .await
    }

    async fn resolve(&self, asset: &AssetRef) -> Result<ResolvedAsset, AssetError>;
    async fn delete(&self, asset: &AssetRef) -> Result<(), AssetError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePickerRequest {
    pub request_id: u64,
    pub accepted_media_types: Vec<String>,
    pub allow_multiple: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuContext {
    pub block_id: Option<u64>,
    pub selected_text: Option<String>,
}

pub trait CditorHostDelegate: Send + Sync {
    fn open_link(&self, url: &str);
    fn open_file(&self, asset: &AssetRef);
    fn request_file_picker(&self, request: FilePickerRequest);
    fn show_context_menu(&self, context: MenuContext);
}

pub trait TranslationProvider: Send + Sync {
    fn translate(&self, locale: &str, key: &str) -> Option<String>;
}

pub trait CditorExtension: Send + Sync {
    fn commands(&self) -> Vec<CommandDescriptor>;
    fn slash_items(&self) -> Vec<SlashItem>;
    fn toolbar_items(&self) -> Vec<ToolbarItem>;
}
