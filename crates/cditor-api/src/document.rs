use std::{ops::Range, time::Duration};

use cditor_core::{
    ids::{BlockId, DocumentId},
    rich_text::{BlockAttrs, BlockPayload, RichBlockKind},
};

pub const CURRENT_DOCUMENT_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInfo {
    pub document_id: DocumentId,
    pub title: Option<String>,
    pub revision: u64,
    pub block_count: usize,
    pub readonly: bool,
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
    pub can_close_safely: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveStatus {
    Clean,
    Dirty,
    Saving,
    Failed(String),
    Readonly,
}

impl SaveStatus {
    pub const fn is_blocking_close(&self) -> bool {
        matches!(self, Self::Dirty | Self::Saving | Self::Failed(_))
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

#[derive(Debug, Clone, PartialEq)]
pub struct BlockInput {
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayload,
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
        assert!(SaveStatus::Failed("offline".to_owned()).is_blocking_close());
        assert!(!SaveStatus::Readonly.is_blocking_close());
    }
}
