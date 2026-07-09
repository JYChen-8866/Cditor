pub mod index;
pub mod visible_index;

pub use index::{
    BlockFlags, BlockIndexRecord, BlockKindTag, DocumentIndex, DocumentIndexBuildError,
    DocumentIndexStore,
};
pub use visible_index::{
    ScrollTargetResolution, VisibilityChange, VisibilityUpdate, VisibleDocumentIndex,
    VisibleDocumentIndexError, VisibleScrollTarget,
};
