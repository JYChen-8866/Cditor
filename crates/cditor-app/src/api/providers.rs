use std::{fmt, path::PathBuf};

use async_trait::async_trait;
pub use cditor_ai::{AiProvider, AiProviderError, AiProviderRequest as AiRequest, AiTaskKind};
pub use cditor_core::rich_text::AssetRef;
use ding_board::Scene as WhiteboardScene;

use super::command::{CommandDescriptor, SlashItem, ToolbarItem};

pub type AiRequestId = u64;
pub type WhiteboardId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetInput {
    pub name: String,
    pub media_type: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    pub reference: AssetRef,
    pub local_path: Option<PathBuf>,
    pub bytes: Option<Vec<u8>>,
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
    async fn import(&self, input: AssetInput) -> Result<AssetRef, AssetError>;
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

pub trait ThemeProvider: Send + Sync {
    fn theme(&self) -> crate::gui::GuiTheme;
    fn version(&self) -> u64;
}

pub trait TranslationProvider: Send + Sync {
    fn translate(&self, locale: &str, key: &str) -> Option<String>;
}

pub trait WhiteboardProvider: Send + Sync {
    fn create_scene(&self) -> WhiteboardScene;
    fn load_scene(&self, id: WhiteboardId) -> WhiteboardScene;
    fn save_scene(&self, scene: WhiteboardScene);
}

pub trait CditorExtension: Send + Sync {
    fn commands(&self) -> Vec<CommandDescriptor>;
    fn slash_items(&self) -> Vec<SlashItem>;
    fn toolbar_items(&self) -> Vec<ToolbarItem>;
}
