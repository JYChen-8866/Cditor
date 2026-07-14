use cditor_core::ids::DocumentId;

use super::{
    document::{DocumentInfo, DocumentSelection},
    error::CditorError,
    providers::AssetDescriptor,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOrigin {
    Local,
    Host,
    Import,
    Undo,
    Redo,
    Ai,
    Remote,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CditorEvent {
    LoadStarted { document_id: Option<DocumentId> },
    LoadProgress { loaded: usize, total: Option<usize> },
    Ready { document: DocumentInfo },
    LoadFailed { error: CditorError },
    ContentChanged { revision: u64, origin: ChangeOrigin },
    SelectionChanged { selection: DocumentSelection },
    FocusChanged { focused: bool },
    SaveStarted { revision: u64 },
    SaveSucceeded { revision: u64 },
    SaveFailed { revision: u64, error: CditorError },
    DirtyChanged { dirty: bool },
    LinkActivated { url: String },
    AssetActivated { asset: AssetDescriptor },
}
