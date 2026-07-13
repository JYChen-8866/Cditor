use super::*;

impl DocumentRuntime {
    pub fn paste_clipboard_selection(
        &mut self,
        selection: &ClipboardSelection,
    ) -> Result<bool, String> {
        match selection {
            ClipboardSelection::Inline { spans } => self.paste_inline_clipboard_spans(spans),
            ClipboardSelection::TextFragments { fragments } => {
                if self.focused_table_cell.is_some() {
                    let mut spans = Vec::new();
                    for (index, fragment) in fragments.iter().enumerate() {
                        if index > 0 {
                            spans.push(InlineSpan::plain("\n"));
                        }
                        spans.extend(fragment.spans.clone());
                    }
                    self.paste_inline_clipboard_spans(&spans)
                } else {
                    self.paste_rich_text_fragments(fragments)
                }
            }
            ClipboardSelection::Blocks { blocks } => self.paste_clipboard_blocks(blocks),
            ClipboardSelection::Table { table } => {
                if table.row_count() == 0 || table.column_count() == 0 {
                    return Ok(false);
                }
                let snapshot = TableClipboardSnapshot {
                    range: TableRange::normalized(
                        0,
                        0,
                        table.row_count() - 1,
                        table.column_count() - 1,
                    ),
                    table: table.clone(),
                    plain_text: table.plain_text(),
                    markdown: table.plain_text(),
                };
                self.paste_table_clipboard_at_focused_cell(&snapshot)
            }
        }
    }

    fn paste_inline_clipboard_spans(&mut self, spans: &[InlineSpan]) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return self.replace_focused_range_with_rich_text_spans(spans);
        };
        let inserted_len = spans.iter().map(|span| span.text.len()).sum::<usize>();
        let range = focused.selected_range();
        let changed = {
            let runtime = self
                .table_runtime_mut(focused.block_id)
                .ok_or_else(|| format!("missing table runtime for block {}", focused.block_id))?;
            runtime.replace_cell_spans(focused.row, focused.col, range.clone(), spans)?
        };
        if changed {
            let caret = range.start.saturating_add(inserted_len);
            self.focused_table_cell = Some(FocusedTableCell::collapsed(
                focused.block_id,
                focused.row,
                focused.col,
                caret,
            ));
            self.commit_table_runtime_payload(focused.block_id)?;
        }
        Ok(changed)
    }

    fn paste_rich_text_fragments(
        &mut self,
        fragments: &[ClipboardBlockFragment],
    ) -> Result<bool, String> {
        if fragments.is_empty() {
            return Ok(false);
        }
        if fragments.len() == 1 {
            return self.replace_focused_range_with_rich_text_spans(&fragments[0].spans);
        }

        let original_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));
        let mut deleted_records = Vec::new();
        let mut deleted_payloads = Vec::new();
        let mut before_current_override = None;
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
                    .ok_or_else(|| "selection start payload is not loaded".to_owned())?,
            ));
            if let Some((block_id, records, payloads)) =
                self.collapse_cross_block_selection_for_paste()?
            {
                current_block_id = block_id;
                deleted_records = records;
                deleted_payloads = payloads;
            }
        }

        let current_index = self
            .index
            .index_of(current_block_id)
            .ok_or_else(|| "focused block is missing from index".to_owned())?;
        let before_current_record = before_current_override
            .as_ref()
            .map(|pair| pair.0.clone())
            .unwrap_or(self.index_record_for_block(current_block_id)?);
        let before_current_payload = before_current_override
            .map(|pair| pair.1)
            .or_else(|| self.payload_window.get(current_block_id).cloned())
            .ok_or_else(|| "focused payload is not loaded".to_owned())?;
        let current_payload = self
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| "focused payload is not loaded".to_owned())?;
        let BlockPayload::RichText {
            spans: current_spans,
        } = current_payload.payload
        else {
            return Ok(false);
        };
        let current_text = plain_text_from_spans(&current_spans);
        let range = self
            .focused_text_selection_range()
            .map(|range| safe_char_range(&current_text, range))
            .unwrap_or_else(|| {
                let caret = self
                    .caret_offset_for_block(current_block_id)
                    .unwrap_or(current_text.len());
                safe_char_range(&current_text, caret..caret)
            });
        let prefix = slice_rich_text_spans(&current_spans, 0..range.start);
        let suffix = slice_rich_text_spans(&current_spans, range.end..current_text.len());
        let parent_id = self.index.parent_ids[current_index];
        let depth = self.index.depths[current_index];

        let mut first_spans = prefix;
        first_spans.extend(fragments[0].spans.clone());
        let replaces_entire_target = range.start == 0 && range.end == current_text.len();
        let current_kind = if replaces_entire_target
            && fragments[0].starts_at_block_start
            && fragments[0].ends_at_block_end
        {
            fragments[0].kind.clone()
        } else {
            current_payload.kind
        };
        self.replace_block_kind_and_payload(
            current_block_id,
            current_kind,
            BlockPayload::RichText {
                spans: coalesce_clipboard_spans(first_spans),
            },
        )?;

        let mut next_id = self.next_available_block_id();
        let mut id_map = HashMap::with_capacity(fragments.len());
        id_map.insert(fragments[0].source_id, current_block_id);
        for fragment in fragments.iter().skip(1) {
            id_map.insert(fragment.source_id, next_id);
            next_id = next_id.saturating_add(1);
        }
        let insert_at = self.subtree_end(current_index);
        let mut inserted_records = Vec::new();
        let mut inserted_payloads = Vec::new();
        let mut focus_block_id = current_block_id;
        let mut focus_offset = fragments[0]
            .spans
            .iter()
            .map(|span| span.text.len())
            .sum::<usize>();
        for (fragment_index, fragment) in fragments.iter().enumerate().skip(1) {
            let block_id = id_map[&fragment.source_id];
            let mut spans = fragment.spans.clone();
            focus_offset = spans.iter().map(|span| span.text.len()).sum();
            if fragment_index + 1 == fragments.len() {
                spans.extend(suffix.clone());
            }
            let payload = BlockPayloadRecord {
                block_id,
                content_version: 1,
                kind: fragment.kind.clone(),
                payload: BlockPayload::RichText {
                    spans: coalesce_clipboard_spans(spans),
                },
            };
            let fragment_parent_id = fragment
                .parent_source_id
                .and_then(|source_id| id_map.get(&source_id).copied())
                .or(parent_id);
            let fragment_depth =
                depth.saturating_add(fragment.depth.saturating_sub(fragments[0].depth));
            let record = BlockIndexRecord::new(
                block_id,
                fragment_parent_id,
                fragment_depth,
                kind_tag_for_rich_block_kind(&fragment.kind),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
                block_id,
                estimate_payload_height(&payload, insert_at + fragment_index - 1),
            ));
            inserted_records.push(record);
            inserted_payloads.push(payload);
            focus_block_id = block_id;
        }
        self.insert_runtime_blocks_batch(insert_at, &inserted_records, inserted_payloads)?;
        self.focus_block_at_offset(focus_block_id, focus_offset)?;
        let after_current_record = self.index_record_for_block(current_block_id)?;
        let after_current_payload = self
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| "focused payload disappeared after paste".to_owned())?;
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id,
            before_current_record,
            before_current_payload,
            after_current_record,
            after_current_payload,
            inserted_records,
            inserted_payloads: Vec::new(),
            deleted_records,
            deleted_payloads,
            before_focus: original_focus,
            after_focus: Some((focus_block_id, focus_offset)),
        });
        Ok(true)
    }

    fn paste_clipboard_blocks(&mut self, blocks: &[ClipboardBlock]) -> Result<bool, String> {
        if blocks.is_empty() {
            return Ok(false);
        }
        let anchor_id = self
            .selected_block_ids
            .iter()
            .filter_map(|block_id| {
                self.index
                    .index_of(*block_id)
                    .map(|index| (index, *block_id))
            })
            .max_by_key(|pair| pair.0)
            .map(|pair| pair.1)
            .or_else(|| self.focused_block_id())
            .or_else(|| self.index.block_ids.last().copied())
            .ok_or_else(|| "document has no paste anchor".to_owned())?;
        let anchor_index = self
            .index
            .index_of(anchor_id)
            .ok_or_else(|| "paste anchor is missing".to_owned())?;
        let insert_at = self.subtree_end(anchor_index);
        let target_parent = self.index.parent_ids[anchor_index];
        let target_depth = self.index.depths[anchor_index];
        let before_current_record = self.index_record_for_block(anchor_id)?;
        let before_current_payload = self
            .payload_window
            .get(anchor_id)
            .cloned()
            .ok_or_else(|| "paste anchor payload is not loaded".to_owned())?;
        let before_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));

        let mut next_id = self.next_available_block_id();
        let mut id_map = HashMap::new();
        for block in blocks {
            id_map.insert(block.source_id, next_id);
            next_id = next_id.saturating_add(1);
        }
        let root_depth = blocks
            .iter()
            .filter(|block| block.parent_source_id.is_none())
            .map(|block| block.depth)
            .min()
            .unwrap_or(blocks[0].depth);
        let mut inserted_records = Vec::with_capacity(blocks.len());
        let mut inserted_payloads = Vec::with_capacity(blocks.len());
        for (offset, block) in blocks.iter().enumerate() {
            let block_id = id_map[&block.source_id];
            let parent_id = block
                .parent_source_id
                .and_then(|source_id| id_map.get(&source_id).copied())
                .or(target_parent);
            let depth = target_depth.saturating_add(block.depth.saturating_sub(root_depth));
            let payload = BlockPayloadRecord {
                block_id,
                content_version: 1,
                kind: block.kind.clone(),
                payload: block.payload.clone(),
            };
            let record = BlockIndexRecord::new(
                block_id,
                parent_id,
                depth,
                kind_tag_for_rich_block_kind(&block.kind),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
                block_id,
                estimate_payload_height(&payload, insert_at + offset),
            ));
            inserted_records.push(record);
            inserted_payloads.push(payload);
        }
        let first_id = inserted_records[0].id;
        self.insert_runtime_blocks_batch(insert_at, &inserted_records, inserted_payloads)?;
        self.selected_block_ids.clear();
        if let Some(text_len) = self
            .text_models
            .get(&first_id)
            .map(PieceTableTextModel::len)
        {
            self.focus_block_at_offset(first_id, text_len)?;
        } else {
            self.focus_block(first_id);
        }
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id: anchor_id,
            before_current_record,
            before_current_payload: before_current_payload.clone(),
            after_current_record: before_current_record,
            after_current_payload: before_current_payload,
            inserted_records,
            inserted_payloads: Vec::new(),
            deleted_records: Vec::new(),
            deleted_payloads: Vec::new(),
            before_focus,
            after_focus: Some((first_id, self.caret_offset_for_block(first_id).unwrap_or(0))),
        });
        Ok(true)
    }
}

fn coalesce_clipboard_spans(spans: Vec<InlineSpan>) -> Vec<InlineSpan> {
    let mut merged: Vec<InlineSpan> = Vec::with_capacity(spans.len());
    for span in spans.into_iter().filter(|span| !span.text.is_empty()) {
        if let Some(previous) = merged.last_mut()
            && previous.marks == span.marks
        {
            previous.text.push_str(&span.text);
        } else {
            merged.push(span);
        }
    }
    merged
}
