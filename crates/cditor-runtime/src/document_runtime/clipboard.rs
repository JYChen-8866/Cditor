use super::*;

impl DocumentRuntime {
    pub(crate) fn paste_clipboard_selection(
        &mut self,
        selection: &ClipboardSelection,
    ) -> Result<bool, String> {
        if let Some(
            surface_id @ (cditor_core::ids::SurfaceId::ImageCaption { .. }
            | cditor_core::ids::SurfaceId::CollectionTitle { .. }),
        ) = self.focused_text_surface_id()
        {
            let mut spans = auxiliary_clipboard_spans(selection);
            if matches!(
                surface_id,
                cditor_core::ids::SurfaceId::CollectionTitle { .. }
            ) {
                for span in &mut spans {
                    span.text = span.text.replace(['\r', '\n'], " ");
                }
            }
            return self.paste_inline_clipboard_spans(&spans);
        }
        match selection {
            ClipboardSelection::Inline { spans } => self.paste_inline_clipboard_spans(spans),
            ClipboardSelection::TextFragments { fragments } => {
                if self.selection.focused_table_cell.is_some() {
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
        if let Some(
            surface_id @ (cditor_core::ids::SurfaceId::ImageCaption { .. }
            | cditor_core::ids::SurfaceId::CollectionTitle { .. }),
        ) = self.focused_text_surface_id()
        {
            let Some(edit) = self.resolve_focused_text_edit(None) else {
                return Ok(false);
            };
            let snapshot = self
                .text_surface_base_snapshot(surface_id)
                .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
            let inserted_len = spans.iter().map(|span| span.text.len()).sum::<usize>();
            let next =
                replace_rich_text_spans_with_spans(&snapshot.spans, edit.range.clone(), spans);
            let next_offset = edit.range.start.saturating_add(inserted_len);
            self.cancel_composition();
            let changed = self.apply_text_surface_paste_transaction(
                surface_id,
                snapshot.spans,
                next,
                next_offset,
            )?;
            if changed && let Some(editing) = self.editing.session.as_mut() {
                editing.set_collapsed_selection(next_offset);
            }
            return Ok(changed);
        }
        let Some(focused) = self.selection.focused_table_cell else {
            return self.replace_focused_range_with_rich_text_spans(spans);
        };
        let inserted_len = spans.iter().map(|span| span.text.len()).sum::<usize>();
        let range = focused.selected_range();
        let surface_id = SurfaceId::TableCell {
            block_id: focused.block_id,
            row: focused.row,
            column: focused.col,
        };
        let snapshot = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing table text surface {surface_id:?}"))?;
        let next = replace_rich_text_spans_with_spans(&snapshot.spans, range.clone(), spans);
        let caret = range.start.saturating_add(inserted_len);
        self.cancel_composition();
        let changed =
            self.apply_text_surface_paste_transaction(surface_id, snapshot.spans, next, caret)?;
        if changed {
            self.selection.focused_table_cell = Some(FocusedTableCell::collapsed(
                focused.block_id,
                focused.row,
                focused.col,
                caret,
            ));
            if let Some(editing) = self.editing.session.as_mut() {
                editing.set_input_target(InputTarget::TableCell {
                    block_id: focused.block_id,
                    row: focused.row,
                    col: focused.col,
                });
                editing.set_collapsed_selection(caret);
            }
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
        self.paste_rich_text_fragments_transaction(fragments)
    }

    fn paste_rich_text_fragments_transaction(
        &mut self,
        fragments: &[ClipboardBlockFragment],
    ) -> Result<bool, String> {
        let cross_plan = self
            .selection
            .document_selection
            .filter(|selection| selection.anchor.block_id != selection.focus.block_id)
            .map(|selection| self.plan_cross_block_replacement(selection))
            .transpose()?;
        let cross_selection = cross_plan.as_ref().map(|plan| plan.selection);
        let current_block_id = cross_selection
            .map(|selection| selection.start.block_id)
            .or_else(|| self.focused_block_id())
            .ok_or_else(|| "missing focused block".to_owned())?;
        let current_index = self
            .document
            .index
            .index_of(current_block_id)
            .ok_or_else(|| "focused block is missing from index".to_owned())?;
        let before_current = self
            .document
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| "focused payload is not loaded".to_owned())?;
        let BlockPayload::RichText {
            spans: current_spans,
        } = &before_current.payload
        else {
            return Ok(false);
        };
        let current_text = plain_text_from_spans(current_spans);
        let (prefix, suffix) = if let Some(selection) = cross_selection {
            let end_payload = self
                .document
                .payload_window
                .get(selection.end.block_id)
                .ok_or_else(|| "selection end payload is not loaded".to_owned())?;
            let BlockPayload::RichText { spans: end_spans } = &end_payload.payload else {
                return Ok(false);
            };
            let end_text = plain_text_from_spans(end_spans);
            let start_range = safe_char_range(
                &current_text,
                selection.start.offset..selection.start.offset,
            );
            let end_range = safe_char_range(&end_text, selection.end.offset..selection.end.offset);
            (
                slice_rich_text_spans(current_spans, 0..start_range.start),
                slice_rich_text_spans(end_spans, end_range.start..end_text.len()),
            )
        } else {
            let range = self
                .focused_text_selection_range()
                .map(|range| safe_char_range(&current_text, range))
                .unwrap_or_else(|| {
                    let caret = self
                        .caret_offset_for_block(current_block_id)
                        .unwrap_or(current_text.len());
                    safe_char_range(&current_text, caret..caret)
                });
            (
                slice_rich_text_spans(current_spans, 0..range.start),
                slice_rich_text_spans(current_spans, range.end..current_text.len()),
            )
        };
        let range = if cross_selection.is_some() {
            None
        } else {
            Some(
                self.focused_text_selection_range()
                    .map(|range| safe_char_range(&current_text, range))
                    .unwrap_or_else(|| {
                        let caret = self
                            .caret_offset_for_block(current_block_id)
                            .unwrap_or(current_text.len());
                        safe_char_range(&current_text, caret..caret)
                    }),
            )
        };
        let mut first_spans = prefix;
        first_spans.extend(fragments[0].spans.clone());
        let replaces_entire_target = range
            .as_ref()
            .is_some_and(|range| range.start == 0 && range.end == current_text.len());
        let after_current_kind = if replaces_entire_target
            && fragments[0].starts_at_block_start
            && fragments[0].ends_at_block_end
        {
            fragments[0].kind.clone()
        } else {
            before_current.kind.clone()
        };
        let after_current_payload = BlockPayload::RichText {
            spans: coalesce_clipboard_spans(first_spans),
        };

        let parent_id = self.document.index.parent_ids[current_index];
        let depth = self.document.index.depths[current_index];
        let insert_at = cross_plan
            .as_ref()
            .map(|plan| plan.replacement_index)
            .unwrap_or_else(|| self.subtree_end(current_index));
        let mut next_id = self.next_available_block_id();
        let mut id_map = HashMap::with_capacity(fragments.len());
        id_map.insert(fragments[0].source_id, current_block_id);
        for fragment in fragments.iter().skip(1) {
            id_map.insert(fragment.source_id, next_id);
            next_id = next_id.saturating_add(1);
        }

        let mut inserted_records = Vec::with_capacity(fragments.len() - 1);
        let mut inserted_payloads = Vec::with_capacity(fragments.len() - 1);
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
                content_version: 0,
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

        let before_selection = self.document_selection_snapshot();
        let after_selection = Some(DocumentSelection::caret(TextPosition {
            block_id: focus_block_id,
            offset: focus_offset,
            affinity: TextAffinity::Downstream,
        }));
        let mut preconditions = Vec::with_capacity(inserted_records.len() + 2);
        preconditions.push(TransactionPrecondition::StructureVersion(
            self.structure_version(),
        ));
        preconditions.push(TransactionPrecondition::BlockContentVersion {
            block_id: current_block_id,
            version: before_current.content_version,
        });
        preconditions.extend(
            inserted_records
                .iter()
                .map(|record| TransactionPrecondition::BlockAbsent(record.id)),
        );
        let mut ops = vec![EditOperation::Block(
            cditor_core::edit::BlockEditOperation::ReplacePayload {
                block_id: current_block_id,
                before_kind: before_current.kind.clone(),
                before_payload: before_current.payload.clone(),
                after_kind: after_current_kind.clone(),
                after_payload: after_current_payload.clone(),
            },
        )];
        let (structure_ops, mut inverse_ops) = if let Some(plan) = &cross_plan {
            plan.structure_operations(inserted_records, inserted_payloads)?
        } else {
            let inserted_end = insert_at.saturating_add(inserted_records.len());
            (
                vec![EditOperation::InsertBlocks {
                    index: insert_at,
                    blocks: inserted_records,
                    payloads: inserted_payloads,
                }],
                vec![EditOperation::DeleteBlockRange {
                    range: insert_at..inserted_end,
                }],
            )
        };
        ops.extend(structure_ops);
        inverse_ops.push(EditOperation::Block(
            cditor_core::edit::BlockEditOperation::ReplacePayload {
                block_id: current_block_id,
                before_kind: after_current_kind,
                before_payload: after_current_payload,
                after_kind: before_current.kind,
                after_payload: before_current.payload,
            },
        ));
        self.cancel_composition();
        self.apply_local_structure_transaction(
            EditTransactionKind::Paste,
            cditor_core::edit::ChangeOrigin::Import,
            ops,
            inverse_ops,
            preconditions,
            before_selection,
            after_selection,
            self.selected_block_ids_snapshot(),
            Vec::new(),
        )?;
        self.focus_block_at_offset(focus_block_id, focus_offset)?;
        Ok(true)
    }
}

fn auxiliary_clipboard_spans(selection: &ClipboardSelection) -> Vec<InlineSpan> {
    match selection {
        ClipboardSelection::Inline { spans } => spans.clone(),
        ClipboardSelection::TextFragments { fragments } => fragments
            .iter()
            .enumerate()
            .flat_map(|(index, fragment)| {
                let mut spans = Vec::with_capacity(fragment.spans.len() + usize::from(index > 0));
                if index > 0 {
                    spans.push(InlineSpan::plain("\n"));
                }
                spans.extend(fragment.spans.clone());
                spans
            })
            .collect(),
        ClipboardSelection::Blocks { blocks } => vec![InlineSpan::plain(
            blocks
                .iter()
                .map(|block| block.payload.plain_text())
                .collect::<Vec<_>>()
                .join("\n"),
        )],
        ClipboardSelection::Table { table } => vec![InlineSpan::plain(table.plain_text())],
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
