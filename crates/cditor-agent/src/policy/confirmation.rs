use super::capability::AgentCapability;
use crate::tools::effects::ToolEffects;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationStatus {
    Required,
    AutoApproved,
    Approved,
    Rejected,
    Expired,
    AlwaysAllowGranted,
}

#[derive(Debug, Clone)]
pub struct MutationConfirmationSummary {
    pub summary: String,
    pub target_document_title: Option<String>,
    pub block_count: usize,
    pub inserted_count: usize,
    pub deleted_count: usize,
    pub has_lossy_conversion: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConfirmationPolicy {
    session_allow: HashSet<String>,
    capability_allow: HashSet<AgentCapability>,
    safe_tools: HashSet<String>,
    safe_whole_tools: HashSet<String>,
    pending: Vec<(Uuid, MutationConfirmationSummary, AgentCapability, Instant)>,
    pub mutation_ttl: Duration,
}
impl Default for ConfirmationPolicy {
    fn default() -> Self {
        let mut st = HashSet::new();
        for n in &[
            "block.get_summary",
            "block.get_markdown",
            "block.get_structured",
            "block.list_children",
            "block.batch_get",
            "block.batch_markdown",
            "block.breadcrumb",
            "document.stat",
            "selection.get_content",
            "search.blocks",
        ] {
            st.insert(n.to_string());
        }
        let mut sw = HashSet::new();
        for n in &["question", "web_search", "web_fetch"] {
            sw.insert(n.to_string());
        }
        Self {
            session_allow: HashSet::new(),
            capability_allow: HashSet::new(),
            safe_tools: st,
            safe_whole_tools: sw,
            pending: Vec::new(),
            mutation_ttl: Duration::from_secs(300),
        }
    }
}
impl ConfirmationPolicy {
    pub fn needs_tool_confirmation(
        &self,
        tool: &str,
        effects: ToolEffects,
        is_native: bool,
        readonly_hint: bool,
    ) -> ConfirmationStatus {
        if self.session_allow.contains("*") || self.session_allow.contains(tool) {
            return ConfirmationStatus::AutoApproved;
        }
        if self.safe_whole_tools.contains(tool) {
            return ConfirmationStatus::AutoApproved;
        }
        if self.safe_tools.contains(tool) {
            return ConfirmationStatus::AutoApproved;
        }
        if !is_native {
            return if readonly_hint {
                ConfirmationStatus::AutoApproved
            } else {
                ConfirmationStatus::Required
            };
        }
        if effects.is_write() {
            ConfirmationStatus::Required
        } else {
            ConfirmationStatus::AutoApproved
        }
    }
    pub fn needs_local_snapshot(&self, tool: &str, effects: ToolEffects, is_native: bool) -> bool {
        !self.safe_tools.contains(tool)
            && !self.safe_whole_tools.contains(tool)
            && is_native
            && effects.is_write()
    }
    pub fn grant_always_allow_tool(&mut self, tool: &str) {
        self.session_allow.insert(tool.to_string());
    }
    pub fn grant_always_allow_capability(&mut self, cap: AgentCapability) -> bool {
        if cap.is_high_risk() {
            false
        } else {
            self.capability_allow.insert(cap);
            true
        }
    }
    pub fn register_pending(
        &mut self,
        id: Uuid,
        summary: MutationConfirmationSummary,
        cap: AgentCapability,
    ) {
        self.purge_expired();
        self.pending.push((id, summary, cap, Instant::now()));
    }
    pub fn approve(&mut self, id: Uuid) -> Option<MutationConfirmationSummary> {
        self.purge_expired();
        self.pending
            .iter()
            .position(|(i, _, _, _)| *i == id)
            .map(|idx| self.pending.remove(idx).1)
    }
    pub fn reject(&mut self, id: Uuid) {
        self.pending.retain(|(i, _, _, _)| *i != id);
    }
    pub fn is_pending(&self, id: Uuid) -> bool {
        self.pending
            .iter()
            .any(|(i, _, _, t)| *i == id && t.elapsed() < self.mutation_ttl)
    }
    pub fn pending_count(&self) -> usize {
        self.pending
            .iter()
            .filter(|(_, _, _, t)| t.elapsed() < self.mutation_ttl)
            .count()
    }
    fn purge_expired(&mut self) {
        self.pending
            .retain(|(_, _, _, t)| t.elapsed() < self.mutation_ttl);
    }
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn safe_tools_no_confirm() {
        let p = ConfirmationPolicy::default();
        assert_eq!(
            p.needs_tool_confirmation("block.get_summary", ToolEffects::LocalRead, true, true),
            ConfirmationStatus::AutoApproved
        );
    }
    #[test]
    fn write_tool_needs_confirm() {
        let p = ConfirmationPolicy::default();
        assert_eq!(
            p.needs_tool_confirmation("block.replace", ToolEffects::LocalWrite, true, true),
            ConfirmationStatus::Required
        );
    }
    #[test]
    fn pending_mutation() {
        let mut p = ConfirmationPolicy::default();
        let id = uuid::Uuid::new_v4();
        let s = MutationConfirmationSummary {
            summary: "test".into(),
            target_document_title: None,
            block_count: 1,
            inserted_count: 0,
            deleted_count: 0,
            has_lossy_conversion: false,
            warnings: vec![],
        };
        p.register_pending(id, s, AgentCapability::WriteBlockContent);
        assert!(p.is_pending(id));
        let a = p.approve(id);
        assert!(a.is_some());
    }
}
