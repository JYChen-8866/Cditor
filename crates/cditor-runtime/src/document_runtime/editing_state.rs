use super::*;

/// Active text-input ownership and its synchronous hot-path state.
///
/// Composition is owned by `EditingSession`; it cannot outlive or diverge from
/// the input session identity. Undo grouping remains in `HistoryState` (R4-004).
#[derive(Debug, Default)]
pub(super) struct EditingState {
    pub(super) session: Option<EditingSession>,
    pub(super) next_input_session_id: u64,
    pub(super) hot_path: SingleCharInputHotPath,
    pub(super) typing_mark_override: Option<TypingMarkOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TypingMarkOverride {
    pub(super) surface_id: SurfaceId,
    pub(super) offset: usize,
    pub(super) marks: Vec<InlineMark>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_preserves_composition_session_and_commit_contract() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "ab",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 1).unwrap();
        let identity_before = runtime.input_session_identity().unwrap();
        let revision_before = runtime.revision();
        let transaction_before = runtime.last_committed_transaction_id();

        runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

        assert!(runtime.active_composition().is_some());
        assert_ne!(runtime.input_session_identity(), Some(identity_before));
        assert_eq!(
            runtime.document.payload_window.get(1).unwrap().plain_text(),
            "ab"
        );
        assert_eq!(runtime.revision(), revision_before);

        let composition_identity = runtime.input_session_identity().unwrap();
        let outcome = runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected: composition_identity,
                input: RealtimeInput::UnmarkComposition,
            })
            .unwrap();

        assert!(runtime.active_composition().is_none());
        assert!(outcome.document_changed);
        assert_eq!(
            runtime.block_payload_record(1).unwrap().plain_text(),
            "a中b"
        );
        assert_eq!(runtime.revision(), revision_before + 1);
        assert_ne!(runtime.last_committed_transaction_id(), transaction_before);
    }
}
