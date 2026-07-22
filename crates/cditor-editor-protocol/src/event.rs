use cditor_core::{edit::ChangeOrigin, ids::DocumentId};

use crate::{ProtocolError, projection::SelectionProjection};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EditorEvent {
    LoadStarted {
        document_id: Option<DocumentId>,
    },
    LoadProgress {
        loaded: usize,
        total: Option<usize>,
    },
    Ready {
        document_id: DocumentId,
        revision: u64,
    },
    LoadFailed {
        error: ProtocolError,
    },
    ContentChanged {
        revision: u64,
        origin: ChangeOrigin,
    },
    SelectionChanged {
        selection: SelectionProjection,
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
        error: ProtocolError,
    },
    DirtyChanged {
        dirty: bool,
    },
    LinkActivated {
        url: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_is_an_observed_fact_not_a_command() {
        let event = EditorEvent::ContentChanged {
            revision: 9,
            origin: ChangeOrigin::User,
        };
        assert!(matches!(
            event,
            EditorEvent::ContentChanged { revision: 9, .. }
        ));
    }
}
