//! HTTP/SSE API endpoint stubs for the Cditor agent.
//! These are placeholder definitions — the actual HTTP server is
//! provided by the desktop app (or an optional axum/actix feature).

use serde::{Deserialize, Serialize};
use crate::{AgentSessionId, JsonValue, TurnId};

// ── Request/Response types ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub session_id: Option<AgentSessionId>,
    pub message: String,
    pub language: Option<String>,
    pub model: Option<String>,
    pub references: Vec<ReferenceInput>,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceInput {
    pub id: String,
    pub title: String,
    pub r#type: String,
    pub url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub session_id: AgentSessionId,
    pub turn_id: TurnId,
    pub events_url: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    pub session_id: AgentSessionId,
    pub request_id: String,
    pub approved: bool,
    pub always: bool,
}

#[derive(Debug, Deserialize)]
pub struct QuestionRequest {
    pub session_id: AgentSessionId,
    pub question_id: String,
    pub answers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FrontendCallRequest {
    pub session_id: AgentSessionId,
    pub call_id: String,
    pub result: String,
    pub is_error: bool,
}

#[derive(Debug, Deserialize)]
pub struct AbortRequest {
    pub session_id: AgentSessionId,
    pub turn_id: Option<TurnId>,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: AgentSessionId,
    pub active: bool,
    pub turn_count: usize,
    pub created_at_ms: u64,
}

// ── SSE Event types ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEvent {
    Turn { turn_id: TurnId },
    Delta { delta: String },
    ThinkingDelta { delta: String },
    ThinkingDone,
    ToolCall { call_id: String, name: String, arguments: JsonValue },
    ToolResult { call_id: String, result: String, error: Option<String> },
    ConfirmRequired { request_id: String, summary: String },
    Question { question_id: String, title: String, options: Vec<SseOption> },
    Error { message: String },
    Usage { prompt: u64, completion: u64 },
    Done { turn_id: TurnId },
}

#[derive(Debug, Serialize)]
pub struct SseOption {
    pub label: String,
    pub description: Option<String>,
}

// ── API route definitions (stubs) ───────────────────────────────

/// POST /api/agent/chat — start a new chat turn.
/// Returns a session ID + SSE event stream URL.
pub fn route_chat(_req: ChatRequest) -> ChatResponse {
    ChatResponse {
        session_id: AgentSessionId::new_v4(),
        turn_id: TurnId::new_v4(),
        events_url: "/api/agent/events".into(),
    }
}

/// GET /api/agent/events?session_id=...&turn_id=... — SSE event stream.
/// Streams AgentEvent variants as SseEvent JSON lines.
pub fn route_events(_session_id: AgentSessionId, _turn_id: TurnId) -> Vec<SseEvent> {
    Vec::new() // Real implementation uses an async stream
}

/// POST /api/agent/confirm — answer a confirm request.
pub fn route_confirm(_req: ConfirmRequest) -> bool {
    true
}

/// POST /api/agent/question — answer a question.
pub fn route_question(_req: QuestionRequest) -> bool {
    true
}

/// POST /api/agent/frontend — return frontend tool result.
pub fn route_frontend(_req: FrontendCallRequest) -> bool {
    true
}

/// POST /api/agent/abort — abort an active turn.
pub fn route_abort(_req: AbortRequest) -> bool {
    true
}

/// GET /api/agent/sessions — list sessions.
pub fn route_list_sessions() -> Vec<SessionInfo> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_route_returns_ids() {
        let resp = route_chat(ChatRequest {
            session_id: None,
            message: "hello".into(),
            language: None,
            model: None,
            references: vec![],
        });
        assert!(!resp.events_url.is_empty());
    }

    #[test]
    fn sse_event_serialization() {
        let ev = SseEvent::Turn { turn_id: TurnId::new_v4() };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("turn"));
    }
}
