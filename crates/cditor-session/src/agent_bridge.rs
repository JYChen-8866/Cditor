//! Agent bridge — connects cditor-agent to the live editor runtime.
//!
//! Follows the clawcode pattern: Read/Write ports hold
//! Arc<Mutex<DocumentRuntime>> for thread-safe access from the
//! agent's background thread. The session spawns the agent
//! on a thread and processes tool operations through the ports.

use std::sync::{Arc, Mutex};

use cditor_agent::runtime::adapter::{AgentPorts, AgentReadPort, AgentWritePort};
use cditor_agent::tools::mutation::{
    AgentContent, AgentMutationIntent, AgentMutationPreview, PreparedAgentMutation,
    PreparedMutationId, VersionedBlockRef, VersionedInsertAnchor,
};
use cditor_agent::tools::read::{
    AgentBlockNode, AgentBlockSummary, AgentReadEnvelope, BlockKind, ChildListEntry, DocumentStat,
    MarkdownDialect,
};
use cditor_agent::{BlockId, DocumentId, ToolCallId};
use cditor_agent::protocol::context::AgentContextSnapshot;
use cditor_agent::protocol::error::AgentToolError;
use cditor_core::ids::BlockId as CoreBlockId;
use cditor_core::ids::DocumentId as CoreDocId;
use cditor_runtime::DocumentRuntime;

// ── ID conversion helpers ─────────────────────────────────────────

fn to_agent_id(id: CoreBlockId) -> BlockId {
    BlockId::from_u64_pair(id, 0)
}
fn from_agent_id(id: BlockId) -> CoreBlockId {
    id.as_u64_pair().0
}
fn to_agent_doc_id(_id: CoreDocId) -> DocumentId {
    // Cditor is single-document; use a well-known UUID
    DocumentId::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
}

// ── Runtime-backed Read Port ──────────────────────────────────────

pub struct RuntimeReadPort {
    runtime: Arc<Mutex<DocumentRuntime>>,
}
impl RuntimeReadPort {
    pub fn new(runtime: Arc<Mutex<DocumentRuntime>>) -> Self {
        Self { runtime }
    }
    fn with_runtime<F, R>(&self, f: F) -> Result<R, AgentToolError>
    where
        F: FnOnce(&DocumentRuntime) -> Result<R, AgentToolError>,
    {
        let rt = self.runtime.lock().map_err(|_| AgentToolError::Cancelled)?;
        f(&rt)
    }
}
impl AgentReadPort for RuntimeReadPort {
    fn block_summary(&self, bid: BlockId) -> Result<AgentBlockSummary, AgentToolError> {
        let core_id = from_agent_id(bid);
        self.with_runtime(|rt| {
            let (kind, text, version, has_children) = rt
                .agent_block_summary(core_id)
                .ok_or(AgentToolError::NotFound { block_id: bid })?;
            Ok(AgentBlockSummary {
                block_id: bid,
                kind: block_kind_from_core(&kind),
                plain_text: text.chars().take(200).collect(),
                attrs_json: None,
                content_version: version,
                has_children,
            })
        })
    }
    fn block_markdown(
        &self, bid: BlockId, _scope: &str, _max_depth: usize, _max_blocks: usize,
    ) -> Result<AgentReadEnvelope<AgentBlockNode>, AgentToolError> {
        let core_id = from_agent_id(bid);
        self.with_runtime(|rt| {
            let (kind, text, _version, _has_children) = rt
                .agent_block_summary(core_id)
                .ok_or(AgentToolError::NotFound { block_id: bid })?;
            let node = AgentBlockNode {
                block_id: bid,
                kind: block_kind_from_core(&kind),
                inline_marks: vec![],
                attrs_json: None,
                children: vec![],
            };
            Ok(AgentReadEnvelope {
                request_id: ToolCallId::new_v4(),
                document_id: to_agent_doc_id(rt.document_id()),
                observed_revision: 0,
                observed_structure_version: rt.structure_version(),
                data: node,
                truncated: false,
                continuation: None,
                estimated_tokens: (text.len() / 4) as u32,
            })
        })
    }
    fn block_structured(
        &self, bid: BlockId, _max_depth: usize,
    ) -> Result<AgentReadEnvelope<AgentBlockNode>, AgentToolError> {
        self.block_markdown(bid, "self", 0, 1)
    }
    fn block_children(
        &self, parent_id: BlockId, limit: usize, offset: usize,
    ) -> Result<Vec<ChildListEntry>, AgentToolError> {
        let core_id = from_agent_id(parent_id);
        self.with_runtime(|rt| {
            Ok(rt
                .agent_children(core_id, limit, offset)
                .into_iter()
                .map(|(cid, kind, text)| ChildListEntry {
                    block_id: to_agent_id(cid),
                    kind: block_kind_from_core(&kind),
                    summary: text.chars().take(100).collect(),
                    has_children: false,
                    content_version: 0,
                })
                .collect())
        })
    }
    fn block_batch_summaries(
        &self, block_ids: &[BlockId],
    ) -> Result<Vec<AgentBlockSummary>, AgentToolError> {
        block_ids.iter().map(|bid| self.block_summary(*bid)).collect()
    }
    fn block_batch_markdown(
        &self, block_ids: &[BlockId],
    ) -> Result<Vec<AgentReadEnvelope<AgentBlockNode>>, AgentToolError> {
        block_ids
            .iter()
            .map(|bid| self.block_markdown(*bid, "self", 8, 200))
            .collect()
    }
    fn document_stat(&self, _doc_id: DocumentId) -> Result<DocumentStat, AgentToolError> {
        self.with_runtime(|rt| {
            let (_doc_id, _title, total) = rt.agent_document_stat();
            Ok(DocumentStat {
                document_id: to_agent_doc_id(rt.document_id()),
                title: rt.document_title().map(|s| s.to_string()),
                total_blocks: total,
                top_level_count: 0,
                kind_counts: vec![], // simplified
                structure_version: rt.structure_version(),
                fully_indexed: true,
            })
        })
    }
    fn capture_context(&self, _doc_id: DocumentId) -> AgentContextSnapshot {
        self.with_runtime(|rt| {
            let title = rt.agent_document_stat().1;
            Ok(cditor_agent::protocol::context::AgentContextSnapshot {
                snapshot_id: cditor_agent::AgentSnapshotId::new_v4(),
                document_id: to_agent_doc_id(rt.document_id()),
                title,
                document_revision: 0,
                structure_version: rt.structure_version(),
                focused: None,
                selected_block_ids: vec![],
                visible_window: cditor_agent::protocol::context::AgentVisibleWindow {
                    visible_block_ids: vec![],
                    truncated: false,
                    first_block_id: None,
                    last_block_id: None,
                },
                captured_at_ms: 0,
                selection: cditor_agent::protocol::context::AgentSelectionDescriptor::None,
                })
        }).unwrap_or_else(|_| cditor_agent::protocol::context::AgentContextSnapshot {
            snapshot_id: cditor_agent::AgentSnapshotId::new_v4(),
            document_id: to_agent_doc_id(0),
            title: None,
            document_revision: 0,
            structure_version: 0,
            focused: None,
            selected_block_ids: vec![],
            visible_window: cditor_agent::protocol::context::AgentVisibleWindow {
                visible_block_ids: vec![],
                truncated: false,
                first_block_id: None,
                last_block_id: None,
            },
            captured_at_ms: 0,
            selection: cditor_agent::protocol::context::AgentSelectionDescriptor::None,
            })
    }
}

// ── Runtime-backed Write Port ─────────────────────────────────────

pub struct RuntimeWritePort {
    runtime: Arc<Mutex<DocumentRuntime>>,
    session_id: cditor_agent::AgentSessionId,
}
impl RuntimeWritePort {
    pub fn new(runtime: Arc<Mutex<DocumentRuntime>>, session_id: cditor_agent::AgentSessionId) -> Self {
        Self { runtime, session_id }
    }
    fn with_runtime_mut<F, R>(&self, f: F) -> Result<R, AgentToolError>
    where
        F: FnOnce(&mut DocumentRuntime) -> Result<R, AgentToolError>,
    {
        let mut rt = self.runtime.lock().map_err(|_| AgentToolError::Cancelled)?;
        f(&mut rt)
    }
}
impl AgentWritePort for RuntimeWritePort {
    fn prepare(
        &self, intent: AgentMutationIntent,
    ) -> Result<PreparedAgentMutation, AgentToolError> {
        let _ = intent.kind();
        Ok(PreparedAgentMutation {
            id: PreparedMutationId::new_v4(),
            session_id: self.session_id,
            tool_call_id: ToolCallId::new_v4(),
            base_document_revision: 0,
            base_structure_version: 0,
            capability: cditor_agent::policy::capability::AgentCapability::WriteBlockContent,
            digest: [0u8; 32],
            expires_at_ms: u64::MAX,
        })
    }
    fn preview(&self, _id: PreparedMutationId) -> Result<AgentMutationPreview, AgentToolError> {
        Ok(AgentMutationPreview {
            summary: "preview pending".into(),
            affected_blocks: vec![],
            inserted_blocks: vec![],
            deleted_blocks: vec![],
            textual_diffs: vec![],
            structural_moves: vec![],
            warnings: vec![],
        })
    }
    fn commit(&self, _id: PreparedMutationId) -> Result<Vec<BlockId>, AgentToolError> {
        self.with_runtime_mut(|_rt| {
            // TODO: Apply the prepared mutation via EditTransaction
            Ok(vec![BlockId::new_v4()])
        })
    }
    fn cancel(&self, _id: PreparedMutationId) {}
}

// ── Helpers ───────────────────────────────────────────────────────

fn block_kind_from_core(kind: &cditor_core::rich_text::block_kind::RichBlockKind) -> BlockKind {
    use cditor_core::rich_text::block_kind::RichBlockKind;
    match kind {
        RichBlockKind::Heading { .. } => BlockKind::Heading,
        RichBlockKind::Paragraph => BlockKind::Paragraph,
        RichBlockKind::BulletedList | RichBlockKind::NumberedList => BlockKind::List,
        
        RichBlockKind::Code { .. } => BlockKind::CodeBlock,
        RichBlockKind::Table => BlockKind::Table,
        
        
        RichBlockKind::Image { .. } => BlockKind::Image,
        RichBlockKind::Callout { .. } => BlockKind::Callout,
        RichBlockKind::Quote => BlockKind::Blockquote,
        RichBlockKind::Divider => BlockKind::Divider,
        
        RichBlockKind::Todo { .. } => BlockKind::Paragraph,
        RichBlockKind::Toggle => BlockKind::Paragraph,
        _ => BlockKind::Unknown,
    }
}

/// Create AgentPorts backed by a DocumentRuntime.
pub fn create_runtime_ports(
    runtime: Arc<Mutex<DocumentRuntime>>,
    session_id: cditor_agent::AgentSessionId,
) -> AgentPorts {
    AgentPorts::new(
        Arc::new(RuntimeReadPort::new(runtime.clone())),
        Arc::new(RuntimeWritePort::new(runtime, session_id)),
    )
}

// ── Agent session runner (stub — to be wired to AI popup) ──────

// TODO: Wire AgentRuntime to the existing AI popup flow.
// See the clawcode pattern: spawn agent on background thread,
// bridge events through AiStreamEvent, apply writes via EditTransaction.

