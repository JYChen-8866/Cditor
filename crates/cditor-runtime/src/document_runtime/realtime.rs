use std::{fmt, ops::Range};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeInput<'a> {
    ReplaceText {
        range: Option<Range<usize>>,
        text: &'a str,
    },
    UpdateComposition {
        range: Range<usize>,
        text: &'a str,
        selected_range: Option<Range<usize>>,
    },
    UnmarkComposition,
    CommitBeforeExternalFocus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimeInputRequest<'a> {
    pub expected: InputSessionIdentity,
    pub input: RealtimeInput<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeInputOutcome {
    pub document_changed: bool,
    pub state_changed: bool,
    pub revision: u64,
    pub transaction_id: Option<u64>,
    pub input_identity: Option<InputSessionIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeInputError {
    StaleIdentity {
        expected: InputSessionIdentity,
        current: Box<Option<InputSessionIdentity>>,
    },
    ApplyFailed(String),
}

impl fmt::Display for RealtimeInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleIdentity { expected, current } => write!(
                formatter,
                "stale realtime input identity: expected {expected:?}, current {current:?}"
            ),
            Self::ApplyFailed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RealtimeInputError {}

impl DocumentRuntime {
    /// Synchronous platform-input port. Validation and mutation deliberately
    /// share one Runtime borrow so focus/version changes cannot race the edit.
    pub fn apply_realtime_input(
        &mut self,
        request: RealtimeInputRequest<'_>,
    ) -> Result<RealtimeInputOutcome, RealtimeInputError> {
        let current = self.input_session_identity();
        if current != Some(request.expected) {
            return Err(RealtimeInputError::StaleIdentity {
                expected: request.expected,
                current: Box::new(current),
            });
        }

        let before_revision = self.revision();
        let before_transaction = self.last_committed_transaction_id();
        let before_identity = current;
        let document_changed = match request.input {
            RealtimeInput::ReplaceText { range, text } => {
                if text.is_empty() && self.has_active_selection() {
                    self.delete_active_selection()
                } else {
                    self.replace_text_from_platform(range, text)
                }
                .map_err(RealtimeInputError::ApplyFailed)?
            }
            RealtimeInput::UpdateComposition {
                range,
                text,
                selected_range,
            } => {
                self.begin_or_update_composition_with_selection(
                    request.expected.target.block_id(),
                    range,
                    text,
                    selected_range,
                )
                .map_err(RealtimeInputError::ApplyFailed)?;
                false
            }
            RealtimeInput::UnmarkComposition => {
                if self.active_composition().is_some() {
                    self.commit_composition()
                        .map_err(RealtimeInputError::ApplyFailed)?
                } else {
                    self.cancel_composition();
                    false
                }
            }
            RealtimeInput::CommitBeforeExternalFocus => self
                .commit_composition_before_external_focus()
                .map_err(RealtimeInputError::ApplyFailed)?,
        };
        if document_changed && self.revision() == before_revision {
            self.note_content_changed();
        }
        let input_identity = self.input_session_identity();
        let transaction_id = self
            .last_committed_transaction_id()
            .filter(|transaction| Some(*transaction) != before_transaction);
        Ok(RealtimeInputOutcome {
            document_changed,
            state_changed: document_changed
                || before_identity != input_identity
                || before_revision != self.revision(),
            revision: self.revision(),
            transaction_id,
            input_identity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_runtime(text: &str) -> DocumentRuntime {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                text,
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, text.len()).unwrap();
        runtime
    }

    #[test]
    fn realtime_replace_returns_next_identity_revision_and_transaction() {
        let mut runtime = text_runtime("a");
        let expected = runtime.input_session_identity().unwrap();
        let before_revision = runtime.revision();

        let outcome = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::ReplaceText {
                    range: None,
                    text: "中",
                },
            })
            .unwrap();

        assert!(outcome.document_changed);
        assert!(outcome.state_changed);
        assert_eq!(outcome.revision, before_revision + 1);
        assert!(outcome.transaction_id.is_some());
        assert_ne!(outcome.input_identity, Some(expected));
        assert_eq!(runtime.focused_text(), Some("a中"));
    }

    #[test]
    fn stale_realtime_identity_rejects_without_any_mutation() {
        let mut runtime = text_runtime("ab");
        let stale = runtime.input_session_identity().unwrap();
        runtime.focus_block_at_offset(1, 0).unwrap();
        let before_revision = runtime.revision();
        let before_transaction = runtime.last_committed_transaction_id();
        let before_text = runtime.focused_text().map(ToOwned::to_owned);

        let error = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected: stale,
                input: RealtimeInput::ReplaceText {
                    range: None,
                    text: "x",
                },
            })
            .unwrap_err();

        assert!(matches!(error, RealtimeInputError::StaleIdentity { .. }));
        assert_eq!(runtime.revision(), before_revision);
        assert_eq!(runtime.last_committed_transaction_id(), before_transaction);
        assert_eq!(runtime.focused_text(), before_text.as_deref());
        assert!(runtime.active_composition().is_none());
    }

    #[test]
    fn composition_update_and_commit_chain_return_versioned_identities() {
        let mut runtime = text_runtime("ab");
        runtime.focus_block_at_offset(1, 1).unwrap();
        let expected = runtime.input_session_identity().unwrap();
        let before_revision = runtime.revision();

        let preview = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::UpdateComposition {
                    range: 1..1,
                    text: "你",
                    selected_range: Some("你".len().."你".len()),
                },
            })
            .unwrap();
        assert!(!preview.document_changed);
        assert!(preview.state_changed);
        assert_eq!(preview.revision, before_revision);
        assert_eq!(runtime.composition_preview_text().as_deref(), Some("a你b"));

        let committed = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected: preview.input_identity.unwrap(),
                input: RealtimeInput::UnmarkComposition,
            })
            .unwrap();
        assert!(committed.document_changed);
        assert_eq!(committed.revision, before_revision + 1);
        assert_eq!(runtime.focused_text(), Some("a你b"));
        assert!(runtime.active_composition().is_none());
    }

    #[test]
    fn empty_replacement_deletes_the_active_document_selection() {
        let mut runtime = text_runtime("abcd");
        runtime.set_document_text_selection(1, 1, 1, 3).unwrap();
        let expected = runtime.input_session_identity().unwrap();

        let outcome = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::ReplaceText {
                    range: Some(1..3),
                    text: "",
                },
            })
            .unwrap();

        assert!(outcome.document_changed);
        assert_eq!(runtime.focused_text(), Some("ad"));
        assert!(!runtime.has_active_selection());
    }

    #[test]
    fn external_focus_commit_is_versioned_and_commits_once() {
        let mut runtime = text_runtime("ab");
        runtime.focus_block_at_offset(1, 1).unwrap();
        let expected = runtime.input_session_identity().unwrap();
        let preview = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::UpdateComposition {
                    range: 1..1,
                    text: "中",
                    selected_range: None,
                },
            })
            .unwrap();

        let committed = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected: preview.input_identity.unwrap(),
                input: RealtimeInput::CommitBeforeExternalFocus,
            })
            .unwrap();

        assert!(committed.document_changed);
        assert_eq!(runtime.focused_text(), Some("a中b"));
        assert!(runtime.active_composition().is_none());
    }
}
