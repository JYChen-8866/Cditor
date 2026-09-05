use cditor_core::ids::DocumentId;

use super::{
    document::{DocumentInfo, DocumentSelection},
    error::CditorError,
    providers::AssetDescriptor,
};

/// 统一变更来源（P4-007）：core 是唯一定义点，SDK 直接复用。
pub use cditor_core::edit::ChangeOrigin;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CditorEvent {
    LoadStarted {
        document_id: Option<DocumentId>,
    },
    LoadProgress {
        loaded: usize,
        total: Option<usize>,
    },
    Ready {
        document: DocumentInfo,
    },
    LoadFailed {
        error: CditorError,
    },
    ContentChanged {
        revision: u64,
        origin: ChangeOrigin,
    },
    DocumentNameChanged {
        name: String,
        revision: u64,
    },
    SelectionChanged {
        selection: DocumentSelection,
    },
    FocusChanged {
        focused: bool,
    },
    SaveStarted {
        revision: u64,
    },
    SaveSucceeded {
        revision: u64,
    },
    SaveFailed {
        revision: u64,
        error: CditorError,
    },
    HistoryHydrationStarted {
        snapshot_id: u64,
        redo: bool,
    },
    HistoryHydrationSucceeded {
        snapshot_id: u64,
        redo: bool,
    },
    HistoryHydrationFailed {
        snapshot_id: u64,
        redo: bool,
        error: CditorError,
    },
    DirtyChanged {
        dirty: bool,
    },
    LinkActivated {
        url: String,
    },
    AssetActivated {
        asset: AssetDescriptor,
    },
}
