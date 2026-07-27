use std::collections::VecDeque;

use cditor_core::ids::DocumentId;
use serde::{Deserialize, Serialize};

use crate::protocol::checkpoint::AgentCheckpoint;
use crate::protocol::event::AgentEvent;
use crate::runtime::budget::AgentBudget;
use crate::runtime::compaction::Compactor;
use crate::runtime::doom_loop::DoomLoopDetector;
use crate::tools::registry::ToolRegistry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionConfig {
    pub max_turns: usize,
    pub max_tool_calls_per_turn: usize,
    pub max_total_tokens: usize,
    pub compact_at_token_count: usize,
    pub tool_timeout_ms: u64,
    pub confirmation_required_for_writes: bool,
    pub doom_loop_signature_threshold: usize,
    pub doom_loop_hard_stop_threshold: usize,
}

impl Default for AgentSessionConfig {
    fn default() -> Self {
        Self {
            max_turns: 20,
            max_tool_calls_per_turn: 50,
            max_total_tokens: 200_000,
            compact_at_token_count: 80_000,
            tool_timeout_ms: 30_000,
            confirmation_required_for_writes: true,
            doom_loop_signature_threshold: 3,
            doom_loop_hard_stop_threshold: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSessionState {
    Idle,
    Thinking,
    CallingTool,
    AwaitingConfirmation,
    Completed,
    Failed,
    Cancelled,
}

pub struct AgentSession {
    pub config: AgentSessionConfig,
    pub state: AgentSessionState,
    pub document_id: Option<DocumentId>,
    pub turn_count: usize,
    pub tool_call_count: usize,
    pub mutation_count: usize,
    pub cursor_token_count: usize,
    pub events: VecDeque<AgentEvent>,
    pub registry: ToolRegistry,
    pub budget: AgentBudget,
    pub compactor: Compactor,
    pub doom_loop: DoomLoopDetector,
}

impl AgentSession {
    pub fn new(config: AgentSessionConfig, registry: ToolRegistry) -> Self {
        Self {
            budget: AgentBudget::new(config.max_total_tokens),
            config: config.clone(),
            state: AgentSessionState::Idle,
            document_id: None,
            turn_count: 0,
            tool_call_count: 0,
            mutation_count: 0,
            cursor_token_count: 0,
            events: VecDeque::new(),
            registry,
            compactor: Compactor::new(config.compact_at_token_count),
            doom_loop: DoomLoopDetector::new(config.doom_loop_signature_threshold),
        }
    }

    pub fn next_turn_id(&mut self) -> u64 {
        let id = self.turn_count as u64 + 1;
        self.turn_count += 1;
        id
    }

    pub fn next_tool_call_id(&mut self) -> u64 {
        let id = self.tool_call_count as u64;
        self.tool_call_count += 1;
        id
    }

    pub fn push_event(&mut self, event: AgentEvent) {
        self.events.push_back(event);
    }

    pub fn drain_events(&mut self) -> Vec<AgentEvent> {
        self.events.drain(..).collect()
    }

    pub fn checkpoint(&self) -> AgentCheckpoint {
        AgentCheckpoint {
            turn_count: self.turn_count as u64,
            cursor_token_count: self.cursor_token_count as u64,
            tool_call_count: self.tool_call_count as u64,
            mutation_count: self.mutation_count as u64,
            committed: matches!(self.state, AgentSessionState::Completed),
        }
    }
}
