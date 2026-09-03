use cditor_core::import_plan::{ImportContent, ImportLimits, ImportReport, ImportSource};
use cditor_core::rich_text::looks_like_markdown;
use cditor_editor_protocol::{
    ProtocolError, ProtocolErrorCode,
    command::{CommandOutcome, CommandOutcomeStatus},
};
use cditor_runtime::{AiApplyMode, DocumentRuntime, ImportApplicationReport};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDispatchReport {
    pub outcome: CommandOutcome,
    pub source_report: ImportReport,
    pub before_revision: u64,
    pub revision: u64,
}

pub fn project_ai_preview_import(
    runtime: &mut DocumentRuntime,
    mode: AiApplyMode,
) -> Result<CommandOutcome, ProtocolError> {
    let Some(snapshot) = runtime.ai_session_snapshot() else {
        return Ok(CommandOutcome::no_op());
    };
    let imported = if looks_like_markdown(&snapshot.preview) {
        let (target, first_block_id) = runtime.import_target();
        let plan = cditor_import_export::import_plan::plan_markdown_import(
            &snapshot.preview,
            ImportSource::Ai,
            target,
            first_block_id,
            ImportLimits::default(),
        );
        if plan.report().rejected() {
            return Err(ProtocolError::new(
                ProtocolErrorCode::InvalidArguments,
                "AI preview exceeds import limits",
            )
            .with_document(runtime.document_id()));
        }
        match plan.content() {
            ImportContent::Markdown(document) => Some(document.clone()),
            _ => None,
        }
    } else {
        None
    };
    let before_transaction = runtime.last_committed_transaction_id();
    let focused = runtime.focused_block_id();
    let changed = runtime
        .apply_ai_preview(mode, imported)
        .map_err(|message| {
            ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
                .with_document(runtime.document_id())
        })?;
    let transaction_ids = runtime
        .last_committed_transaction_id()
        .filter(|id| Some(*id) != before_transaction)
        .into_iter()
        .collect();
    Ok(if changed {
        CommandOutcome::applied(transaction_ids, focused.into_iter().collect())
    } else {
        CommandOutcome::no_op()
    })
}

/// Applies `markdown` as a forced Markdown document import.
///
/// Unlike [`project_clipboard_import`], this never falls back to a single
/// plain-text block: the text is always parsed into typed blocks (headings,
/// lists, code, tables, and blank-line-separated paragraphs), which is what
/// external document imports need regardless of whether the text "looks like"
/// a markdown paste.
pub fn project_markdown_import(
    runtime: &mut DocumentRuntime,
    markdown: &str,
) -> Result<ImportDispatchReport, ProtocolError> {
    let before_revision = runtime.revision();
    let before_transaction = runtime.last_committed_transaction_id();
    let focused = runtime.focused_block_id();
    let (target, first_block_id) = runtime.import_target();
    let plan = cditor_import_export::import_plan::plan_markdown_import(
        markdown,
        ImportSource::Markdown,
        target,
        first_block_id,
        ImportLimits::default(),
    );
    let ImportApplicationReport {
        changed,
        revision,
        plan_report,
    } = runtime.apply_import_plan(&plan).map_err(|message| {
        let code = if plan.report().rejected() {
            ProtocolErrorCode::InvalidArguments
        } else {
            ProtocolErrorCode::ApplyFailed
        };
        ProtocolError::new(code, message).with_document(runtime.document_id())
    })?;
    if changed && runtime.revision() == before_revision {
        runtime.note_content_changed();
    }
    let transaction_ids = runtime
        .last_committed_transaction_id()
        .filter(|id| Some(*id) != before_transaction)
        .into_iter()
        .collect();
    let outcome = if changed {
        CommandOutcome::applied(transaction_ids, focused.into_iter().collect())
    } else {
        CommandOutcome::no_op()
    };
    debug_assert_eq!(outcome.status == CommandOutcomeStatus::Applied, changed);
    Ok(ImportDispatchReport {
        outcome,
        source_report: plan_report,
        before_revision,
        revision: if changed {
            runtime.revision()
        } else {
            revision
        },
    })
}

pub fn project_clipboard_import(
    runtime: &mut DocumentRuntime,
    text: &str,
    metadata_json: Option<&str>,
) -> Result<ImportDispatchReport, ProtocolError> {
    let before_revision = runtime.revision();
    let before_transaction = runtime.last_committed_transaction_id();
    let focused = runtime.focused_block_id();
    let (target, first_block_id) = runtime.import_target();
    let plan = cditor_import_export::import_plan::plan_clipboard_import(
        text,
        metadata_json,
        target,
        first_block_id,
        ImportLimits::default(),
    );
    let ImportApplicationReport {
        changed,
        revision,
        plan_report,
    } = runtime.apply_import_plan(&plan).map_err(|message| {
        let code = if plan.report().rejected() {
            ProtocolErrorCode::InvalidArguments
        } else {
            ProtocolErrorCode::ApplyFailed
        };
        ProtocolError::new(code, message).with_document(runtime.document_id())
    })?;
    if changed && runtime.revision() == before_revision {
        runtime.note_content_changed();
    }
    let transaction_ids = runtime
        .last_committed_transaction_id()
        .filter(|id| Some(*id) != before_transaction)
        .into_iter()
        .collect();
    let outcome = if changed {
        CommandOutcome::applied(transaction_ids, focused.into_iter().collect())
    } else {
        CommandOutcome::no_op()
    };
    debug_assert_eq!(outcome.status == CommandOutcomeStatus::Applied, changed);
    Ok(ImportDispatchReport {
        outcome,
        source_report: plan_report,
        before_revision,
        revision: if changed {
            runtime.revision()
        } else {
            revision
        },
    })
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, RichBlockKind, TablePayload, TableRange,
    };
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    fn command(command: EditorCommand) -> CommandEnvelope {
        CommandEnvelope::new(command, CommandSource::Import)
    }

    #[test]
    fn session_plans_markdown_and_applies_it_as_one_undo_unit() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Automation,
            ))
            .unwrap();
        handle
            .dispatch(command(EditorCommand::ApplyMarkdownImport {
                text: "# Title\n- one\n- two".to_owned(),
            }))
            .unwrap();
        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.block_count, 3);
        assert!(matches!(
            handle.text_block_context(1).unwrap().unwrap().kind,
            RichBlockKind::Heading { level: 1 }
        ));
        assert_eq!(handle.text_block_context(3).unwrap().unwrap().text, "one");
        handle.dispatch(command(EditorCommand::Undo)).unwrap();
        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.block_count, 1);
        assert_eq!(handle.text_block_context(1).unwrap().unwrap().text, "");
    }

    #[test]
    fn markdown_import_splits_plain_prose_into_paragraph_blocks() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Automation,
            ))
            .unwrap();
        handle
            .dispatch(command(EditorCommand::ApplyMarkdownImport {
                text: "first paragraph\n\nsecond paragraph".to_owned(),
            }))
            .unwrap();
        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(
            snapshot.block_count, 2,
            "plain prose must not collapse into one block"
        );
        assert_eq!(
            handle.text_block_context(1).unwrap().unwrap().kind,
            RichBlockKind::Paragraph
        );
        assert_eq!(
            handle.text_block_context(1).unwrap().unwrap().text,
            "first paragraph"
        );
        assert_eq!(
            handle.text_block_context(3).unwrap().unwrap().text,
            "second paragraph"
        );
    }

    #[test]
    fn markdown_import_parses_typed_blocks_without_heuristics() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Automation,
            ))
            .unwrap();
        handle
            .dispatch(command(EditorCommand::ApplyMarkdownImport {
                text: "# Title\n- one\n- two\n\n```rust\nfn main() {}\n```".to_owned(),
            }))
            .unwrap();
        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.block_count, 4);
        assert!(matches!(
            handle.text_block_context(1).unwrap().unwrap().kind,
            RichBlockKind::Heading { level: 1 }
        ));
        for block_id in [3, 4] {
            assert_eq!(
                handle.text_block_context(block_id).unwrap().unwrap().kind,
                RichBlockKind::BulletedList,
                "block {block_id} should be a bulleted list item"
            );
        }
        assert!(matches!(
            handle.text_block_context(5).unwrap().unwrap().kind,
            RichBlockKind::Code { .. }
        ));
    }

    #[test]
    fn clipboard_import_keeps_single_line_prose_as_one_plain_text_block() {
        // Regression guard: 单行散文粘贴不进 markdown 解析，仍是一个纯文本块。
        // 多行粘贴按行/空行拆块，见
        // `clipboard_import_splits_multi_line_prose_into_blocks`。
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Automation,
            ))
            .unwrap();
        handle
            .dispatch(command(EditorCommand::ApplyClipboardData {
                text: "first paragraph and #not a heading".to_owned(),
                metadata_json: None,
            }))
            .unwrap();
        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.block_count, 1);
        assert_eq!(
            handle.block_plain_text(1).unwrap().as_deref(),
            Some("first paragraph and #not a heading")
        );
    }

    #[test]
    fn clipboard_import_splits_multi_line_prose_into_blocks() {
        // 粘贴多行文本按块拆分（`fix(markdown): paste lines into separate
        // blocks and keep --- as divider`）：每段落一个块，而不是把换行塞进
        // 同一个块。
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Automation,
            ))
            .unwrap();
        handle
            .dispatch(command(EditorCommand::ApplyClipboardData {
                text: "first paragraph\n\nsecond paragraph".to_owned(),
                metadata_json: None,
            }))
            .unwrap();
        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.block_count, 2);
        assert_eq!(
            handle.text_block_context(1).unwrap().unwrap().text,
            "first paragraph"
        );
        assert_eq!(
            handle.block_plain_text(3).unwrap().as_deref(),
            Some("second paragraph")
        );
    }

    #[test]
    fn malformed_metadata_is_reported_and_plain_text_still_applies() {
        let mut runtime = DocumentRuntime::empty();
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Automation,
            ))
            .unwrap();
        let report = project_clipboard_import(&mut runtime, "plain", Some("{bad"))
            .expect("malformed optional metadata must fall back to system text");
        assert!(report.outcome.changed());
        assert!(
            report
                .source_report
                .diagnostics
                .iter()
                .any(|item| item.code == "clipboard_metadata_rejected")
        );
        assert_eq!(
            runtime.block_payload_record(1).unwrap().plain_text(),
            "plain"
        );
    }

    #[test]
    fn readonly_and_stale_clipboard_targets_are_rejected_before_apply() {
        let readonly = EditorSession::new(DocumentRuntime::empty(), true).into_handle();
        let error = readonly
            .dispatch(command(EditorCommand::ApplyClipboardData {
                text: "x".to_owned(),
                metadata_json: None,
            }))
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::Readonly);

        let writable = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        let error = writable
            .dispatch(
                command(EditorCommand::ApplyClipboardData {
                    text: "x".to_owned(),
                    metadata_json: None,
                })
                .expecting_revision(99),
            )
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::StalePrecondition);
        assert_eq!(writable.block_plain_text(1).unwrap().as_deref(), Some(""));
    }

    #[test]
    fn session_dispatches_preparsed_tsv_to_the_focused_table() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 10,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(TablePayload::default()),
                content_version: 0,
            }],
            720.0,
        );
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusTableCell {
                    block_id: 10,
                    row: 0,
                    col: 0,
                    offset: Some(0),
                    affinity: cditor_core::edit::TextAffinity::Downstream,
                },
                CommandSource::Automation,
            ))
            .unwrap();
        handle
            .dispatch(command(EditorCommand::ApplyClipboardData {
                text: "A\tB\nC\tD".to_owned(),
                metadata_json: None,
            }))
            .unwrap();
        let selection = handle
            .table_clipboard_selection(10, TableRange::normalized(0, 0, 1, 1))
            .unwrap()
            .unwrap();
        assert_eq!(selection.plain_text(), "A\tB\nC\tD");
    }
}
