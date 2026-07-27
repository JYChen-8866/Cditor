//! Model provider abstraction for the Cditor agent.
//! The desktop app injects a concrete provider (OpenAI, Anthropic, Ollama, etc.)
//! and the agent engine streams through it.

use crate::JsonValue;
use crate::runtime::engine::ChatMessage;
use async_channel::Sender;

/// Lightweight model metadata.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_limit: usize,
}

/// Streaming events from a chat model.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text delta from the model.
    Delta(String),
    /// Thinking delta (extended reasoning).
    ThinkingDelta(String),
    /// Thinking phase completed.
    ThinkingDone,
    /// Tool call fragment — id & name sent once, arguments accumulate.
    /// Callers aggregate fragments by id until Done fires.
    ToolCallFragment {
        id: String,
        name: Option<String>,
        arguments: String,
    },
    /// Stream completed with token usage.
    Done {
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    /// Fatal error from the provider.
    Error(String),
}

/// Trait that a model provider must implement.
/// The provider is responsible for HTTP/SSE connection and sends events
/// through the provided `Sender`. It must close the sender on completion.
pub trait ModelProvider: Send + Sync {
    /// List available models.
    fn models(&self) -> Vec<ModelInfo>;

    /// Default model id.
    fn default_model(&self) -> &str;

    /// Context window size for a model.
    fn context_limit(&self, model: &str) -> usize;

    /// Stream a chat completion. This is expected to run on a background thread
    /// or async task; it sends events through `sender` and closes it when done.
    fn stream_chat(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Vec<JsonValue>,
        sender: Sender<StreamEvent>,
    );
}

/// Parses a streaming SSE response into aggregated tool calls.
/// Mirrors SiYuan's aggregateToolCallsFromStream.
pub mod tool_parser {
    use crate::JsonValue;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AggregatedToolCall {
        pub id: String,
        pub name: String,
        pub arguments: JsonValue,
    }

    /// In-progress tool call accumulation.
    struct ToolCallAccumulator {
        id: String,
        name: Option<String>,
        arguments: Vec<String>,
    }

    /// Aggregator that collects streaming fragments into complete tool calls.
    #[derive(Default)]
    pub struct ToolCallAggregator {
        calls: BTreeMap<String, ToolCallAccumulator>,
    }

    impl ToolCallAggregator {
        pub fn new() -> Self {
            Self {
                calls: BTreeMap::new(),
            }
        }

        /// Feed a fragment. Returns the completed call if this fragment finished it.
        pub fn feed(
            &mut self,
            id: &str,
            name: Option<&str>,
            args_fragment: &str,
            is_done: bool,
        ) -> Option<AggregatedToolCall> {
            use std::collections::btree_map::Entry;

            let acc = match self.calls.entry(id.to_string()) {
                Entry::Vacant(e) => e.insert(ToolCallAccumulator {
                    id: id.to_string(),
                    name: name.map(|n| n.to_string()),
                    arguments: Vec::new(),
                }),
                Entry::Occupied(e) => {
                    let a = e.into_mut();
                    if let Some(n) = name {
                        a.name = Some(n.to_string());
                    }
                    a
                }
            };
            if !args_fragment.is_empty() {
                acc.arguments.push(args_fragment.to_string());
            }

            if is_done {
                let tc = self.calls.remove(id)?;
                let args_str = tc.arguments.join("");
                let parsed: JsonValue = serde_json::from_str(&args_str).unwrap_or(JsonValue::Null);
                Some(AggregatedToolCall {
                    id: tc.id,
                    name: tc.name.unwrap_or_default(),
                    arguments: parsed,
                })
            } else {
                None
            }
        }

        pub fn pending_count(&self) -> usize {
            self.calls.len()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn aggregates_fragments() {
            let mut a = ToolCallAggregator::new();
            assert!(
                a.feed("call_1", Some("block.get_summary"), r#"{"block"_"#, false)
                    .is_none()
            );
            assert!(a.feed("call_1", None, r#""id":"abc"}"#, true).is_some());
            assert_eq!(a.pending_count(), 0);
        }

        #[test]
        fn multiple_concurrent_calls() {
            let mut a = ToolCallAggregator::new();
            a.feed("a", Some("tool_a"), "{}", false);
            a.feed("b", Some("tool_b"), "{}", false);
            assert_eq!(a.pending_count(), 2);
            assert!(a.feed("a", None, "", true).is_some());
            assert_eq!(a.pending_count(), 1);
        }
    }
}
