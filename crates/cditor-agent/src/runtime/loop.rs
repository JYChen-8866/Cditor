//! AgentRuntime — the main orchestrator that ties the model provider,
//! streaming parser, and AgentService state machine together.
//!
//! Mirrors SiYuan's agentChat() loop (agent.go:434-1068).

use async_channel::{Receiver, Sender};

use crate::model::tool_parser::{AggregatedToolCall, ToolCallAggregator};
use crate::model::{ModelProvider, StreamEvent};
use crate::protocol::context::AgentContextSnapshot;
use crate::protocol::event::AgentEvent;
use crate::runtime::engine::{AgentService, AggregatedToolCall as EngineAggregatedCall};
use crate::{JsonValue, TurnId};

/// Results of a streaming call to the model.
struct StreamResult {
    text: String,
    aggregated_calls: Vec<AggregatedToolCall>,
    prompt_tokens: u64,
    completion_tokens: u64,
}

/// The runtime orchestrates the agent loop per turn.
pub struct AgentRuntime {
    pub service: AgentService,
    provider: std::sync::Arc<dyn ModelProvider>,
    model: String,
    event_tx: Sender<AgentEvent>,
    event_rx: Receiver<AgentEvent>,
    max_tool_rounds: usize,
}

impl AgentRuntime {
    pub fn new(
        service: AgentService,
        provider: std::sync::Arc<dyn ModelProvider>,
        model: String,
    ) -> Self {
        let (tx, rx) = async_channel::bounded(256);
        Self {
            service,
            provider,
            model,
            event_tx: tx,
            event_rx: rx,
            max_tool_rounds: 20,
        }
    }

    /// Event receiver — the caller drains this to feed the UI.
    pub fn event_receiver(&self) -> Receiver<AgentEvent> {
        self.event_rx.clone()
    }

    /// Emit an event (non-blocking, drops if channel full).
    fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.try_send(event);
    }

    /// Run one turn end-to-end.
    /// This is a blocking call meant for a background thread.
    pub fn run_turn(
        &mut self,
        user_message: &str,
        ctx: &AgentContextSnapshot,
    ) -> Result<(), String> {
        let snapshot_id = ctx.snapshot_id;
        let turn_id = self.service.begin_turn(user_message, ctx);
        self.emit(AgentEvent::Turn {
            turn_id,
            base_revision: ctx.document_revision as i64,
        });
        self.emit(AgentEvent::TurnStarted {
            turn_id,
            context: snapshot_id,
        });

        // ── Main loop ──────────────────────────────────────────────
        let mut round = 0;
        loop {
            if self.service.is_done() {
                break;
            }
            round += 1;
            if round > self.max_tool_rounds {
                self.emit(AgentEvent::Failed {
                    code: crate::protocol::error::AgentErrorCode::BudgetExceeded,
                    message: "max tool rounds reached".into(),
                    retryable: false,
                    critical: false,
                });
                break;
            }

            // 1. Stream model response (with retry)
            let stream_result = match self.stream_with_retry(turn_id) {
                Ok(r) => r,
                Err(e) => {
                    self.emit(AgentEvent::Failed {
                        code: crate::protocol::error::AgentErrorCode::Internal,
                        message: e,
                        retryable: false,
                        critical: true,
                    });
                    return Err("stream failed".into());
                }
            };

            // 2. Feed response to engine
            self.emit(AgentEvent::Usage {
                prompt_tokens: stream_result.prompt_tokens,
                cached_tokens: 0,
                completion_tokens: stream_result.completion_tokens,
            });
            self.service.record_usage(
                stream_result.prompt_tokens,
                0,
                stream_result.completion_tokens,
            );

            let engine_calls: Vec<EngineAggregatedCall> = stream_result
                .aggregated_calls
                .iter()
                .map(|c| {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    let args_str = serde_json::to_string(&c.arguments).unwrap_or_default();
                    args_str.hash(&mut h);
                    EngineAggregatedCall {
                        id: uuid::Uuid::parse_str(&c.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                        name: c.name.clone(),
                        arguments: args_str,
                        args_hash: h.finish(),
                    }
                })
                .collect();

            self.service
                .accept_assistant_response(&stream_result.text, &engine_calls);

            // 3. Execute pending tool calls
            while self.service.has_pending_tool_calls() {
                let call = self.service.current_tool_call().cloned();
                if let Some(call) = call {
                    let name = call.name.clone();
                    let call_id = call.id;

                    // Emit tool call started
                    self.emit(AgentEvent::ToolCallStarted {
                        call_id,
                        name: name.clone(),
                        safe_args: JsonValue::Null,
                    });

                    // Check confirmation
                    if self.service.needs_confirmation(&name) {
                        // For now, deny unconfirmed writes (Section 3 will add real flow)
                        self.emit(AgentEvent::Failed {
                            code: crate::protocol::error::AgentErrorCode::ConfirmationRequired,
                            message: format!("tool {} requires confirmation", name),
                            retryable: false,
                            critical: false,
                        });
                        continue;
                    }

                    // Execute
                    match self.service.execute_tool(&call) {
                        Ok(event) => {
                            self.emit(event);
                        }
                        Err(code) => {
                            self.emit(AgentEvent::Failed {
                                code,
                                message: format!("tool {} execution failed", name),
                                retryable: false,
                                critical: false,
                            });
                        }
                    }
                }
            }
        }

        self.emit(AgentEvent::TurnCompleted { turn_id });
        Ok(())
    }

    /// Stream model response with retry logic.
    fn stream_with_retry(&self, turn_id: TurnId) -> Result<StreamResult, String> {
        let mut attempt = 0;
        let max_retries = 5;
        let messages = self.service.provider_messages().to_vec();
        let tools = self.service.tool_definitions();
        let event_tx = self.event_tx.clone();

        loop {
            attempt += 1;

            // Spawn provider call on a background thread
            let msgs = messages.clone();
            let tls = tools.clone();
            let model = self.model.clone();

            // Can't borrow self.provider in a thread, so we use unsafe or restructure.
            // For now use a simple call pattern: the provider's stream_chat is synchronous.
            // We call it on the current thread.
            let result = Self::collect_stream_from_provider(
                std::sync::Arc::clone(&self.provider),
                &model,
                msgs,
                tls,
                turn_id,
                &event_tx,
            );

            match result {
                Ok(r) => return Ok(r),
                Err(err) => {
                    let cat = AgentService::classify_retry_error(&err);
                    if cat == "unretryable" || attempt > max_retries {
                        return Err(err);
                    }
                    let delay = AgentService::delay_for_category(cat, attempt);
                    std::thread::sleep(delay);
                }
            }
        }
    }

    /// Call provider and collect stream on current thread.
    fn collect_stream_from_provider(
        provider: std::sync::Arc<dyn ModelProvider>,
        model: &str,
        messages: Vec<crate::runtime::engine::ChatMessage>,
        tools: Vec<JsonValue>,
        turn_id: TurnId,
        event_tx: &Sender<AgentEvent>,
    ) -> Result<StreamResult, String> {
        let (stream_tx, stream_rx): (Sender<StreamEvent>, Receiver<StreamEvent>) =
            async_channel::bounded(64);

        // Clone what we need for the thread
        let model_owned = model.to_string();
        let event_tx2 = event_tx.clone();
        
        // Call provider synchronously on current thread
        provider.stream_chat(&model_owned, messages, tools, stream_tx);

        // Drain stream
        Self::collect_stream(stream_rx, turn_id, &event_tx2)
    }

    /// Drain a stream receiver, emitting deltas and aggregating tool calls.
    fn collect_stream(
        rx: Receiver<StreamEvent>,
        turn_id: TurnId,
        event_tx: &Sender<AgentEvent>,
    ) -> Result<StreamResult, String> {
        let mut text = String::new();
        let mut aggregator = ToolCallAggregator::new();
        let mut completed_calls: Vec<AggregatedToolCall> = Vec::new();
        let mut prompt_tokens = 0u64;
        let mut completion_tokens = 0u64;

        loop {
            match rx.recv_blocking() {
                Ok(StreamEvent::Delta(delta)) => {
                    text.push_str(&delta);
                    let _ = event_tx.try_send(AgentEvent::ModelTextDelta { turn_id, delta });
                }
                Ok(StreamEvent::ThinkingDelta(delta)) => {
                    let _ = event_tx.try_send(AgentEvent::ThinkingDelta { delta });
                }
                Ok(StreamEvent::ThinkingDone) => {
                    let _ = event_tx.try_send(AgentEvent::ThinkingDone);
                }
                Ok(StreamEvent::ToolCallFragment {
                    id,
                    name,
                    arguments,
                }) => {
                    if let Some(call) = aggregator.feed(&id, name.as_deref(), &arguments, false) {
                        completed_calls.push(call);
                    }
                }
                Ok(StreamEvent::Done {
                    prompt_tokens: p,
                    completion_tokens: c,
                }) => {
                    prompt_tokens = p;
                    completion_tokens = c;
                    break;
                }
                Ok(StreamEvent::Error(e)) => {
                    return Err(e);
                }
                Err(_) => {
                    // Channel closed — check if we got Done
                    break;
                }
            }
        }

        Ok(StreamResult {
            text,
            aggregated_calls: completed_calls,
            prompt_tokens,
            completion_tokens,
        })
    }
}
