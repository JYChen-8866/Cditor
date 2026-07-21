pub mod index;
pub mod visible_index;

pub use index::{
    BLOCK_FLAG_FOLDED, BLOCK_FLAG_HAS_STRUCTURAL_CHILDREN, BLOCK_FLAG_LOCKED, BlockFlags,
    BlockIndexRecord, BlockKindTag, DocumentIndex, DocumentIndexBuildError, DocumentIndexStore,
};
pub use visible_index::{
    ScrollTargetResolution, VisibilityChange, VisibilityUpdate, VisibleDocumentIndex,
    VisibleDocumentIndexError, VisibleScrollTarget,
};
