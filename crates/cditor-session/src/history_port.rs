use cditor_core::edit::{EditTransaction, ExternalUndoBlobRef};
use cditor_editor_protocol::{
    ProtocolError, ProtocolErrorCode,
    command::{CommandEnvelope, CommandSource, EditorCommand},
};
use cditor_runtime::DocumentRuntime;

use crate::{CommandDispatchSnapshot, EditorSessionHandle, project_command_dispatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryDirection {
    Undo,
    Redo,
}

impl HistoryDirection {
    fn command(self) -> EditorCommand {
        match self {
            Self::Undo => EditorCommand::Undo,
            Self::Redo => EditorCommand::Redo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryActionSnapshot {
    Applied(CommandDispatchSnapshot),
    HydrationRequired {
        reference: ExternalUndoBlobRef,
        dispatch_error: String,
    },
}

pub fn project_history_action(
    runtime: &mut DocumentRuntime,
    source: CommandSource,
    direction: HistoryDirection,
) -> Result<HistoryActionSnapshot, ProtocolError> {
    match project_command_dispatch(runtime, CommandEnvelope::new(direction.command(), source)) {
        Ok(snapshot) => Ok(HistoryActionSnapshot::Applied(snapshot)),
        Err(error) => {
            let reference = match direction {
                HistoryDirection::Undo => runtime.pending_undo_hydration(),
                HistoryDirection::Redo => runtime.pending_redo_hydration(),
            };
            reference
                .map(|reference| HistoryActionSnapshot::HydrationRequired {
                    reference,
                    dispatch_error: error.message.clone(),
                })
                .ok_or(error)
        }
    }
}

pub fn project_hydrated_history_action(
    runtime: &mut DocumentRuntime,
    reference: &ExternalUndoBlobRef,
    transaction: EditTransaction,
    source: CommandSource,
    direction: HistoryDirection,
) -> Result<CommandDispatchSnapshot, ProtocolError> {
    if !runtime.hydrate_external_undo(reference.snapshot_id, transaction) {
        return Err(ProtocolError::new(
            ProtocolErrorCode::StalePrecondition,
            "undo reference changed while hydrating",
        )
        .with_document(runtime.document_id()));
    }
    match project_history_action(runtime, source, direction)? {
        HistoryActionSnapshot::Applied(snapshot) => Ok(snapshot),
        HistoryActionSnapshot::HydrationRequired { .. } => Err(ProtocolError::new(
            ProtocolErrorCode::ApplyFailed,
            "hydrated history action still requires external state",
        )
        .with_document(runtime.document_id())),
    }
}

impl EditorSessionHandle {
    pub fn apply_history(
        &self,
        source: CommandSource,
        direction: HistoryDirection,
    ) -> Result<HistoryActionSnapshot, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.readonly {
            return Err(
                ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                    .with_document(session.runtime.document_id()),
            );
        }
        project_history_action(&mut session.runtime, source, direction)
    }
}

#[cfg(test)]
mod tests {
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    #[test]
    fn empty_history_is_a_successful_no_op_snapshot() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        let HistoryActionSnapshot::Applied(snapshot) = handle
            .apply_history(CommandSource::Sdk, HistoryDirection::Undo)
            .unwrap()
        else {
            panic!("empty local history must not request hydration");
        };

        assert!(!snapshot.outcome.changed());
        assert_eq!(snapshot.before_revision, snapshot.revision);
    }

    #[test]
    fn local_history_returns_the_applied_revision_snapshot() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::InsertParagraphAfterFocused,
                CommandSource::Sdk,
            ))
            .unwrap();

        let HistoryActionSnapshot::Applied(snapshot) = handle
            .apply_history(CommandSource::Sdk, HistoryDirection::Undo)
            .unwrap()
        else {
            panic!("resident local history must not request hydration");
        };
        assert!(snapshot.outcome.changed());
        assert!(snapshot.revision > snapshot.before_revision);
    }
}
