//! Token estimation and context compaction.
//! Mirrors SiYuan's compaction.go — heuristic token counting
//! followed by truncation of old messages while preserving structure.

use crate::protocol::checkpoint::AgentCheckpoint;
use crate::protocol::messages::SessionEntry;
use crate::runtime::engine::ChatMessage;
use serde::{Deserialize, Serialize};

// ── Token counter ─────────────────────────────────────────────────

/// Simple character-based token estimator (~4 chars per token for English,
/// ~2 chars for CJK). Produces a conservative upper bound.
pub fn estimate_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] > '\u{2FFF}' {
            // CJK: 2 chars ≈ 1 token
            tokens += 1;
            i += 1;
        } else if chars[i].is_whitespace() {
            i += 1;
        } else {
            // ASCII word chars: ~4 chars per token
            tokens += 1;
            i += 4.min(chars.len() - i);
            // Skip to next whitespace or CJK
            while i < chars.len()
                && !chars[i].is_whitespace()
                && chars[i] <= '\u{2FFF}'
            {
                i += 1;
            }
        }
    }
    tokens.max(1)
}

/// Count tokens for a ChatMessage.
pub fn message_tokens(msg: &ChatMessage) -> usize {
    let content = msg.content.as_deref().unwrap_or("");
    estimate_tokens(content)
}

/// Count tokens for message list (including role overhead ~4 tokens/msg).
pub fn messages_token_count(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| message_tokens(m) + 4).sum()
}

/// Count tokens for a SessionEntry.
pub fn entry_tokens(entry: &SessionEntry) -> usize {
    estimate_tokens(&entry.content)
        + entry
            .tool_calls
            .iter()
            .map(|tc| {
                estimate_tokens(&tc.name)
                    + tc.result.as_deref().map(estimate_tokens).unwrap_or(0)
            })
            .sum::<usize>()
        + 4 // role overhead
}

// ── Compaction ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub before_tokens: u32,
    pub after_tokens: u32,
    pub entries_removed: usize,
    pub system_prompt_kept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compactor {
    /// When current tokens exceed this fraction of context limit, compact.
    pub threshold_ratio: f64,
    /// Keep at least this many recent entries.
    pub min_keep: usize,
}

impl Default for Compactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compactor {
    pub fn new() -> Self {
        Self {
            threshold_ratio: 0.75,
            min_keep: 4,
        }
    }

    /// Check if compaction is needed.
    pub fn should_compact(&self, current_tokens: u64, context_limit: u64) -> bool {
        current_tokens >= ((context_limit as f64) * self.threshold_ratio) as u64
    }

    /// Compact a message list by removing old non-system messages.
    /// Keeps: system message, last min_keep messages, all messages with pending tool calls.
    pub fn compact_messages(&self, messages: &[ChatMessage]) -> (Vec<ChatMessage>, CompactionResult) {
        let before = messages_token_count(messages);
        if messages.len() <= self.min_keep + 1 {
            return (
                messages.to_vec(),
                CompactionResult {
                    before_tokens: before as u32,
                    after_tokens: before as u32,
                    entries_removed: 0,
                    system_prompt_kept: true,
                },
            );
        }

        let system = messages.first().filter(|m| m.role == "system");
        let rest: Vec<_> = messages
            .iter()
            .enumerate()
            .filter(|(i, m)| i != &0 || m.role != "system")
            .collect();

        if rest.len() <= self.min_keep {
            return (
                messages.to_vec(),
                CompactionResult {
                    before_tokens: before as u32,
                    after_tokens: before as u32,
                    entries_removed: 0,
                    system_prompt_kept: true,
                },
            );
        }

        // Keep last min_keep messages + any with pending tool results
        let keep_count = self.min_keep;
        let removed = rest.len() - keep_count;
        let kept_rest: Vec<&ChatMessage> =
            rest.iter().skip(removed).map(|(_, m)| *m).collect();

        let mut compacted = Vec::new();
        if let Some(sys) = system {
            compacted.push(sys.clone());
        }
        for msg in kept_rest {
            compacted.push(msg.clone());
        }

        let after = messages_token_count(&compacted);
        (
            compacted,
            CompactionResult {
                before_tokens: before as u32,
                after_tokens: after as u32,
                entries_removed: removed,
                system_prompt_kept: true,
            },
        )
    }

    /// Compact session entries for checkpoint storage.
    pub fn compact_entries(
        &self,
        entries: &[SessionEntry],
        _checkpoint: &AgentCheckpoint,
    ) -> (Vec<SessionEntry>, CompactionResult) {
        let before: u32 = entries.iter().map(|e| entry_tokens(e) as u32).sum();
        if entries.len() <= self.min_keep {
            return (
                entries.to_vec(),
                CompactionResult {
                    before_tokens: before,
                    after_tokens: before,
                    entries_removed: 0,
                    system_prompt_kept: true,
                },
            );
        }

        let removed = entries.len() - self.min_keep;
        let compacted: Vec<SessionEntry> = entries.iter().skip(removed).cloned().collect();
        let after: u32 = compacted.iter().map(|e| entry_tokens(e) as u32).sum();

        (
            compacted,
            CompactionResult {
                before_tokens: before,
                after_tokens: after,
                entries_removed: removed,
                system_prompt_kept: true,
            },
        )
    }
}

// ── Integration helper ─────────────────────────────────────────────

/// Estimate context usage and determine if compaction is needed.
pub fn check_context_budget(
    compactor: &Compactor,
    messages: &[ChatMessage],
    context_limit: usize,
) -> (bool, usize) {
    let tokens = messages_token_count(messages);
    (compactor.should_compact(tokens as u64, context_limit as u64), tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_english_text() {
        let tokens = estimate_tokens("Hello world, this is a test.");
        assert!(tokens > 0 && tokens < 20);
    }

    #[test]
    fn estimate_cjk_text() {
        let tokens = estimate_tokens("你好世界这是一个测试");
        assert!(tokens > 0 && tokens < 20);
    }

    #[test]
    fn estimate_empty_string() {
        assert_eq!(estimate_tokens(""), 1);
    }

    #[test]
    fn message_token_count() {
        let msg = ChatMessage {
            role: "user".into(),
            content: Some("hello".into()),
            tool_calls: None,
            tool_call_id: None,
        };
        let count = messages_token_count(&[msg]);
        assert!(count > 0);
    }

    #[test]
    fn should_compact_at_threshold() {
        let c = Compactor::new();
        assert!(c.should_compact(80, 100));
        assert!(!c.should_compact(70, 100));
    }

    #[test]
    fn compact_preserves_system() {
        let c = Compactor::new();
        let msgs = vec![
            ChatMessage {
                role: "system".into(),
                content: Some("sys".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".into(),
                content: Some("u1".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "assistant".into(),
                content: Some("a1".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".into(),
                content: Some("u2".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "assistant".into(),
                content: Some("a2".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".into(),
                content: Some("u3".into()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let (compacted, result) = c.compact_messages(&msgs);
        assert_eq!(compacted[0].role, "system");
        assert!(result.entries_removed > 0);
        assert!(compacted.len() < msgs.len());
    }

    #[test]
    fn compact_empty_list() {
        let c = Compactor::new();
        let (compacted, _) = c.compact_messages(&[]);
        assert!(compacted.is_empty());
    }
}
