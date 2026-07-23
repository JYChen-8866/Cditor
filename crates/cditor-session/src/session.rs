use std::{
    cell::{RefCell, RefMut},
    fmt,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use cditor_core::ids::DocumentId;
use cditor_editor_protocol::{
    ProtocolError, ProtocolErrorCode,
    command::{CommandCatalog, CommandEnvelope, CommandMutability, CommandOutcome},
    projection::ProjectionRequest,
    query::{CommandQuery, DocumentSummary, QueryResult},
};
use cditor_runtime::{
    DocumentRuntime, EditorViewProjection, RealtimeInputError, RealtimeInputOutcome,
    RealtimeInputRequest,
};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(u64);

impl SessionId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub document_id: DocumentId,
    pub revision: u64,
    pub block_count: usize,
    pub readonly: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDispatchSnapshot {
    pub before_revision: u64,
    pub revision: u64,
    pub outcome: CommandOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRealtimeError {
    Protocol(ProtocolError),
    Input(RealtimeInputError),
}

impl fmt::Display for SessionRealtimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(error) => error.fmt(formatter),
            Self::Input(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SessionRealtimeError {}

/// The sole mutable owner of a document runtime.
///
/// It is deliberately not shared by a mutex. The desktop host runs requests
/// synchronously on its UI thread; future hosts can bind the same request
/// boundary to a different session runner.
pub struct EditorSession {
    pub(crate) id: SessionId,
    pub(crate) runtime: DocumentRuntime,
    pub(crate) readonly: bool,
}

impl EditorSession {
    pub fn new(runtime: DocumentRuntime, readonly: bool) -> Self {
        Self {
            id: SessionId(NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed)),
            runtime,
            readonly,
        }
    }

    pub fn into_handle(self) -> EditorSessionHandle {
        EditorSessionHandle {
            inner: Rc::new(RefCell::new(self)),
        }
    }

    fn dispatch(&mut self, envelope: CommandEnvelope) -> Result<CommandOutcome, ProtocolError> {
        if self.readonly && command_mutability(&envelope) == Some(CommandMutability::Document) {
            return Err(
                ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                    .with_document(self.runtime.document_id()),
            );
        }
        Ok(project_command_dispatch(&mut self.runtime, envelope)?.outcome)
    }

    fn query(&self, query: CommandQuery) -> QueryResult {
        let result = self.runtime.query(query);
        match result {
            QueryResult::DocumentSummary(summary) => {
                QueryResult::DocumentSummary(DocumentSummary {
                    readonly: self.readonly,
                    ..summary
                })
            }
            other => other,
        }
    }

    pub(crate) fn snapshot(&self) -> SessionSnapshot {
        let QueryResult::DocumentSummary(summary) = self.query(CommandQuery::DocumentSummary)
        else {
            unreachable!("document summary query returned the wrong result variant")
        };
        SessionSnapshot {
            session_id: self.id,
            document_id: summary.document_id,
            revision: summary.revision,
            block_count: summary.block_count,
            readonly: summary.readonly,
            dirty: summary.dirty,
        }
    }
}

pub fn project_command_dispatch(
    runtime: &mut DocumentRuntime,
    envelope: CommandEnvelope,
) -> Result<CommandDispatchSnapshot, ProtocolError> {
    let before_revision = runtime.revision();
    let outcome = runtime.dispatch(envelope)?;
    Ok(CommandDispatchSnapshot {
        before_revision,
        revision: runtime.revision(),
        outcome,
    })
}

fn command_mutability(envelope: &CommandEnvelope) -> Option<CommandMutability> {
    let invocation = envelope.invocation();
    CommandCatalog::builtin()
        .definition(&invocation.id)
        .map(|definition| definition.mutability)
}

#[derive(Clone)]
pub struct EditorSessionHandle {
    pub(crate) inner: Rc<RefCell<EditorSession>>,
}

impl EditorSessionHandle {
    pub fn id(&self) -> SessionId {
        self.inner.borrow().id
    }

    pub fn dispatch(&self, envelope: CommandEnvelope) -> Result<CommandOutcome, ProtocolError> {
        self.try_session_mut()?.dispatch(envelope)
    }

    pub fn dispatch_with_snapshot(
        &self,
        envelope: CommandEnvelope,
    ) -> Result<CommandDispatchSnapshot, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.readonly && command_mutability(&envelope) == Some(CommandMutability::Document) {
            return Err(
                ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                    .with_document(session.runtime.document_id()),
            );
        }
        project_command_dispatch(&mut session.runtime, envelope)
    }

    pub fn query(&self, query: CommandQuery) -> Result<QueryResult, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session.query(query))
    }

    pub fn projection(
        &self,
        request: ProjectionRequest,
    ) -> Result<EditorViewProjection, ProtocolError> {
        let mut session = self.try_session_mut()?;
        Ok(session.runtime.projection(request))
    }

    pub fn apply_realtime_input(
        &self,
        request: RealtimeInputRequest<'_>,
    ) -> Result<RealtimeInputOutcome, SessionRealtimeError> {
        let mut session = self
            .try_session_mut()
            .map_err(SessionRealtimeError::Protocol)?;
        if session.readonly {
            return Err(SessionRealtimeError::Protocol(
                ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                    .with_document(session.runtime.document_id()),
            ));
        }
        session
            .runtime
            .apply_realtime_input(request)
            .map_err(SessionRealtimeError::Input)
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session.snapshot())
    }

    pub(crate) fn try_session_mut(&self) -> Result<RefMut<'_, EditorSession>, ProtocolError> {
        self.inner.try_borrow_mut().map_err(|_| busy_error())
    }
}

fn busy_error() -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorCode::Busy,
        "editor session is already processing a synchronous request",
    )
    .retryable()
}

impl fmt::Debug for EditorSessionHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorSessionHandle")
            .field("session_id", &self.id())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use cditor_editor_protocol::command::{CommandSource, EditorCommand};

    use super::*;

    fn command(command: EditorCommand) -> CommandEnvelope {
        CommandEnvelope::new(command, CommandSource::Sdk)
    }

    #[test]
    fn cloned_handles_share_one_session_identity_and_runtime_owner() {
        let runtime = DocumentRuntime::demo();
        let first_block = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, false).into_handle();
        let cloned = handle.clone();

        let before = handle.snapshot().unwrap();
        handle
            .dispatch(command(EditorCommand::FocusBlock {
                block_id: first_block,
            }))
            .unwrap();
        let dispatched = cloned
            .dispatch_with_snapshot(command(EditorCommand::InsertParagraphAfterFocused))
            .unwrap();
        let after = handle.snapshot().unwrap();

        assert_eq!(handle.id(), cloned.id());
        assert_eq!(before.session_id, after.session_id);
        assert_eq!(dispatched.before_revision, before.revision);
        assert_eq!(dispatched.revision, after.revision);
        assert!(after.revision > before.revision);
    }

    #[test]
    fn readonly_session_rejects_document_commands_but_allows_selection_commands() {
        let handle = EditorSession::new(DocumentRuntime::empty(), true).into_handle();

        let error = handle
            .dispatch(command(EditorCommand::InsertParagraphAfterFocused))
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::Readonly);
        handle
            .dispatch(command(EditorCommand::SelectAll))
            .expect("selection commands remain available in read-only sessions");

        let QueryResult::DocumentSummary(summary) =
            handle.query(CommandQuery::DocumentSummary).unwrap()
        else {
            panic!("expected document summary");
        };
        assert!(summary.readonly);
    }

    #[test]
    fn snapshot_is_bounded_and_does_not_expose_runtime_borrows() {
        let handle = EditorSession::new(DocumentRuntime::demo(), false).into_handle();
        let snapshot = handle.snapshot().unwrap();

        assert_eq!(snapshot.session_id, handle.id());
        assert_eq!(snapshot.document_id, 1);
        assert!(snapshot.block_count > 0);
    }

    #[test]
    fn projection_is_read_through_the_session_owner() {
        let handle = EditorSession::new(DocumentRuntime::demo(), false).into_handle();
        let projection = handle
            .projection(ProjectionRequest {
                viewport_revision: 7,
                include_diagnostics: false,
            })
            .unwrap();

        assert_eq!(
            projection.document_id,
            handle.snapshot().unwrap().document_id
        );
        assert_eq!(projection.viewport_revision, 7);
        assert!(!projection.blocks.is_empty());
    }
}
