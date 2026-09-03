use std::{ops::Range, time::Duration};

use cditor_core::{
    ids::{BlockId, DocumentId},
    rich_text::{BlockAttrs, BlockPayload, PageIcon, RichBlockKind},
};
pub use cditor_editor_protocol::command::BlockInput;

pub const CURRENT_DOCUMENT_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInfo {
    pub document_id: DocumentId,
    pub title: Option<String>,
    pub title_from_heading: bool,
    pub icon: Option<PageIcon>,
    pub revision: u64,
    pub block_count: usize,
    pub readonly: bool,
}

/// Plain-text statistics for the currently loaded document content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStatistics {
    /// Number of whitespace-separated words in the document's plain text.
    pub word_count: usize,
    /// Number of lines in the document's plain text.
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSnapshot {
    pub schema_version: u32,
    pub document: DocumentInfo,
    pub blocks: Vec<BlockSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DocumentSource {
    Empty,
    Snapshot(DocumentSnapshot),
    PostgreSql { document_id: DocumentId },
    Markdown(String),
    Json(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosePolicy {
    RejectIfDirty,
    SaveThenClose,
    DiscardChanges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReport {
    pub revision: u64,
    pub saved_blocks: usize,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseGuard {
    pub dirty: bool,
    pub saving: bool,
    pub failed_operations: usize,
    pub local_failure: Option<SaveFailure>,
    pub requires_recovery_export: bool,
    pub can_close_safely: bool,
}

/// Fail-closed snapshot used by hosts that temporarily release an inactive
/// editor runtime while retaining its tab metadata.
///
/// This is stricter than [`CloseGuard`]: an active host, IME composition,
/// non-collapsed selection, or a busy synchronous session also prevents
/// hibernation. Hosts must still flush persistent storage before releasing the
/// runtime and sample this guard again after that flush.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HibernationGuard {
    pub ready: bool,
    pub loading: bool,
    pub load_failed: bool,
    /// The current document can be reconstructed from its persistent backend
    /// after the live runtime is released.
    pub durable_storage: bool,
    /// The host must await `sdk_flush` before releasing this runtime. Clean
    /// readonly documents are durably reloadable but cannot accept a flush.
    pub flush_required: bool,
    pub host_active: bool,
    pub dirty: bool,
    pub saving: bool,
    pub conflict: bool,
    pub failed_operations: usize,
    pub requires_recovery_export: bool,
    pub can_close_safely: bool,
    pub composing: bool,
    pub selected: bool,
    pub runtime_busy: bool,
    pub can_hibernate_after_flush: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFailureKind {
    Busy,
    CapacityExhausted,
    PermissionDenied,
    Conflict,
    Corruption,
    Timeout,
    Io,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFailure {
    pub kind: SaveFailureKind,
    pub message: String,
    pub retryable: bool,
    pub requires_recovery_export: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryExport {
    pub document_id: DocumentId,
    pub revision: u64,
    pub transaction_count: usize,
    pub suggested_file_name: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveStatus {
    DirtyMemory,
    SavingLocal,
    LocallySaved,
    Syncing,
    Synced,
    FailedLocal(SaveFailure),
    Failed(String),
    Readonly,
}

impl SaveStatus {
    pub const fn is_blocking_close(&self) -> bool {
        matches!(
            self,
            Self::DirtyMemory | Self::SavingLocal | Self::FailedLocal(_) | Self::Failed(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextOffset {
    Utf8Bytes(usize),
    Utf16CodeUnits(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Affinity {
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentPosition {
    pub block_id: BlockId,
    pub offset: TextOffset,
    pub affinity: Affinity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentSelection {
    pub anchor: DocumentPosition,
    pub head: DocumentPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDecoration {
    pub block_id: BlockId,
    pub content_version: u64,
    pub byte_range: Range<usize>,
    pub current: bool,
}

impl DocumentSelection {
    pub const fn caret(position: DocumentPosition) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockSnapshot {
    pub id: BlockId,
    pub parent_id: Option<BlockId>,
    pub depth: u16,
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayload,
    pub content_version: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockPatch {
    pub kind: Option<RichBlockKind>,
    pub attrs: Option<BlockAttrs>,
    pub payload: Option<BlockPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRange {
    pub indices: Range<usize>,
}

impl BlockRange {
    pub fn new(indices: Range<usize>) -> Self {
        Self { indices }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPosition {
    DocumentStart,
    DocumentEnd,
    Before(BlockId),
    After(BlockId),
    FirstChildOf(BlockId),
    LastChildOf(BlockId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAlignment {
    Start,
    Center,
    End,
    Nearest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_selection_keeps_offset_units_explicit() {
        let position = DocumentPosition {
            block_id: 7,
            offset: TextOffset::Utf16CodeUnits(4),
            affinity: Affinity::Downstream,
        };

        assert_eq!(DocumentSelection::caret(position).head, position);
    }

    #[test]
    fn failed_save_status_blocks_close() {
        assert!(SaveStatus::DirtyMemory.is_blocking_close());
        assert!(SaveStatus::SavingLocal.is_blocking_close());
        assert!(!SaveStatus::LocallySaved.is_blocking_close());
        assert!(!SaveStatus::Syncing.is_blocking_close());
        assert!(!SaveStatus::Synced.is_blocking_close());
        assert!(
            SaveStatus::FailedLocal(SaveFailure {
                kind: SaveFailureKind::CapacityExhausted,
                message: "disk full".to_owned(),
                retryable: true,
                requires_recovery_export: true,
            })
            .is_blocking_close()
        );
        assert!(SaveStatus::Failed("offline".to_owned()).is_blocking_close());
        assert!(!SaveStatus::Readonly.is_blocking_close());
    }
}
