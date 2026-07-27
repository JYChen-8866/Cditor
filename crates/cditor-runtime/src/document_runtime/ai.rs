use super::ai_utils::{ai_selection_fingerprint, bounded_prefix, bounded_suffix};
use super::*;

use cditor_ai::{AiCancellationToken, AiProviderRequest, AiStreamEvent, AiTaskKind};

const MAX_AI_CONTEXT_BYTES: usize = 4 * 1024;
const MAX_AI_PREVIEW_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeAiTarget {
    InlineCaret(TextPosition),
    TextSelection(DocumentSelection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiApplyMode {
    Replace,
    InsertAfter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiRequestPresentation {
    Automatic,
    AssistantPanel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiSessionStatus {
    Streaming,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct AiRequestDispatch {
    pub request: AiProviderRequest,
    pub cancellation: AiCancellationToken,
    pub redacted_fields: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSessionSnapshot {
    pub request_id: u64,
    pub target: RuntimeAiTarget,
    pub selection_fingerprint: u64,
    pub instruction: String,
    pub preview: String,
    pub status: AiSessionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiStreamApplyResult {
    Applied,
    IgnoredRequest,
    DiscardedStale,
    RejectedTooLarge,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeAiSession {
    request_id: u64,
    target: RuntimeAiTarget,
    target_block_versions: Vec<(BlockId, u64)>,
    selection_fingerprint: u64,
    preview_kind: AiPreviewKind,
    instruction: String,
    preview: String,
    status: AiSessionStatus,
    cancellation: AiCancellationToken,
}

fn empty_text_kind_supports_ai(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::Paragraph
            | RichBlockKind::Heading { .. }
            | RichBlockKind::Quote
            | RichBlockKind::Todo { .. }
            | RichBlockKind::BulletedList
            | RichBlockKind::NumberedList
            | RichBlockKind::Toggle
            | RichBlockKind::Callout { .. }
    )
}

impl DocumentRuntime {
    pub(super) fn ai_payload_pin_ids(&self) -> Vec<BlockId> {
        self.ai_session
            .as_ref()
            .map(|session| {
                session
                    .target_block_versions
                    .iter()
                    .map(|(block_id, _)| *block_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn focused_empty_text_block_for_ai(&self) -> Option<(BlockId, usize)> {
        if self.selection.document_selection.is_some()
            || self.has_selected_blocks()
            || self.selection.focused_table_cell.is_some()
            || self.active_composition().is_some()
        {
            return None;
        }
        let block_id = self.focused_block_id()?;
        let payload = self.document.payload_window.get(block_id)?;
        if !empty_text_kind_supports_ai(&payload.kind) {
            return None;
        }
        let text = self.document.text_models.get(&block_id)?.text();
        text.is_empty()
            .then(|| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)))
    }

    #[cfg(test)]
    pub(crate) fn begin_ai_request(
        &mut self,
        instruction: impl Into<String>,
    ) -> Result<AiRequestDispatch, String> {
        self.begin_ai_request_with_presentation(instruction, AiRequestPresentation::Automatic)
    }

    pub(crate) fn begin_ai_request_with_presentation(
        &mut self,
        instruction: impl Into<String>,
        presentation: AiRequestPresentation,
    ) -> Result<AiRequestDispatch, String> {
        if self.active_composition().is_some() {
            return Err("finish text composition before starting Inline AI".to_owned());
        }
        if self.has_selected_blocks() {
            return Err("Inline AI currently requires a caret or text selection".to_owned());
        }
        let instruction = instruction.into().trim().to_owned();
        if instruction.is_empty() {
            return Err("AI instruction cannot be empty".to_owned());
        }
        self.cancel_ai_request();

        let target = if let Some(selection) = self
            .selection
            .document_selection
            .filter(|selection| !selection.is_caret())
        {
            RuntimeAiTarget::TextSelection(selection)
        } else {
            let block_id = self
                .focused_block_id()
                .ok_or_else(|| "Inline AI requires a focused text block".to_owned())?;
            if self.selection.focused_table_cell.is_some()
                || !self.document.text_models.contains_key(&block_id)
            {
                return Err("Inline AI requires an editable text block".to_owned());
            }
            let offset = self
                .caret_offset_for_block(block_id)
                .ok_or_else(|| "Inline AI requires a caret".to_owned())?;
            RuntimeAiTarget::InlineCaret(TextPosition::downstream(block_id, offset))
        };

        let target_block_versions = self.ai_target_block_versions(&target)?;
        let selection_fingerprint = ai_selection_fingerprint(&target, &target_block_versions);
        let preview_kind = match (presentation, &target) {
            (AiRequestPresentation::AssistantPanel, _) => AiPreviewKind::AssistantPanel,
            (AiRequestPresentation::Automatic, RuntimeAiTarget::InlineCaret(_)) => {
                AiPreviewKind::InlineCompletion
            }
            (AiRequestPresentation::Automatic, RuntimeAiTarget::TextSelection(_)) => {
                AiPreviewKind::SelectionRewrite
            }
        };
        let (task, selected_text, prefix, suffix) = self.ai_provider_context(&target)?;
        let request_id = self.next_ai_request_id;
        self.next_ai_request_id = self.next_ai_request_id.saturating_add(1);
        let cancellation = AiCancellationToken::default();
        let request = AiProviderRequest {
            request_id,
            task,
            instruction: instruction.clone(),
            selected_text,
            prefix,
            suffix,
        };
        self.ai_session = Some(RuntimeAiSession {
            request_id,
            target,
            target_block_versions,
            selection_fingerprint,
            preview_kind,
            instruction,
            preview: String::new(),
            status: AiSessionStatus::Streaming,
            cancellation: cancellation.clone(),
        });
        Ok(AiRequestDispatch {
            request,
            cancellation,
            redacted_fields: 0,
        })
    }

    pub(crate) fn apply_ai_stream_event(&mut self, event: AiStreamEvent) -> AiStreamApplyResult {
        let Some(session) = self.ai_session.as_ref() else {
            return AiStreamApplyResult::IgnoredRequest;
        };
        if session.request_id != event.request_id() {
            return AiStreamApplyResult::IgnoredRequest;
        }
        if !self.ai_session_is_current(session) {
            self.cancel_ai_request();
            return AiStreamApplyResult::DiscardedStale;
        }
        let session = self.ai_session.as_mut().expect("AI session exists");
        match event {
            AiStreamEvent::Delta { text, .. } => {
                if session.preview.len().saturating_add(text.len()) > MAX_AI_PREVIEW_BYTES {
                    session.cancellation.cancel();
                    session.status = AiSessionStatus::Failed(
                        "AI preview exceeded the editor safety limit".to_owned(),
                    );
                    return AiStreamApplyResult::RejectedTooLarge;
                }
                session.preview.push_str(&text);
            }
            AiStreamEvent::Done { .. } => {
                session.status = AiSessionStatus::Ready;
            }
            AiStreamEvent::Error { message, .. } => {
                session.status = AiSessionStatus::Failed(message);
            }
        }
        AiStreamApplyResult::Applied
    }

    pub fn ai_session_snapshot(&self) -> Option<AiSessionSnapshot> {
        let session = self.ai_session.as_ref()?;
        self.ai_session_is_current(session)
            .then(|| AiSessionSnapshot {
                request_id: session.request_id,
                target: session.target.clone(),
                selection_fingerprint: session.selection_fingerprint,
                instruction: session.instruction.clone(),
                preview: session.preview.clone(),
                status: session.status.clone(),
            })
    }

    pub(crate) fn cancel_ai_request(&mut self) -> bool {
        let Some(session) = self.ai_session.take() else {
            return false;
        };
        session.cancellation.cancel();
        true
    }

    pub(crate) fn reject_ai_preview(&mut self) -> bool {
        self.cancel_ai_request()
    }

    #[cfg(test)]
    pub(crate) fn accept_ai_preview(&mut self) -> Result<bool, String> {
        let mode = match self.ai_session.as_ref().map(|session| &session.target) {
            Some(RuntimeAiTarget::InlineCaret(_)) => AiApplyMode::InsertAfter,
            Some(RuntimeAiTarget::TextSelection(_)) => AiApplyMode::Replace,
            None => return Ok(false),
        };
        self.apply_ai_preview(mode, None)
    }

    pub fn apply_ai_preview(
        &mut self,
        mode: AiApplyMode,
        imported_markdown: Option<ImportedBlockDocument>,
    ) -> Result<bool, String> {
        let Some(session) = self.ai_session.as_ref() else {
            return Ok(false);
        };
        if !self.ai_session_is_current(session) {
            self.cancel_ai_request();
            return Err("AI preview is stale because the document or selection changed".to_owned());
        }
        if !matches!(session.status, AiSessionStatus::Ready) {
            return Ok(false);
        }
        if session.preview.is_empty() {
            self.cancel_ai_request();
            return Ok(false);
        }
        let session = self.ai_session.take().expect("validated AI session exists");
        session.cancellation.cancel();
        super::markdown_paste::trace_markdown(
            "ai.apply",
            format_args!(
                "mode={mode:?} kind={:?} target={:?} bytes={} preview=\"{}\"",
                session.preview_kind,
                session.target,
                session.preview.len(),
                super::markdown_paste::markdown_trace_preview(&session.preview)
            ),
        );
        let changed = match session.target {
            RuntimeAiTarget::InlineCaret(position) => {
                self.focus_block_at_offset(position.block_id, position.offset)?;
                if session.preview_kind == AiPreviewKind::AssistantPanel {
                    if let Some(document) = imported_markdown {
                        self.insert_imported_markdown_content_transaction(
                            document,
                            EditTransactionKind::AiApply,
                            cditor_core::edit::ChangeOrigin::Ai,
                        )?
                    } else {
                        self.replace_text_from_ai(
                            Some(position.offset..position.offset),
                            &session.preview,
                        )?
                    }
                } else {
                    self.replace_text_from_ai(
                        Some(position.offset..position.offset),
                        &session.preview,
                    )?
                }
            }
            RuntimeAiTarget::TextSelection(selection) => {
                let normalized = selection
                    .normalize(&self.document.index)
                    .map_err(|error| format!("{error:?}"))?;
                match mode {
                    AiApplyMode::InsertAfter => {
                        self.focus_block_at_offset(normalized.end.block_id, normalized.end.offset)?;
                        if let Some(document) = imported_markdown {
                            self.insert_imported_markdown_content_transaction(
                                document,
                                EditTransactionKind::AiApply,
                                cditor_core::edit::ChangeOrigin::Ai,
                            )?
                        } else {
                            self.replace_text_from_ai(
                                Some(normalized.end.offset..normalized.end.offset),
                                &session.preview,
                            )?
                        }
                    }
                    AiApplyMode::Replace
                        if normalized.start.block_id == normalized.end.block_id =>
                    {
                        self.set_document_text_selection(
                            normalized.start.block_id,
                            normalized.start.offset,
                            normalized.end.block_id,
                            normalized.end.offset,
                        )?;
                        if let Some(document) = imported_markdown {
                            self.insert_imported_markdown_content_transaction(
                                document,
                                EditTransactionKind::AiApply,
                                cditor_core::edit::ChangeOrigin::Ai,
                            )?
                        } else {
                            self.focus_block_at_offset(
                                normalized.start.block_id,
                                normalized.start.offset,
                            )?;
                            self.replace_text_from_ai(
                                Some(normalized.start.offset..normalized.end.offset),
                                &session.preview,
                            )?
                        }
                    }
                    AiApplyMode::Replace => {
                        self.selection.document_selection = Some(selection);
                        if let Some(document) = imported_markdown {
                            self.insert_imported_markdown_content_transaction(
                                document,
                                EditTransactionKind::AiApply,
                                cditor_core::edit::ChangeOrigin::Ai,
                            )?
                        } else {
                            self.apply_cross_block_ai_replacement(selection, &session.preview)?
                        }
                    }
                }
            }
        };
        if changed {
            let _ = self.refresh_focused_text_block_height();
        }
        Ok(changed)
    }

    pub(super) fn refresh_ai_session_validity(&mut self) -> bool {
        let stale = self
            .ai_session
            .as_ref()
            .is_some_and(|session| !self.ai_session_is_current(session));
        if stale {
            self.cancel_ai_request();
        }
        !stale
    }

    pub(super) fn ai_preview_for_block_range(
        &self,
        block_range: &Range<usize>,
    ) -> Option<AiPreviewSnapshot> {
        let session = self.ai_session.as_ref()?;
        if !self.ai_session_is_current(session) {
            return None;
        }
        let (block_id, anchor_offset, replacement_range) = match session.target {
            RuntimeAiTarget::InlineCaret(position) => (position.block_id, position.offset, None),
            RuntimeAiTarget::TextSelection(selection) => {
                let normalized = selection.normalize(&self.document.index).ok()?;
                let replacement_range = (normalized.start.block_id == normalized.end.block_id)
                    .then_some(normalized.start.offset..normalized.end.offset);
                (
                    normalized.start.block_id,
                    normalized.start.offset,
                    replacement_range,
                )
            }
        };
        let visible_index = self.document.visible_index.visible_index_of(block_id)?;
        if !block_range.contains(&visible_index) {
            return None;
        }
        let status = match &session.status {
            AiSessionStatus::Streaming => AiPreviewStatus::Streaming,
            AiSessionStatus::Ready => AiPreviewStatus::Ready,
            AiSessionStatus::Failed(message) => AiPreviewStatus::Failed(message.clone()),
        };
        Some(AiPreviewSnapshot {
            request_id: session.request_id,
            block_id,
            anchor_offset,
            replacement_range,
            selection_fingerprint: session.selection_fingerprint,
            text: session.preview.clone(),
            status,
            kind: session.preview_kind,
        })
    }

    fn ai_session_is_current(&self, session: &RuntimeAiSession) -> bool {
        if session
            .target_block_versions
            .iter()
            .any(|(block_id, version)| self.block_content_version(*block_id) != Some(*version))
        {
            return false;
        }
        match &session.target {
            RuntimeAiTarget::InlineCaret(position) => {
                let selection_is_clear = self.selection.document_selection.is_none()
                    && self.selection.selected_block_ids.is_empty();
                if session.preview_kind == AiPreviewKind::AssistantPanel {
                    // The assistant prompt temporarily owns the GUI focus. Do not
                    // discard its stream just because the editor caret is no longer
                    // the active platform input target; the original block version
                    // check above still protects against document edits.
                    selection_is_clear
                } else {
                    selection_is_clear
                        && self.focused_block_id() == Some(position.block_id)
                        && self.caret_offset_for_block(position.block_id) == Some(position.offset)
                }
            }
            RuntimeAiTarget::TextSelection(selection) => {
                self.selection.selected_block_ids.is_empty()
                    && self.selection.document_selection.as_ref() == Some(selection)
            }
        }
    }

    fn ai_target_block_versions(
        &self,
        target: &RuntimeAiTarget,
    ) -> Result<Vec<(BlockId, u64)>, String> {
        let block_ids = match target {
            RuntimeAiTarget::InlineCaret(position) => vec![position.block_id],
            RuntimeAiTarget::TextSelection(selection) => {
                let normalized = selection
                    .normalize(&self.document.index)
                    .map_err(|error| format!("{error:?}"))?;
                let start = self
                    .document
                    .index
                    .index_of(normalized.start.block_id)
                    .ok_or_else(|| "AI selection start block is missing".to_owned())?;
                let end = self
                    .document
                    .index
                    .index_of(normalized.end.block_id)
                    .ok_or_else(|| "AI selection end block is missing".to_owned())?;
                self.document.index.block_ids[start..=end].to_vec()
            }
        };
        block_ids
            .into_iter()
            .map(|block_id| {
                self.block_content_version(block_id)
                    .map(|version| (block_id, version))
                    .ok_or_else(|| format!("AI target block {block_id} is not loaded"))
            })
            .collect()
    }

    fn ai_provider_context(
        &self,
        target: &RuntimeAiTarget,
    ) -> Result<(AiTaskKind, String, String, String), String> {
        match target {
            RuntimeAiTarget::InlineCaret(position) => {
                let text = self
                    .document
                    .text_models
                    .get(&position.block_id)
                    .ok_or_else(|| "AI caret block is not loaded".to_owned())?
                    .text();
                let offset = safe_char_range(text, position.offset..position.offset).start;
                Ok((
                    AiTaskKind::InlineCompletion,
                    String::new(),
                    bounded_suffix(&text[..offset], MAX_AI_CONTEXT_BYTES),
                    bounded_prefix(&text[offset..], MAX_AI_CONTEXT_BYTES),
                ))
            }
            RuntimeAiTarget::TextSelection(selection) => {
                let normalized = selection
                    .normalize(&self.document.index)
                    .map_err(|error| format!("{error:?}"))?;
                let start_text = self
                    .document
                    .text_models
                    .get(&normalized.start.block_id)
                    .ok_or_else(|| "AI selection start block is not loaded".to_owned())?
                    .text();
                let end_text = self
                    .document
                    .text_models
                    .get(&normalized.end.block_id)
                    .ok_or_else(|| "AI selection end block is not loaded".to_owned())?
                    .text();
                let start =
                    safe_char_range(start_text, normalized.start.offset..normalized.start.offset)
                        .start;
                let end =
                    safe_char_range(end_text, normalized.end.offset..normalized.end.offset).start;
                Ok((
                    AiTaskKind::RewriteSelection,
                    self.selected_document_text().unwrap_or_default(),
                    bounded_suffix(&start_text[..start], MAX_AI_CONTEXT_BYTES),
                    bounded_prefix(&end_text[end..], MAX_AI_CONTEXT_BYTES),
                ))
            }
        }
    }

    fn apply_cross_block_ai_replacement(
        &mut self,
        selection: DocumentSelection,
        replacement: &str,
    ) -> Result<bool, String> {
        let normalized = selection
            .normalize(&self.document.index)
            .map_err(|error| format!("{error:?}"))?;
        let start_block_id = normalized.start.block_id;
        let start_index = self
            .document
            .index
            .index_of(start_block_id)
            .ok_or_else(|| "AI selection start block is missing".to_owned())?;
        let end_index = self
            .document
            .index
            .index_of(normalized.end.block_id)
            .ok_or_else(|| "AI selection end block is missing".to_owned())?;
        let delete_range = start_index + 1..end_index + 1;
        let records = self.index_records();
        super::transaction_apply_structure::ensure_complete_subtrees(
            &records,
            delete_range.clone(),
        )?;
        let before_current = self
            .document
            .payload_window
            .get(start_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {start_block_id}"))?;
        let end_payload = self
            .document
            .payload_window
            .get(normalized.end.block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {}", normalized.end.block_id))?;
        let (
            BlockPayload::RichText { spans: start_spans },
            BlockPayload::RichText { spans: end_spans },
        ) = (&before_current.payload, &end_payload.payload)
        else {
            return Ok(false);
        };
        let start_text = plain_text_from_spans(start_spans);
        let end_text = plain_text_from_spans(end_spans);
        let start = safe_char_range(
            &start_text,
            normalized.start.offset..normalized.start.offset,
        )
        .start;
        let end = safe_char_range(&end_text, normalized.end.offset..normalized.end.offset).start;
        let mut next_spans = slice_rich_text_spans(start_spans, 0..start);
        next_spans.push(InlineSpan::plain(replacement));
        next_spans.extend(slice_rich_text_spans(end_spans, end..end_text.len()));
        merge_inline_spans(&mut next_spans);
        let after_payload = BlockPayload::RichText { spans: next_spans };
        let deleted_records = records[delete_range.clone()].to_vec();
        let deleted_payloads = deleted_records
            .iter()
            .map(|record| {
                self.document
                    .payload_window
                    .get(record.id)
                    .cloned()
                    .ok_or_else(|| format!("missing AI selection payload {}", record.id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let caret = start.saturating_add(replacement.len());
        self.apply_local_structure_transaction(
            EditTransactionKind::AiApply,
            cditor_core::edit::ChangeOrigin::Ai,
            vec![
                EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                    block_id: start_block_id,
                    before_kind: before_current.kind.clone(),
                    before_payload: before_current.payload.clone(),
                    after_kind: before_current.kind.clone(),
                    after_payload: after_payload.clone(),
                }),
                EditOperation::DeleteBlockRange {
                    range: delete_range.clone(),
                },
            ],
            vec![
                EditOperation::InsertBlocks {
                    index: delete_range.start,
                    blocks: deleted_records,
                    payloads: deleted_payloads,
                },
                EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                    block_id: start_block_id,
                    before_kind: before_current.kind.clone(),
                    before_payload: after_payload,
                    after_kind: before_current.kind,
                    after_payload: before_current.payload,
                }),
            ],
            vec![
                TransactionPrecondition::StructureVersion(self.structure_version()),
                TransactionPrecondition::BlockContentVersion {
                    block_id: start_block_id,
                    version: before_current.content_version,
                },
            ],
            Some(selection),
            Some(DocumentSelection::caret(TextPosition {
                block_id: start_block_id,
                offset: caret,
                affinity: TextAffinity::Downstream,
            })),
            self.selected_block_ids_snapshot(),
            Vec::new(),
        )?;
        self.focus_block_at_offset(start_block_id, caret)?;
        Ok(true)
    }
}

// ── Agent bridge methods ──────────────────────────────────────────

impl DocumentRuntime {
    /// Get block summary for agent: kind, text, version, has_children.
    pub fn agent_block_summary(
        &self,
        block_id: BlockId,
    ) -> Option<(RichBlockKind, String, u64, bool)> {
        let payload = self.document.payload_window.get(block_id)?;
        let text = self
            .document
            .text_models
            .get(&block_id)
            .map(|tm| tm.text().to_string())
            .unwrap_or_default();
        let has_children = self.index_records().iter().any(|r| r.parent_id == Some(block_id));
        Some((
            payload.kind.clone(),
            text,
            payload.content_version,
            has_children,
        ))
    }

    /// List block children for agent.
    pub fn agent_children(
        &self,
        parent_id: BlockId,
        limit: usize,
        offset: usize,
    ) -> Vec<(BlockId, RichBlockKind, String)> {
        self.index_records()
            .iter()
            .filter(|r| r.parent_id == Some(parent_id))
            .skip(offset)
            .take(limit)
            .filter_map(|record| {
                let cid = record.id;
                let payload = self.document.payload_window.get(cid)?;
                let text = self
                    .document
                    .text_models
                    .get(&cid)
                    .map(|tm| tm.text().to_string())
                    .unwrap_or_default();
                Some((cid, payload.kind.clone(), text))
            })
            .collect()
    }

    /// Document statistics for agent.
    pub fn agent_document_stat(&self) -> (u64, Option<String>, usize) {
        let records = self.index_records();
        (
            self.document_id(),
            self.document_title().map(|s| s.to_string()),
            records.len(),
        )
    }
}
