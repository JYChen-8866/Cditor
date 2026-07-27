//! Cditor-specific adapter traits — the bridge between the agent crate
//! and the actual editor runtime.
//!
//! These traits are implemented by the desktop app / cditor-session layer.
//! The agent crate depends only on these abstractions, never directly
//! on cditor-runtime or GPUI.


use crate::protocol::context::AgentContextSnapshot;
use crate::protocol::error::AgentToolError;
use crate::tools::mutation::{
    AgentMutationIntent, AgentMutationPreview, PreparedAgentMutation, PreparedMutationId,
};
use crate::tools::read::{AgentBlockNode, AgentBlockSummary, AgentReadEnvelope, ChildListEntry, DocumentStat};
use crate::{BlockId, DocumentId};
use std::sync::Arc;

// ── Read port ─────────────────────────────────────────────────────

/// A live read connection into the editor document.
/// The agent uses this to explore document structure before writing.
pub trait AgentReadPort: Send + Sync {
    /// Get a block summary (kind, plain text excerpt, scalar attrs).
    fn block_summary(&self, block_id: BlockId) -> Result<AgentBlockSummary, AgentToolError>;

    /// Render a block (and optionally subtree) as Markdown.
    fn block_markdown(
        &self,
        block_id: BlockId,
        scope: &str,
        max_depth: usize,
        max_blocks: usize,
    ) -> Result<AgentReadEnvelope<AgentBlockNode>, AgentToolError>;

    /// Get structured representation (table rows, property bags).
    fn block_structured(
        &self,
        block_id: BlockId,
        max_depth: usize,
    ) -> Result<AgentReadEnvelope<AgentBlockNode>, AgentToolError>;

    /// List direct children with pagination.
    fn block_children(
        &self,
        parent_id: BlockId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ChildListEntry>, AgentToolError>;

    /// Batch get block summaries.
    fn block_batch_summaries(
        &self,
        block_ids: &[BlockId],
    ) -> Result<Vec<AgentBlockSummary>, AgentToolError>;

    /// Batch get block markdown.
    fn block_batch_markdown(
        &self,
        block_ids: &[BlockId],
    ) -> Result<Vec<AgentReadEnvelope<AgentBlockNode>>, AgentToolError>;

    /// Get document statistics.
    fn document_stat(&self, doc_id: DocumentId) -> Result<DocumentStat, AgentToolError>;

    /// Capture the current editor context (focus, selection, visible blocks).
    fn capture_context(&self, doc_id: DocumentId) -> AgentContextSnapshot;
}

// ── Write port ────────────────────────────────────────────────────

/// A write connection into the editor document.
/// Writes go through a prepare → confirm → commit two-phase protocol.
pub trait AgentWritePort: Send + Sync {
    /// Prepare a mutation. Returns a prepared mutation ID.
    /// The mutation is NOT applied yet — the caller must call commit().
    fn prepare(&self, intent: AgentMutationIntent) -> Result<PreparedAgentMutation, AgentToolError>;

    /// Generate a preview of the prepared mutation (for UI diff display).
    fn preview(
        &self,
        prepared_id: PreparedMutationId,
    ) -> Result<AgentMutationPreview, AgentToolError>;

    /// Commit a previously prepared mutation.
    /// Returns the list of block IDs that were created/modified/deleted.
    fn commit(
        &self,
        prepared_id: PreparedMutationId,
    ) -> Result<Vec<BlockId>, AgentToolError>;

    /// Cancel a prepared mutation (no changes applied).
    fn cancel(&self, prepared_id: PreparedMutationId);
}

// ── Transaction channel ───────────────────────────────────────────


// ── Agent ports bundle ────────────────────────────────────────────

/// Bundle of read and write ports injected into tool execution.
/// The session layer creates this and passes it into the agent runtime.
pub struct AgentPorts {
    pub read: Arc<dyn AgentReadPort>,
    pub write: Arc<dyn AgentWritePort>,
}

impl AgentPorts {
    pub fn new(read: Arc<dyn AgentReadPort>, write: Arc<dyn AgentWritePort>) -> Self {
        Self { read, write }
    }
}

/// The transaction channel handles the flow of mutations from agent to editor.
/// It implements the two-phase commit protocol with precondition checking.
pub struct AgentTransactionChannel<R: AgentReadPort, W: AgentWritePort> {
    reader: R,
    writer: W,
    active_prepare: Option<PreparedMutationId>,
}

impl<R: AgentReadPort, W: AgentWritePort> AgentTransactionChannel<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            active_prepare: None,
        }
    }

    /// Prepare → preview → confirm → commit flow.
    /// Returns committed block IDs or error.
    pub fn execute_mutation(
        &mut self,
        intent: AgentMutationIntent,
    ) -> Result<Vec<BlockId>, AgentToolError> {
        let prepared = self.writer.prepare(intent)?;
        let id = prepared.id;
        self.active_prepare = Some(id);

        let _preview = self.writer.preview(id)?;
        // In production: preview is shown to user for confirmation.
        // For now: auto-commit.

        let committed = self.writer.commit(id)?;
        self.active_prepare = None;
        Ok(committed)
    }

    /// Cancel the active prepared mutation.
    pub fn cancel_active(&mut self) {
        if let Some(id) = self.active_prepare.take() {
            self.writer.cancel(id);
        }
    }

    pub fn reader(&self) -> &R {
        &self.reader
    }

    pub fn writer(&self) -> &W {
        &self.writer
    }
}

// ── Context capture helper ────────────────────────────────────────

/// Build an AgentContextSnapshot from editor state.
/// This is a convenience that the desktop app calls.
pub fn build_context_snapshot(
    doc_id: DocumentId,
    doc_title: Option<&str>,
    doc_revision: u64,
    structure_version: u64,
    focused_block_id: Option<BlockId>,
    selected_block_ids: &[BlockId],
    visible_block_ids: &[BlockId],
    now_ms: u64,
) -> AgentContextSnapshot {
    use crate::protocol::context::{
        AgentSelectionDescriptor, AgentTextAnchor, AgentVisibleWindow,
    };

    let focused = focused_block_id.map(|bid| AgentTextAnchor {
        block_id: bid,
        surface: crate::protocol::context::AgentSurface::Body,
        utf16_offset: 0,
        content_version: 0,
    });

    let selection = if selected_block_ids.is_empty() {
        AgentSelectionDescriptor::None
    } else if selected_block_ids.len() == 1 {
        AgentSelectionDescriptor::Caret {
            anchor: AgentTextAnchor {
                block_id: selected_block_ids[0],
                surface: crate::protocol::context::AgentSurface::Body,
                utf16_offset: 0,
                content_version: 0,
            },
        }
    } else {
        AgentSelectionDescriptor::BlockSelection {
            block_ids: selected_block_ids.to_vec(),
        }
    };

    AgentContextSnapshot {
        snapshot_id: uuid::Uuid::new_v4(),
        document_id: doc_id,
        document_revision: doc_revision,
        structure_version,
        title: doc_title.map(|s| s.to_string()),
        focused,
        selection,
        selected_block_ids: selected_block_ids.to_vec(),
        visible_window: AgentVisibleWindow {
            first_block_id: visible_block_ids.first().copied(),
            last_block_id: visible_block_ids.last().copied(),
            visible_block_ids: visible_block_ids.to_vec(),
            truncated: visible_block_ids.len() > 50,
        },
        captured_at_ms: now_ms,
    }
}

// ── Tests ─────────────────────────────────────────────────────────


// ── Test helpers ─────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    #[allow(dead_code)]
    pub(crate) fn mock_agent_ports() -> Arc<AgentPorts> {
        Arc::new(AgentPorts::new(
            Arc::new(MockReadPort { blocks: HashMap::new() }),
            Arc::new(MockWritePort { prepared: None }),
        ))
    }

    pub(crate) struct MockReadPort {
        pub blocks: HashMap<BlockId, AgentBlockSummary>,
    }

    pub(crate) struct MockWritePort {
        pub prepared: Option<PreparedAgentMutation>,
    }

    impl AgentReadPort for MockReadPort {
        fn block_summary(&self, block_id: BlockId) -> Result<AgentBlockSummary, AgentToolError> {
            self.blocks.get(&block_id).cloned().ok_or(AgentToolError::NotFound { block_id })
        }
        fn block_markdown(&self, _bid: BlockId, _scope: &str, _max_depth: usize, _max_blocks: usize) -> Result<AgentReadEnvelope<AgentBlockNode>, AgentToolError> {
            Err(AgentToolError::NotFound { block_id: BlockId::new_v4() })
        }
        fn block_structured(&self, _bid: BlockId, _max_depth: usize) -> Result<AgentReadEnvelope<AgentBlockNode>, AgentToolError> {
            Err(AgentToolError::NotFound { block_id: BlockId::new_v4() })
        }
        fn block_children(&self, _pid: BlockId, _limit: usize, _offset: usize) -> Result<Vec<ChildListEntry>, AgentToolError> {
            Ok(Vec::new())
        }
        fn block_batch_summaries(&self, _ids: &[BlockId]) -> Result<Vec<AgentBlockSummary>, AgentToolError> {
            Ok(Vec::new())
        }
        fn block_batch_markdown(&self, _ids: &[BlockId]) -> Result<Vec<AgentReadEnvelope<AgentBlockNode>>, AgentToolError> {
            Ok(Vec::new())
        }
        fn document_stat(&self, doc_id: DocumentId) -> Result<DocumentStat, AgentToolError> {
            Ok(DocumentStat { document_id: doc_id, title: None, total_blocks: 0, top_level_count: 0, kind_counts: vec![], structure_version: 0, fully_indexed: false })
        }
        fn capture_context(&self, doc_id: DocumentId) -> AgentContextSnapshot {
            build_context_snapshot(doc_id, Some("test"), 0, 0, None, &[], &[], 1000)
        }
    }

    impl AgentWritePort for MockWritePort {
        fn prepare(&self, _intent: AgentMutationIntent) -> Result<PreparedAgentMutation, AgentToolError> {
            Ok(PreparedAgentMutation {
                id: PreparedMutationId::new_v4(),
                session_id: crate::AgentSessionId::new_v4(),
                tool_call_id: crate::ToolCallId::new_v4(),
                base_document_revision: 0,
                base_structure_version: 0,
                capability: crate::policy::capability::AgentCapability::WriteBlockContent,
                digest: [0u8; 32],
                expires_at_ms: u64::MAX,
            })
        }
        fn preview(&self, _pid: PreparedMutationId) -> Result<AgentMutationPreview, AgentToolError> {
            Ok(AgentMutationPreview {
                summary: "mock".into(), affected_blocks: vec![], inserted_blocks: vec![],
                deleted_blocks: vec![], textual_diffs: vec![], structural_moves: vec![], warnings: vec![],
            })
        }
        fn commit(&self, _pid: PreparedMutationId) -> Result<Vec<BlockId>, AgentToolError> {
            Ok(vec![BlockId::new_v4()])
        }
        fn cancel(&self, _pid: PreparedMutationId) {}
    }

    #[test]
    fn context_snapshot_includes_title() {
        let snap = build_context_snapshot(DocumentId::new_v4(), Some("My Document"), 42, 3, None, &[], &[], 1000);
        assert_eq!(snap.title, Some("My Document".into()));
    }

    #[test]
    fn context_snapshot_with_selection() {
        let bid = BlockId::new_v4();
        let snap = build_context_snapshot(DocumentId::new_v4(), None, 0, 0, Some(bid), &[bid], &[bid], 1000);
        assert!(snap.focused.is_some());
    }

    #[test]
    fn visible_window_truncated_at_50() {
        let ids: Vec<BlockId> = (0..60).map(|_| BlockId::new_v4()).collect();
        let snap = build_context_snapshot(DocumentId::new_v4(), None, 0, 0, None, &[], &ids, 1000);
        assert!(snap.visible_window.truncated);
    }

    #[test]
    fn transaction_channel_auto_commit() {
        let reader = MockReadPort { blocks: HashMap::new() };
        let writer = MockWritePort { prepared: None };
        let mut channel = AgentTransactionChannel::new(reader, writer);
        let result = channel.execute_mutation(AgentMutationIntent::DeleteBlocks { targets: vec![] });
        assert!(result.is_ok());
    }

    #[test]
    fn transaction_channel_cancel() {
        let reader = MockReadPort { blocks: HashMap::new() };
        let writer = MockWritePort { prepared: None };
        let mut channel = AgentTransactionChannel::new(reader, writer);
        let intent = AgentMutationIntent::DeleteBlocks { targets: vec![] };
        let _ = channel.writer().prepare(intent.clone());
        channel.cancel_active();
        assert!(channel.active_prepare.is_none());
    }

    #[test]
    fn build_context_snapshot_empty_doc() {
        let snap = build_context_snapshot(DocumentId::new_v4(), None, 0, 0, None, &[], &[], 1000);
        assert!(snap.focused.is_none());
    }
}
