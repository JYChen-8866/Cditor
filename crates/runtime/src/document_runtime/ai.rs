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
    pub fn focused_empty_text_block_for_ai(&self) -> Option<(BlockId, usize)> {
        if self.document_selection.is_some()
            || self.has_selected_blocks()
            || self.focused_table_cell.is_some()
            || self.active_composition().is_some()
        {
            return None;
        }
        let block_id = self.focused_block_id()?;
        let payload = self.payload_window.get(block_id)?;
        if !empty_text_kind_supports_ai(&payload.kind) {
            return None;
        }
        let text = self.text_models.get(&block_id)?.text();
        text.is_empty()
            .then(|| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)))
    }

    pub fn begin_ai_request(
        &mut self,
        instruction: impl Into<String>,
    ) -> Result<AiRequestDispatch, String> {
        self.begin_ai_request_with_presentation(instruction, AiRequestPresentation::Automatic)
    }

    pub fn begin_ai_request_with_presentation(
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
            .document_selection
            .filter(|selection| !selection.is_caret())
        {
            RuntimeAiTarget::TextSelection(selection)
        } else {
            let block_id = self
                .focused_block_id()
                .ok_or_else(|| "Inline AI requires a focused text block".to_owned())?;
            if self.focused_table_cell.is_some() || !self.text_models.contains_key(&block_id) {
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
        })
    }

    pub fn apply_ai_stream_event(&mut self, event: AiStreamEvent) -> AiStreamApplyResult {
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

    pub fn cancel_ai_request(&mut self) -> bool {
        let Some(session) = self.ai_session.take() else {
            return false;
        };
        session.cancellation.cancel();
        true
    }

    pub fn reject_ai_preview(&mut self) -> bool {
        self.cancel_ai_request()
    }

    pub fn accept_ai_preview(&mut self) -> Result<bool, String> {
        let mode = match self.ai_session.as_ref().map(|session| &session.target) {
            Some(RuntimeAiTarget::InlineCaret(_)) => AiApplyMode::InsertAfter,
            Some(RuntimeAiTarget::TextSelection(_)) => AiApplyMode::Replace,
            None => return Ok(false),
        };
        self.apply_ai_preview(mode)
    }

    pub fn apply_ai_preview(&mut self, mode: AiApplyMode) -> Result<bool, String> {
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
                    if self.insert_markdown_content(&session.preview)? {
                        true
                    } else {
                        self.replace_text_in_focused_range(
                            Some(position.offset..position.offset),
                            &session.preview,
                        )?
                    }
                } else {
                    self.replace_text_in_focused_range(
                        Some(position.offset..position.offset),
                        &session.preview,
                    )?
                }
            }
            RuntimeAiTarget::TextSelection(selection) => {
                let normalized = selection
                    .normalize(&self.index)
                    .map_err(|error| format!("{error:?}"))?;
                match mode {
                    AiApplyMode::InsertAfter => {
                        self.focus_block_at_offset(normalized.end.block_id, normalized.end.offset)?;
                        if self.insert_markdown_content(&session.preview)? {
                            true
                        } else {
                            self.replace_text_in_focused_range(
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
                        if self.insert_markdown_content(&session.preview)? {
                            true
                        } else {
                            self.focus_block_at_offset(
                                normalized.start.block_id,
                                normalized.start.offset,
                            )?;
                            self.replace_text_in_focused_range(
                                Some(normalized.start.offset..normalized.end.offset),
                                &session.preview,
                            )?
                        }
                    }
                    AiApplyMode::Replace => {
                        self.document_selection = Some(selection);
                        if self.insert_markdown_content(&session.preview)? {
                            true
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
                let normalized = selection.normalize(&self.index).ok()?;
                let replacement_range = (normalized.start.block_id == normalized.end.block_id)
                    .then_some(normalized.start.offset..normalized.end.offset);
                (
                    normalized.start.block_id,
                    normalized.start.offset,
                    replacement_range,
                )
            }
        };
        let visible_index = self.visible_index.visible_index_of(block_id)?;
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
                let selection_is_clear =
                    self.document_selection.is_none() && self.selected_block_ids.is_empty();
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
                self.selected_block_ids.is_empty()
                    && self.document_selection.as_ref() == Some(selection)
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
                    .normalize(&self.index)
                    .map_err(|error| format!("{error:?}"))?;
                let start = self
                    .index
                    .index_of(normalized.start.block_id)
                    .ok_or_else(|| "AI selection start block is missing".to_owned())?;
                let end = self
                    .index
                    .index_of(normalized.end.block_id)
                    .ok_or_else(|| "AI selection end block is missing".to_owned())?;
                self.index.block_ids[start..=end].to_vec()
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
                    .normalize(&self.index)
                    .map_err(|error| format!("{error:?}"))?;
                let start_text = self
                    .text_models
                    .get(&normalized.start.block_id)
                    .ok_or_else(|| "AI selection start block is not loaded".to_owned())?
                    .text();
                let end_text = self
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
        self.document_selection = Some(selection);
        let normalized = selection
            .normalize(&self.index)
            .map_err(|error| format!("{error:?}"))?;
        let start_block_id = normalized.start.block_id;
        let before_current_record = self.index_record_for_block(start_block_id)?;
        let before_current_payload = self
            .payload_window
            .get(start_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {start_block_id}"))?;
        let before_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));
        let Some((_block_id, deleted_records, deleted_payloads)) =
            self.collapse_cross_block_selection_for_paste()?
        else {
            return Ok(false);
        };
        let collapsed = self
            .text_models
            .get(&start_block_id)
            .ok_or_else(|| format!("missing text model for block {start_block_id}"))?
            .text()
            .to_owned();
        let offset = safe_char_range(
            &collapsed,
            normalized.start.offset.min(collapsed.len())
                ..normalized.start.offset.min(collapsed.len()),
        )
        .start;
        let mut next = String::with_capacity(collapsed.len().saturating_add(replacement.len()));
        next.push_str(&collapsed[..offset]);
        next.push_str(replacement);
        next.push_str(&collapsed[offset..]);
        self.replace_text_in_block_with_plain(start_block_id, next)?;
        self.focus_block_at_offset(start_block_id, offset + replacement.len())?;
        self.document_selection = None;
        self.focused_text_selection = None;
        let after_current_record = self.index_record_for_block(start_block_id)?;
        let after_current_payload = self
            .payload_window
            .get(start_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {start_block_id}"))?;
        let after_focus = Some((start_block_id, offset + replacement.len()));
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id: start_block_id,
            before_current_record,
            before_current_payload,
            after_current_record,
            after_current_payload,
            inserted_records: Vec::new(),
            inserted_payloads: Vec::new(),
            deleted_records,
            deleted_payloads,
            before_focus,
            after_focus,
        });
        self.queue_ai_apply_transaction(start_block_id);
        Ok(true)
    }

    fn queue_ai_apply_transaction(&mut self, block_id: BlockId) {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        self.pending_structure_transactions
            .push(EditTransaction::new(
                transaction_id,
                EditTransactionKind::AiApply,
                transaction_id,
                vec![EditOperation::DeleteBlockRange { range: 0..0 }],
                vec![EditOperation::InsertBlock {
                    index: self.index.index_of(block_id).unwrap_or(0),
                    block: self
                        .index_record_for_block(block_id)
                        .unwrap_or_else(|_| BlockIndexRecord::new(block_id, None, 0, 0, 0)),
                }],
            ));
    }
}

fn bounded_prefix(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn bounded_suffix(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut start = value.len() - max_bytes;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    value[start..].to_owned()
}

fn ai_selection_fingerprint(target: &RuntimeAiTarget, versions: &[(BlockId, u64)]) -> u64 {
    let mut value = 0xcbf29ce484222325u64;
    let mut mix = |part: u64| {
        value ^= part;
        value = value.wrapping_mul(0x100000001b3);
    };
    match target {
        RuntimeAiTarget::InlineCaret(position) => {
            mix(1);
            mix(position.block_id);
            mix(position.offset as u64);
        }
        RuntimeAiTarget::TextSelection(selection) => {
            mix(2);
            mix(selection.anchor.block_id);
            mix(selection.anchor.offset as u64);
            mix(selection.focus.block_id);
            mix(selection.focus.offset as u64);
        }
    }
    for (block_id, version) in versions {
        mix(*block_id);
        mix(*version);
    }
    value
}
