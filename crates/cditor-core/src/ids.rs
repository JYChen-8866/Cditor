use serde::{Deserialize, Serialize};

pub type DocumentId = u64;
pub type BlockId = u64;
pub type CollectionId = u64;
pub type CollectionRecordId = u64;
pub type PropertyId = u64;
pub type ViewId = u64;
pub type CommentThreadId = u64;
pub type CommentId = u64;
pub type AssetId = u64;
pub type ActorId = u64;
pub type DeviceId = u64;
pub type OperationId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceId {
    Block(BlockId),
    TableCell {
        block_id: BlockId,
        row: usize,
        column: usize,
    },
    ImageCaption {
        block_id: BlockId,
    },
    CollectionTitle {
        block_id: BlockId,
    },
    Ephemeral {
        owner_id: u64,
        local_id: u64,
    },
}

impl SurfaceId {
    pub const fn block_id(self) -> Option<BlockId> {
        match self {
            Self::Block(block_id)
            | Self::TableCell { block_id, .. }
            | Self::ImageCaption { block_id }
            | Self::CollectionTitle { block_id } => Some(block_id),
            Self::Ephemeral { .. } => None,
        }
    }
}
