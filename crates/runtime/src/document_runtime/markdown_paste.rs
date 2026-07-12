use super::*;

pub(super) fn markdown_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_MARKDOWN")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

pub(super) fn trace_markdown(event: &str, details: impl std::fmt::Display) {
    if markdown_trace_enabled() {
        eprintln!("[cditor][markdown][{event}] {details}");
    }
}

pub(super) fn markdown_trace_preview(text: &str) -> String {
    text.chars()
        .take(160)
        .collect::<String>()
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

impl DocumentRuntime {
    pub fn insert_markdown_paste(&mut self, markdown: &str) -> Result<bool, String> {
        let detected = looks_like_markdown_paste(markdown);
        trace_markdown(
            "paste.detect",
            format_args!(
                "detected={detected} bytes={} focus={:?} preview=\"{}\"",
                markdown.len(),
                self.focused_block_id(),
                markdown_trace_preview(markdown)
            ),
        );
        if !detected {
            return Ok(false);
        }
        self.insert_markdown_content(markdown)
    }

    pub(super) fn insert_markdown_content(&mut self, markdown: &str) -> Result<bool, String> {
        let first_block_id = self.next_available_block_id();
        let original_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));
        let mut deleted_records = Vec::new();
        let mut deleted_payloads = Vec::new();
        let mut before_current_override: Option<(BlockIndexRecord, BlockPayloadRecord)> = None;
        let mut current_block_id = self
            .focused_block_id()
            .ok_or_else(|| "missing focused block".to_owned())?;
        if self.has_cross_block_text_selection() {
            let normalized = self
                .document_selection
                .ok_or_else(|| "missing document selection".to_owned())?
                .normalize(&self.index)
                .map_err(|error| format!("{error:?}"))?;
            before_current_override = Some((
                self.index_record_for_block(normalized.start.block_id)?,
                self.payload_window
                    .get(normalized.start.block_id)
                    .cloned()
                    .ok_or_else(|| {
                        format!("missing payload for block {}", normalized.start.block_id)
                    })?,
            ));
        }
        if self.has_cross_block_text_selection()
            && let Some((block_id, records, payloads)) =
                self.collapse_cross_block_selection_for_paste()?
        {
            current_block_id = block_id;
            deleted_records = records;
            deleted_payloads = payloads;
        }
        let Some(current_index) = self.index.index_of(current_block_id) else {
            return Ok(false);
        };
        let current_kind = self.kind_at_index(current_index);
        if !current_kind.supports_rich_text_title() {
            trace_markdown(
                "parse.blocked",
                format_args!("block={current_block_id} kind={current_kind:?}"),
            );
            return Ok(false);
        }

        let (prefix, suffix, caret) = {
            let model = self
                .text_models
                .get(&current_block_id)
                .ok_or_else(|| format!("missing text model for block {current_block_id}"))?;
            let range = self
                .focused_text_selection_range()
                .map(|range| safe_char_range(model.text(), range))
                .unwrap_or_else(|| {
                    let caret = self
                        .editing
                        .as_ref()
                        .map(|editing| editing.caret_anchor.text_offset as usize)
                        .unwrap_or(model.len());
                    safe_char_range(model.text(), caret..caret)
                });
            (
                model.text()[..range.start].to_owned(),
                model.text()[range.end..].to_owned(),
                range.start,
            )
        };

        let options = MarkdownImportOptions {
            document_id: self.document_id,
            first_block_id,
        };
        let imported = import_markdown_block_incremental(markdown, options)
            .map(|block| ParsedMarkdownDocument {
                root_blocks: vec![block.id],
                blocks: vec![block],
            })
            .unwrap_or_else(|| parse_markdown_document(markdown, options));
        trace_markdown(
            "parse.result",
            format_args!(
                "block={current_block_id} input_bytes={} blocks={} roots={} kinds={:?}",
                markdown.len(),
                imported.blocks.len(),
                imported.root_blocks.len(),
                imported
                    .blocks
                    .iter()
                    .map(|block| &block.kind)
                    .collect::<Vec<_>>()
            ),
        );
        if imported.blocks.is_empty() {
            return Ok(false);
        }

        let (before_current_record, before_current_payload) = before_current_override.unwrap_or((
            self.index_record_for_block(current_block_id)?,
            self.payload_window
                .get(current_block_id)
                .cloned()
                .ok_or_else(|| format!("missing payload for block {current_block_id}"))?,
        ));
        let before_focus = original_focus;

        self.cancel_composition();
        self.document_selection = None;
        self.focused_text_selection = None;

        let contains_table = imported
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::Table));
        if contains_table {
            self.insert_markdown_table_paste(
                current_block_id,
                current_index,
                imported,
                prefix,
                suffix,
            )?;
        } else {
            self.insert_markdown_text_paste(
                current_block_id,
                current_index,
                imported,
                prefix,
                suffix,
            )?;
        }
        self.focused_text_selection = None;
        self.document_selection = None;
        let after_current_record = self.index_record_for_block(current_block_id)?;
        let after_current_payload = self
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {current_block_id}"))?;
        let inserted_records = self
            .index_records()
            .into_iter()
            .filter(|record| record.id != current_block_id && record.id >= first_block_id)
            .collect::<Vec<_>>();
        let inserted_payloads = inserted_records
            .iter()
            .filter_map(|record| self.payload_window.get(record.id).cloned())
            .collect::<Vec<_>>();
        let after_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id,
            before_current_record,
            before_current_payload,
            after_current_record,
            after_current_payload,
            inserted_records,
            inserted_payloads,
            deleted_records,
            deleted_payloads,
            before_focus,
            after_focus,
        });
        trace_markdown(
            "apply.done",
            format_args!(
                "current_block={current_block_id} total_blocks={} focus={:?}",
                self.index.total_count(),
                self.focused_block_id()
            ),
        );
        let _ = caret;
        Ok(true)
    }

    pub(super) fn collapse_cross_block_selection_for_paste(
        &mut self,
    ) -> Result<Option<(BlockId, Vec<BlockIndexRecord>, Vec<BlockPayloadRecord>)>, String> {
        let Some(selection) = self.document_selection else {
            return Ok(None);
        };
        let normalized = selection
            .normalize(&self.index)
            .map_err(|error| format!("{error:?}"))?;
        if normalized.start.block_id == normalized.end.block_id {
            return Ok(None);
        }
        let start_index = self
            .index
            .index_of(normalized.start.block_id)
            .ok_or_else(|| "selection start block missing".to_owned())?;
        let end_index = self
            .index
            .index_of(normalized.end.block_id)
            .ok_or_else(|| "selection end block missing".to_owned())?;
        let start_text = self
            .text_models
            .get(&normalized.start.block_id)
            .ok_or_else(|| "selection start text not loaded".to_owned())?
            .text()
            .to_owned();
        let end_text = self
            .text_models
            .get(&normalized.end.block_id)
            .ok_or_else(|| "selection end text not loaded".to_owned())?
            .text()
            .to_owned();
        let start_offset =
            previous_char_boundary(&start_text, normalized.start.offset.min(start_text.len()));
        let end_offset =
            previous_char_boundary(&end_text, normalized.end.offset.min(end_text.len()));
        let prefix = start_text[..start_offset].to_owned();
        let suffix = end_text[end_offset..].to_owned();
        let combined = format!("{prefix}{suffix}");

        let mut records = self.index_records();
        let deleted_records = records
            .drain(start_index + 1..=end_index)
            .collect::<Vec<_>>();
        let deleted_ids = deleted_records
            .iter()
            .map(|record| record.id)
            .collect::<HashSet<_>>();
        let deleted_payloads = deleted_records
            .iter()
            .filter_map(|record| self.payload_window.get(record.id).cloned())
            .collect::<Vec<_>>();
        for block_id in &deleted_ids {
            self.payload_window.payloads.remove(block_id);
            self.text_models.remove(block_id);
            self.table_runtimes.remove(block_id);
        }
        self.rebuild_structure_index(records)?;
        self.focus_block_at_offset(normalized.start.block_id, start_offset)?;
        self.replace_text_in_block_with_plain(normalized.start.block_id, combined)?;
        self.focus_block_at_offset(normalized.start.block_id, start_offset)?;
        Ok(Some((
            normalized.start.block_id,
            deleted_records,
            deleted_payloads,
        )))
    }

    fn insert_markdown_text_paste(
        &mut self,
        current_block_id: BlockId,
        current_index: usize,
        mut imported: ParsedMarkdownDocument,
        prefix: String,
        suffix: String,
    ) -> Result<(), String> {
        let imported_first_id = imported.blocks[0].id;
        let parent_id = self.index.parent_ids[current_index];
        let depth = self.index.depths[current_index];
        let current_content_version = self
            .payload_window
            .get(current_block_id)
            .map(|payload| payload.content_version.saturating_add(1))
            .unwrap_or(2);

        let mut remap = HashMap::new();
        remap.insert(imported_first_id, current_block_id);
        for block in imported.blocks.iter().skip(1) {
            remap.insert(block.id, block.id);
        }
        for block in &mut imported.blocks {
            if let Some(mapped) = remap.get(&block.id).copied() {
                block.id = mapped;
            }
            block.document_id = self.document_id;
            block.parent_id = block
                .parent_id
                .and_then(|id| remap.get(&id).copied())
                .or(parent_id);
            if block.parent_id == parent_id {
                block.depth = depth;
            } else if block.parent_id.is_some() {
                block.depth = block.depth.saturating_add(depth);
            }
            for child in &mut block.children {
                if let Some(mapped) = remap.get(child).copied() {
                    *child = mapped;
                }
            }
        }

        let mut first = imported.blocks.remove(0);
        first.id = current_block_id;
        first.parent_id = parent_id;
        first.depth = depth;
        first.content_version = current_content_version;
        first.payload = prepend_plain_text_to_payload(prefix, first.payload);

        let (focus_block_id, focus_offset) = if let Some(last) = imported.blocks.last_mut() {
            let offset = last.payload.plain_text().len();
            last.payload = append_plain_text_to_payload(last.payload.clone(), suffix);
            (last.id, offset)
        } else {
            let offset = first.payload.plain_text().len();
            first.payload = append_plain_text_to_payload(first.payload, suffix);
            (current_block_id, offset)
        };

        self.replace_existing_block_from_record(current_block_id, first)?;
        let insert_at = self.subtree_end(current_index);
        self.insert_imported_blocks_at(insert_at, imported.blocks)?;
        let _ = self.focus_block_at_offset(focus_block_id, focus_offset);
        Ok(())
    }

    fn insert_markdown_table_paste(
        &mut self,
        current_block_id: BlockId,
        current_index: usize,
        imported: ParsedMarkdownDocument,
        prefix: String,
        suffix: String,
    ) -> Result<(), String> {
        let parent_id = self.index.parent_ids[current_index];
        let depth = self.index.depths[current_index];
        let prefix_empty = prefix.is_empty();
        let mut insert_blocks = imported.blocks;
        if insert_blocks.is_empty() {
            return Ok(());
        }

        let insert_at = self.subtree_end(current_index);
        if prefix_empty {
            let mut first = insert_blocks.remove(0);
            first.id = current_block_id;
            first.document_id = self.document_id;
            first.parent_id = parent_id;
            first.depth = depth;
            first.content_version = self
                .payload_window
                .get(current_block_id)
                .map(|payload| payload.content_version.saturating_add(1))
                .unwrap_or(2);
            self.replace_existing_block_from_record(current_block_id, first)?;
        } else {
            self.replace_text_in_block_with_plain(current_block_id, prefix)?;
        }

        for block in &mut insert_blocks {
            block.document_id = self.document_id;
            block.parent_id = parent_id;
            block.depth = depth;
            block.children.clear();
        }
        if !suffix.is_empty()
            || insert_blocks
                .last()
                .is_some_and(|block| !block.kind.supports_rich_text_title())
        {
            let trailing_id = self.next_available_block_id().max(
                insert_blocks
                    .iter()
                    .map(|block| block.id)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1),
            );
            let mut trailing = RichBlockRecord::paragraph(trailing_id, suffix.clone());
            trailing.document_id = self.document_id;
            trailing.parent_id = parent_id;
            trailing.depth = depth;
            insert_blocks.push(trailing);
        }
        let focus_block_id = insert_blocks
            .iter()
            .rev()
            .find(|block| block.kind.supports_rich_text_title())
            .map(|block| block.id)
            .unwrap_or(current_block_id);
        let focus_offset = if focus_block_id
            == insert_blocks
                .last()
                .map(|block| block.id)
                .unwrap_or(current_block_id)
        {
            suffix.len()
        } else {
            0
        };
        self.insert_imported_blocks_at(insert_at, insert_blocks)?;
        let _ = self.focus_block_at_offset(focus_block_id, focus_offset);
        Ok(())
    }

    pub(super) fn try_apply_space_block_markdown_shortcut(
        &mut self,
        block_id: BlockId,
        caret: usize,
    ) -> Result<bool, String> {
        let text = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        let caret = previous_char_boundary(&text, caret.min(text.len()));
        if caret != text.len() {
            return Ok(false);
        }
        let current_kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or(RichBlockKind::Paragraph);
        if let Some((kind, marker_len)) = block_kind_shortcut_with_marker_len(&(text.clone() + " "))
            && marker_len == text.len() + 1
            && should_apply_space_block_markdown_shortcut(&current_kind, &kind)
        {
            self.cancel_composition();
            self.push_undo_snapshot(block_id)?;
            let payload = if matches!(kind, RichBlockKind::Divider | RichBlockKind::Separator) {
                BlockPayload::Empty
            } else {
                BlockPayload::RichText { spans: Vec::new() }
            };
            self.replace_block_kind_and_payload(block_id, kind, payload)?;
            return Ok(true);
        }
        // Inside a Quote block, detect callout markers like [!TIP] and upgrade.
        if matches!(current_kind, RichBlockKind::Quote) {
            if let Some(variant) = parse_callout_marker(&text) {
                self.cancel_composition();
                self.push_undo_snapshot(block_id)?;
                self.replace_block_kind_and_payload(
                    block_id,
                    RichBlockKind::Callout { variant },
                    BlockPayload::RichText { spans: Vec::new() },
                )?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn apply_inline_markdown_shortcut(
        &mut self,
        block_id: BlockId,
    ) -> Result<bool, String> {
        let Some(kind) = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
        else {
            return Ok(false);
        };
        if !matches!(
            kind,
            RichBlockKind::Paragraph
                | RichBlockKind::Heading { .. }
                | RichBlockKind::Quote
                | RichBlockKind::Callout { .. }
                | RichBlockKind::BulletedList
                | RichBlockKind::NumberedList
                | RichBlockKind::Todo { .. }
        ) {
            return Ok(false);
        }

        // Get current text and caret position
        let text = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();

        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or(text.len());

        // Try incremental detection first
        if let Some(detection) = cditor_core::rich_text::detect_delimiter_at_caret(&text, caret) {
            return self.apply_incremental_inline_markdown(block_id, &text, detection);
        }

        // Fallback to full parse for complex cases (links, code blocks, etc.)
        let Some(spans) = markdown_inline_shortcut_spans(&text) else {
            return Ok(false);
        };
        self.replace_block_kind_and_spans(block_id, kind, spans)?;
        Ok(true)
    }

    fn apply_incremental_inline_markdown(
        &mut self,
        block_id: BlockId,
        text: &str,
        detection: cditor_core::rich_text::DelimiterPairDetection,
    ) -> Result<bool, String> {
        let delim_len = detection.delimiter.len();

        // Calculate ranges in the original text
        let opening_delim_start = detection.source_range.start - delim_len;
        let closing_delim_end = detection.source_range.end + delim_len;
        let full_range_with_delims = opening_delim_start..closing_delim_end;

        // Extract the content between delimiters (without delims)
        let content_text = &text[detection.source_range.clone()];

        // Get existing spans to check for marks in the affected range
        let existing_spans = self
            .payload_window
            .get(block_id)
            .and_then(|p| match &p.payload {
                BlockPayload::RichText { spans } => Some(spans.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec![InlineSpan::plain(text.to_string())]);

        // Find existing marks in the content range
        let mut existing_marks_in_range = Vec::new();
        let mut cursor = 0;
        for span in &existing_spans {
            let span_start = cursor;
            let span_end = cursor + span.text.len();

            // Check if this span overlaps with the content range (not the full range with delims)
            if span_end > detection.source_range.start && span_start < detection.source_range.end {
                for mark in &span.marks {
                    if !existing_marks_in_range.contains(mark) {
                        existing_marks_in_range.push(mark.clone());
                    }
                }
            }

            cursor = span_end;
        }

        // Merge new mark with existing marks
        let mut combined_marks = existing_marks_in_range;
        if !combined_marks.contains(&detection.mark) {
            combined_marks.push(detection.mark.clone());
        }

        // Create new span with merged marks
        let new_span = InlineSpan {
            text: content_text.to_string(),
            marks: combined_marks,
        };

        // Update text model: replace "**content**" with "content"
        let model = self
            .text_models
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;

        let inserted = model
            .replace_range(full_range_with_delims.clone(), content_text)
            .map_err(|e| format!("{:?}", e))?;

        // Use the existing span splice logic
        let updated_spans = replace_rich_text_spans_with_spans(
            &existing_spans,
            full_range_with_delims,
            &[new_span],
        );

        // Update editing session
        if let Some(editing) = self.editing.as_mut() {
            editing.content_version += 1;
            editing.set_collapsed_selection(inserted.end);
        }

        // Update payload with new spans
        if let Some(payload) = self.payload_window.payloads.get_mut(&block_id) {
            payload.content_version = self
                .editing
                .as_ref()
                .map(|e| e.content_version)
                .unwrap_or(payload.content_version + 1);
            payload.payload = BlockPayload::RichText {
                spans: updated_spans,
            };
        }

        Ok(true)
    }

    fn insert_imported_blocks_at(
        &mut self,
        insert_at: usize,
        blocks: Vec<RichBlockRecord>,
    ) -> Result<(), String> {
        if blocks.is_empty() {
            return Ok(());
        }
        let mut records = self.index_records();
        let insert_at = insert_at.min(records.len());
        let mut index_records = Vec::with_capacity(blocks.len());
        let mut payload_records = Vec::with_capacity(blocks.len());
        for block in blocks {
            let mut payload = normalize_payload_record_for_kind(block.to_payload_record());
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.payload_window.insert(payload.clone());
            index_records.push(block.to_index_record());
            payload_records.push(payload);
        }
        records.splice(insert_at..insert_at, index_records);
        self.rebuild_structure_index(records)?;
        for mut payload in payload_records {
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.payload_window.insert(payload);
        }
        Ok(())
    }
}

fn should_apply_space_block_markdown_shortcut(
    current_kind: &RichBlockKind,
    shortcut_kind: &RichBlockKind,
) -> bool {
    matches!(current_kind, RichBlockKind::Paragraph)
        || (matches!(current_kind, RichBlockKind::BulletedList)
            && matches!(shortcut_kind, RichBlockKind::Todo { .. }))
        || (matches!(current_kind, RichBlockKind::Quote)
            && matches!(shortcut_kind, RichBlockKind::Callout { .. }))
}
