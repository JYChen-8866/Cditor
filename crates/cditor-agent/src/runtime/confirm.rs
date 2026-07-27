//! Confirmation session — manages pending confirm/ask channels between
//! the agent engine and the frontend UI.
//!
//! Mirrors SiYuan's handleConfirm / handleQuestion flow.
//! The engine posts a confirm/question request and awaits a response
//! through a oneshot channel. The frontend resolves it via answer().

use std::collections::HashMap;
use tokio::sync::oneshot;

use crate::tools::mutation::{AgentMutationIntent, AgentMutationPreview};

// ── Types ────────────────────────────────────────────────────────

/// A pending confirm request waiting for frontend response.
#[derive(Debug)]
pub struct ConfirmRequest {
    pub request_id: String,
    pub summary: String,
    pub intent: AgentMutationIntent,
    pub preview: Option<AgentMutationPreview>,
    pub created_at_ms: u64,
}

/// Response from frontend to a confirm request.
#[derive(Debug, Clone)]
pub struct ConfirmAnswer {
    pub request_id: String,
    pub approved: bool,
    pub always: bool,
}

/// A pending question request (model asked the user).
#[derive(Debug, Clone)]
pub struct QuestionRequest {
    pub question_id: String,
    pub title: String,
    pub options: Vec<QuestionOption>,
    pub created_at_ms: u64,
}

/// A choice option for the question tool.
#[derive(Debug, Clone)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// Response from frontend to a question.
#[derive(Debug, Clone)]
pub struct QuestionAnswer {
    pub question_id: String,
    pub answers: Vec<String>,
}

// ── Pending registry ──────────────────────────────────────────────

struct PendingConfirm {
    request: ConfirmRequest,
    sender: oneshot::Sender<ConfirmAnswer>,
}

struct PendingQuestion {
    request: QuestionRequest,
    sender: oneshot::Sender<QuestionAnswer>,
}

/// Global confirm/question session.
/// The engine posts requests; the frontend resolves them asynchronously.
pub struct ConfirmSession {
    pending_confirms: HashMap<String, PendingConfirm>,
    pending_questions: HashMap<String, PendingQuestion>,
}

impl ConfirmSession {
    pub fn new() -> Self {
        Self {
            pending_confirms: HashMap::new(),
            pending_questions: HashMap::new(),
        }
    }

    /// Post a confirm request. Returns a receiver that the engine awaits.
    pub fn request_confirm(
        &mut self,
        request: ConfirmRequest,
    ) -> oneshot::Receiver<ConfirmAnswer> {
        let id = request.request_id.clone();
        let (tx, rx) = oneshot::channel();
        self.pending_confirms.insert(id, PendingConfirm {
            request,
            sender: tx,
        });
        rx
    }

    /// Post a question. Returns a receiver that the engine awaits.
    pub fn request_question(
        &mut self,
        request: QuestionRequest,
    ) -> oneshot::Receiver<QuestionAnswer> {
        let id = request.question_id.clone();
        let (tx, rx) = oneshot::channel();
        self.pending_questions.insert(id, PendingQuestion {
            request,
            sender: tx,
        });
        rx
    }

    /// Frontend answers a pending confirm.
    pub fn answer_confirm(
        &mut self,
        response: ConfirmAnswer,
    ) -> Result<(), ConfirmAnswer> {
        if let Some(pending) = self.pending_confirms.remove(&response.request_id) {
            let _ = pending.sender.send(response);
            Ok(())
        } else {
            Err(response)
        }
    }

    /// Frontend answers a pending question.
    pub fn answer_question(
        &mut self,
        response: QuestionAnswer,
    ) -> Result<(), QuestionAnswer> {
        if let Some(pending) = self.pending_questions.remove(&response.question_id) {
            let _ = pending.sender.send(response);
            Ok(())
        } else {
            Err(response)
        }
    }

    /// List pending confirm requests (for UI display).
    pub fn pending_confirms_list(&self) -> Vec<&ConfirmRequest> {
        self.pending_confirms
            .values()
            .map(|p| &p.request)
            .collect()
    }

    /// List pending questions (for UI display).
    pub fn pending_questions_list(&self) -> Vec<&QuestionRequest> {
        self.pending_questions
            .values()
            .map(|p| &p.request)
            .collect()
    }

    /// Cancel a confirm request (e.g., timeout or user closed document).
    pub fn cancel_confirm(&mut self, request_id: &str) {
        self.pending_confirms.remove(request_id);
    }

    /// Cancel a question request.
    pub fn cancel_question(&mut self, question_id: &str) {
        self.pending_questions.remove(question_id);
    }

    /// Check if a confirm is still pending.
    pub fn has_pending_confirm(&self, request_id: &str) -> bool {
        self.pending_confirms.contains_key(request_id)
    }

    /// Timeout duration for confirm requests (mirroring SiYuan 5min).
    pub fn confirmation_timeout_ms() -> u64 {
        300_000 // 5 minutes
    }

    /// Timeout duration for question requests.
    pub fn question_timeout_ms() -> u64 {
        300_000 // 5 minutes
    }
}

impl Default for ConfirmSession {
    fn default() -> Self {
        Self::new()
    }
}

// ── Integration with the engine ───────────────────────────────────

/// High-level convenience for the engine to post a confirm and get the answer.
/// Returns Ok(answer) or Err if timeout.
pub async fn await_confirm(
    session: &mut ConfirmSession,
    _turn_id: uuid::Uuid,
    intent: AgentMutationIntent,
    preview: Option<AgentMutationPreview>,
) -> Result<ConfirmAnswer, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let request_id = uuid::Uuid::new_v4().to_string();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let request = ConfirmRequest {
        request_id: request_id.clone(),
        summary: format!("{:?}", intent.kind()),
        intent,
        preview,
        created_at_ms: now,
    };

    let rx = session.request_confirm(request);

    // Await with timeout
    let timeout = tokio::time::timeout(
        std::time::Duration::from_millis(ConfirmSession::confirmation_timeout_ms()),
        rx,
    )
    .await;

    match timeout {
        Ok(Ok(answer)) if answer.request_id == request_id => Ok(answer),
        Ok(Ok(answer)) => Err(format!("request_id mismatch: {}", answer.request_id)),
        Ok(Err(_)) => {
            session.cancel_confirm(&request_id);
            Err("confirm channel closed".into())
        }
        Err(_) => {
            session.cancel_confirm(&request_id);
            Err("confirmation timed out".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_request_response_flow() {
        let mut session = ConfirmSession::new();
        let request = ConfirmRequest {
            request_id: "r1".into(),
            summary: "replace block".into(),
            intent: AgentMutationIntent::ReplaceBlock {
                target: crate::tools::mutation::VersionedBlockRef {
                    block_id: uuid::Uuid::new_v4(),
                    content_version: 1,
                },
                content: crate::tools::mutation::AgentContent::Markdown {
                    source: "hello".into(),
                    dialect: crate::tools::read::MarkdownDialect::CommonMark,
                },
            },
            preview: None,
            created_at_ms: 1000,
        };

        let _rx = session.request_confirm(request);
        assert_eq!(session.pending_confirms_list().len(), 1);

        let answer = ConfirmAnswer {
            request_id: "r1".into(),
            approved: true,
            always: false,
        };
        session.answer_confirm(answer).unwrap();
        assert_eq!(session.pending_confirms_list().len(), 0);
    }

    #[test]
    fn question_request_response_flow() {
        let mut session = ConfirmSession::new();
        let request = QuestionRequest {
            question_id: "q1".into(),
            title: "Choose style".into(),
            options: vec![
                QuestionOption { label: "Formal".into(), description: None },
                QuestionOption { label: "Casual".into(), description: None },
            ],
            created_at_ms: 1000,
        };

        let _rx = session.request_question(request);
        assert_eq!(session.pending_questions_list().len(), 1);

        session.answer_question(QuestionAnswer {
            question_id: "q1".into(),
            answers: vec!["Formal".into()],
        }).unwrap();
        assert_eq!(session.pending_questions_list().len(), 0);
    }

    #[test]
    fn cancel_removes_pending() {
        let mut session = ConfirmSession::new();
        let request = ConfirmRequest {
            request_id: "r1".into(),
            summary: "delete".into(),
            intent: AgentMutationIntent::DeleteBlocks {
                targets: vec![],
            },
            preview: None,
            created_at_ms: 1000,
        };
        let _rx = session.request_confirm(request);
        session.cancel_confirm("r1");
        assert_eq!(session.pending_confirms_list().len(), 0);
    }

    #[test]
    fn answer_nonexistent_returns_err() {
        let mut session = ConfirmSession::new();
        let result = session.answer_confirm(ConfirmAnswer {
            request_id: "nonexistent".into(),
            approved: true,
            always: false,
        });
        assert!(result.is_err());
    }
}
