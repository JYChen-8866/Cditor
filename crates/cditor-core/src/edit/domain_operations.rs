use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{
    ids::{
        ActorId, AssetId, BlockId, CollectionId, CollectionRecordId, CommentId, CommentThreadId,
        PropertyId, SurfaceId, ViewId,
    },
    rich_text::{
        BlockAttrs, BlockPayload, CollectionPropertyPayload, CollectionViewPayload, InlineSpan,
        RichBlockKind, RichTextContent,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TransactionPermission {
    EditDocument,
    ManageCollection,
    Comment,
    ManageAssets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionPermissionSet {
    pub edit_document: bool,
    pub manage_collection: bool,
    pub comment: bool,
    pub manage_assets: bool,
}

impl TransactionPermissionSet {
    pub const FULL_ACCESS: Self = Self {
        edit_document: true,
        manage_collection: true,
        comment: true,
        manage_assets: true,
    };

    pub const READ_ONLY: Self = Self {
        edit_document: false,
        manage_collection: false,
        comment: false,
        manage_assets: false,
    };

    pub const fn allows(self, permission: TransactionPermission) -> bool {
        match permission {
            TransactionPermission::EditDocument => self.edit_document,
            TransactionPermission::ManageCollection => self.manage_collection,
            TransactionPermission::Comment => self.comment,
            TransactionPermission::ManageAssets => self.manage_assets,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextEditOperation {
    ReplaceSpans {
        surface_id: SurfaceId,
        range: Range<usize>,
        old_spans: Vec<InlineSpan>,
        new_spans: Vec<InlineSpan>,
    },
}

impl TextEditOperation {
    pub const fn block_id(&self) -> Option<BlockId> {
        match self {
            Self::ReplaceSpans { surface_id, .. } => surface_id.block_id(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockEditOperation {
    ReplacePayload {
        block_id: BlockId,
        before_kind: RichBlockKind,
        before_payload: BlockPayload,
        after_kind: RichBlockKind,
        after_payload: BlockPayload,
    },
    SetAttrs {
        block_id: BlockId,
        before: BlockAttrs,
        after: BlockAttrs,
    },
}

impl BlockEditOperation {
    pub const fn block_id(&self) -> BlockId {
        match self {
            Self::ReplacePayload { block_id, .. } | Self::SetAttrs { block_id, .. } => *block_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollectionPropertyValue {
    Empty,
    RichText(RichTextContent),
    Number(f64),
    Checkbox(bool),
    Select(Vec<String>),
    Date {
        start: String,
        end: Option<String>,
        time_zone: Option<String>,
    },
    Url(String),
    Email(String),
    Phone(String),
    Relation(Vec<CollectionRecordId>),
    Json(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionRecordSnapshot {
    pub record_id: CollectionRecordId,
    pub collection_id: CollectionId,
    pub page_block_id: Option<BlockId>,
    pub values: Vec<(PropertyId, CollectionPropertyValue)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollectionEditOperation {
    SetTitle {
        block_id: BlockId,
        collection_id: CollectionId,
        before: RichTextContent,
        after: RichTextContent,
    },
    InsertProperty {
        block_id: BlockId,
        collection_id: CollectionId,
        index: usize,
        property: CollectionPropertyPayload,
    },
    DeleteProperty {
        block_id: BlockId,
        collection_id: CollectionId,
        index: usize,
        property: CollectionPropertyPayload,
    },
    UpdateProperty {
        block_id: BlockId,
        collection_id: CollectionId,
        before: CollectionPropertyPayload,
        after: CollectionPropertyPayload,
    },
    InsertView {
        block_id: BlockId,
        collection_id: CollectionId,
        index: usize,
        view: CollectionViewPayload,
    },
    DeleteView {
        block_id: BlockId,
        collection_id: CollectionId,
        index: usize,
        view: CollectionViewPayload,
    },
    UpdateView {
        block_id: BlockId,
        collection_id: CollectionId,
        before: CollectionViewPayload,
        after: CollectionViewPayload,
    },
    InsertRecord {
        block_id: BlockId,
        index: usize,
        record: CollectionRecordSnapshot,
    },
    DeleteRecord {
        block_id: BlockId,
        index: usize,
        record: CollectionRecordSnapshot,
    },
    SetRecordValue {
        block_id: BlockId,
        collection_id: CollectionId,
        record_id: CollectionRecordId,
        property_id: PropertyId,
        before: CollectionPropertyValue,
        after: CollectionPropertyValue,
    },
    SetActiveView {
        block_id: BlockId,
        collection_id: CollectionId,
        before: ViewId,
        after: ViewId,
    },
}

impl CollectionEditOperation {
    pub const fn block_id(&self) -> BlockId {
        match self {
            Self::SetTitle { block_id, .. }
            | Self::InsertProperty { block_id, .. }
            | Self::DeleteProperty { block_id, .. }
            | Self::UpdateProperty { block_id, .. }
            | Self::InsertView { block_id, .. }
            | Self::DeleteView { block_id, .. }
            | Self::UpdateView { block_id, .. }
            | Self::InsertRecord { block_id, .. }
            | Self::DeleteRecord { block_id, .. }
            | Self::SetRecordValue { block_id, .. }
            | Self::SetActiveView { block_id, .. } => *block_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentAnchor {
    pub surface_id: SurfaceId,
    pub range: Range<usize>,
    pub quoted_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentMessageSnapshot {
    pub comment_id: CommentId,
    pub author_id: ActorId,
    pub body: RichTextContent,
    pub created_at_ms: u64,
    pub edited_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentThreadSnapshot {
    pub thread_id: CommentThreadId,
    pub anchor: CommentAnchor,
    pub resolved: bool,
    pub messages: Vec<CommentMessageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentEditOperation {
    CreateThread {
        thread: CommentThreadSnapshot,
    },
    DeleteThread {
        thread: CommentThreadSnapshot,
    },
    AddMessage {
        thread_id: CommentThreadId,
        index: usize,
        message: CommentMessageSnapshot,
    },
    DeleteMessage {
        thread_id: CommentThreadId,
        index: usize,
        message: CommentMessageSnapshot,
    },
    UpdateMessage {
        thread_id: CommentThreadId,
        before: CommentMessageSnapshot,
        after: CommentMessageSnapshot,
    },
    SetResolved {
        thread_id: CommentThreadId,
        before: bool,
        after: bool,
    },
    MoveAnchor {
        thread_id: CommentThreadId,
        before: CommentAnchor,
        after: CommentAnchor,
    },
}

impl CommentEditOperation {
    pub const fn block_id(&self) -> Option<BlockId> {
        match self {
            Self::CreateThread { thread } | Self::DeleteThread { thread } => {
                thread.anchor.surface_id.block_id()
            }
            Self::MoveAnchor { after, .. } => after.surface_id.block_id(),
            Self::AddMessage { .. }
            | Self::DeleteMessage { .. }
            | Self::UpdateMessage { .. }
            | Self::SetResolved { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetState {
    LocalPending,
    Uploading,
    Ready,
    Failed,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSnapshot {
    pub asset_id: AssetId,
    pub file_name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub source: String,
    pub checksum: Option<String>,
    pub state: AssetState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetEditOperation {
    Attach {
        block_id: BlockId,
        asset: AssetSnapshot,
    },
    Detach {
        block_id: BlockId,
        asset: AssetSnapshot,
    },
    Update {
        block_id: BlockId,
        before: AssetSnapshot,
        after: AssetSnapshot,
    },
}

impl AssetEditOperation {
    pub const fn block_id(&self) -> BlockId {
        match self {
            Self::Attach { block_id, .. }
            | Self::Detach { block_id, .. }
            | Self::Update { block_id, .. } => *block_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_domain_operation_has_a_stable_affected_block() {
        let text = TextEditOperation::ReplaceSpans {
            surface_id: SurfaceId::ImageCaption { block_id: 7 },
            range: 0..0,
            old_spans: Vec::new(),
            new_spans: vec![InlineSpan::plain("caption")],
        };
        assert_eq!(text.block_id(), Some(7));

        let asset = AssetEditOperation::Attach {
            block_id: 9,
            asset: AssetSnapshot {
                asset_id: 1,
                file_name: "image.png".to_owned(),
                media_type: "image/png".to_owned(),
                size_bytes: 10,
                source: "asset://1".to_owned(),
                checksum: None,
                state: AssetState::Ready,
            },
        };
        assert_eq!(asset.block_id(), 9);
    }

    #[test]
    fn collection_record_values_round_trip_without_stringly_typed_loss() {
        let value = CollectionPropertyValue::Date {
            start: "2026-07-18".to_owned(),
            end: None,
            time_zone: Some("Asia/Shanghai".to_owned()),
        };
        let encoded = serde_json::to_string(&value).unwrap();
        let decoded: CollectionPropertyValue = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
