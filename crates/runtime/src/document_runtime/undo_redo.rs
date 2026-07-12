use super::*;

impl DocumentRuntime {
    pub fn undo_focused_block(&mut self) -> Result<bool, String> {
        let Some(event) = self.undo_events.pop() else {
            return Ok(false);
        };
        match event {
            RuntimeUndoEvent::Text(block_id) => {
                let Some(previous) = self.undo_stacks.get_mut(&block_id).and_then(Vec::pop) else {
                    return Ok(false);
                };
                let current = self.snapshot(block_id)?;
                self.redo_stacks.entry(block_id).or_default().push(current);
                self.restore_snapshot(block_id, previous)?;
                self.redo_events.push(event);
                Ok(true)
            }
            RuntimeUndoEvent::StructureMove => {
                let Some(step) = self.structure_undo_stack.pop() else {
                    return Ok(false);
                };
                if self.move_block_subtree_to_parent_untracked(
                    step.block_id,
                    step.old_parent_id,
                    step.old_sibling_index,
                )? {
                    self.focus_block(step.block_id);
                    self.structure_redo_stack.push(step);
                    self.redo_events.push(event);
                    self.queue_structure_move_transaction(step, false);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            RuntimeUndoEvent::StructurePaste => {
                let Some(step) = self.paste_undo_stack.pop() else {
                    return Ok(false);
                };
                self.apply_structure_paste_step(&step, false)?;
                self.paste_redo_stack.push(step);
                self.redo_events.push(event);
                Ok(true)
            }
        }
    }

    pub fn redo_focused_block(&mut self) -> Result<bool, String> {
        let Some(event) = self.redo_events.pop() else {
            return Ok(false);
        };
        match event {
            RuntimeUndoEvent::Text(block_id) => {
                let Some(next) = self.redo_stacks.get_mut(&block_id).and_then(Vec::pop) else {
                    return Ok(false);
                };
                let current = self.snapshot(block_id)?;
                self.undo_stacks.entry(block_id).or_default().push(current);
                self.restore_snapshot(block_id, next)?;
                self.undo_events.push(event);
                Ok(true)
            }
            RuntimeUndoEvent::StructureMove => {
                let Some(step) = self.structure_redo_stack.pop() else {
                    return Ok(false);
                };
                if self.move_block_subtree_to_parent_untracked(
                    step.block_id,
                    step.new_parent_id,
                    step.new_sibling_index,
                )? {
                    self.focus_block(step.block_id);
                    self.structure_undo_stack.push(step);
                    self.undo_events.push(event);
                    self.queue_structure_move_transaction(step, true);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            RuntimeUndoEvent::StructurePaste => {
                let Some(step) = self.paste_redo_stack.pop() else {
                    return Ok(false);
                };
                self.apply_structure_paste_step(&step, true)?;
                self.paste_undo_stack.push(step);
                self.undo_events.push(event);
                Ok(true)
            }
        }
    }

    fn snapshot(&self, block_id: BlockId) -> Result<TextSnapshot, String> {
        let payload = self
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        Ok(TextSnapshot {
            kind: payload.kind,
            payload: payload.payload,
            content_version: payload.content_version,
            focused_table_cell: self
                .focused_table_cell
                .filter(|focused| focused.block_id == block_id),
        })
    }

    pub(super) fn push_undo_snapshot(&mut self, block_id: BlockId) -> Result<(), String> {
        let snapshot = self.snapshot(block_id)?;
        let stack = self.undo_stacks.entry(block_id).or_default();
        if stack.last() != Some(&snapshot) {
            stack.push(snapshot);
            if stack.len() > 100 {
                stack.remove(0);
            }
            self.undo_events.push(RuntimeUndoEvent::Text(block_id));
            if self.undo_events.len() > 1_000 {
                self.undo_events.remove(0);
            }
            self.redo_events.clear();
        }
        self.redo_stacks.remove(&block_id);
        Ok(())
    }

    pub(super) fn record_structure_paste(&mut self, step: StructurePasteUndoStep) {
        self.paste_undo_stack.push(step);
        if self.paste_undo_stack.len() > 100 {
            self.paste_undo_stack.remove(0);
        }
        self.paste_redo_stack.clear();
        self.undo_events.push(RuntimeUndoEvent::StructurePaste);
        if self.undo_events.len() > 1_000 {
            self.undo_events.remove(0);
        }
        self.redo_events.clear();
    }

    fn apply_structure_paste_step(
        &mut self,
        step: &StructurePasteUndoStep,
        redo: bool,
    ) -> Result<(), String> {
        let mut records = self.index_records();
        let inserted_ids = step
            .inserted_records
            .iter()
            .map(|record| record.id)
            .collect::<HashSet<_>>();
        let deleted_ids = step
            .deleted_records
            .iter()
            .map(|record| record.id)
            .collect::<HashSet<_>>();
        records.retain(|record| !inserted_ids.contains(&record.id));
        if redo {
            records.retain(|record| !deleted_ids.contains(&record.id));
        }
        let current_record = if redo {
            step.after_current_record
        } else {
            step.before_current_record
        };
        if let Some(index) = records
            .iter()
            .position(|record| record.id == step.current_block_id)
        {
            records[index] = current_record;
        } else {
            records.push(current_record);
        }
        let current_position = records
            .iter()
            .position(|record| record.id == step.current_block_id)
            .unwrap_or(records.len().saturating_sub(1));
        if redo {
            let insert_at = current_position.saturating_add(1).min(records.len());
            records.splice(insert_at..insert_at, step.inserted_records.clone());
        } else {
            let restore_at = current_position.saturating_add(1).min(records.len());
            records.splice(restore_at..restore_at, step.deleted_records.clone());
        }

        let current_payload = if redo {
            step.after_current_payload.clone()
        } else {
            step.before_current_payload.clone()
        };
        let mut current_payload = normalize_payload_record_for_kind(current_payload);
        self.sync_table_runtime_from_loaded_record(&mut current_payload);
        self.payload_window.insert(current_payload.clone());
        if redo {
            for block_id in deleted_ids {
                self.payload_window.payloads.remove(&block_id);
                self.text_models.remove(&block_id);
                self.table_runtimes.remove(&block_id);
            }
            for payload in &step.inserted_payloads {
                let mut payload = normalize_payload_record_for_kind(payload.clone());
                self.sync_table_runtime_from_loaded_record(&mut payload);
                self.payload_window.insert(payload.clone());
            }
        } else {
            for block_id in inserted_ids {
                self.payload_window.payloads.remove(&block_id);
                self.text_models.remove(&block_id);
                self.table_runtimes.remove(&block_id);
            }
            for payload in &step.deleted_payloads {
                let mut payload = normalize_payload_record_for_kind(payload.clone());
                self.sync_table_runtime_from_loaded_record(&mut payload);
                self.payload_window.insert(payload.clone());
            }
        }
        self.rebuild_structure_index(records)?;
        let focus = if redo {
            step.after_focus
        } else {
            step.before_focus
        };
        if let Some((block_id, offset)) = focus {
            let _ = self.focus_block_at_offset(block_id, offset);
        }
        Ok(())
    }

    pub(super) fn record_structure_move(&mut self, step: StructureMoveUndoStep) {
        self.structure_undo_stack.push(step);
        if self.structure_undo_stack.len() > 100 {
            self.structure_undo_stack.remove(0);
        }
        self.structure_redo_stack.clear();
        self.undo_events.push(RuntimeUndoEvent::StructureMove);
        if self.undo_events.len() > 1_000 {
            self.undo_events.remove(0);
        }
        self.redo_events.clear();
    }

    pub(super) fn queue_structure_move_transaction(
        &mut self,
        step: StructureMoveUndoStep,
        forward: bool,
    ) {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let (parent_id, sibling_index, inverse_parent_id, inverse_sibling_index) = if forward {
            (
                step.new_parent_id,
                step.new_sibling_index,
                step.old_parent_id,
                step.old_sibling_index,
            )
        } else {
            (
                step.old_parent_id,
                step.old_sibling_index,
                step.new_parent_id,
                step.new_sibling_index,
            )
        };
        self.pending_structure_transactions
            .push(EditTransaction::new(
                transaction_id,
                EditTransactionKind::BlockStructureChange,
                transaction_id,
                vec![EditOperation::MoveBlockToParent {
                    block_id: step.block_id,
                    parent_id,
                    sibling_index,
                }],
                vec![EditOperation::MoveBlockToParent {
                    block_id: step.block_id,
                    parent_id: inverse_parent_id,
                    sibling_index: inverse_sibling_index,
                }],
            ));
    }

    fn restore_snapshot(
        &mut self,
        block_id: BlockId,
        snapshot: TextSnapshot,
    ) -> Result<(), String> {
        let text = snapshot.payload.plain_text();
        self.replace_block_kind_and_payload(block_id, snapshot.kind, snapshot.payload)?;
        let _ = self.refresh_table_block_height(block_id)?;
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.content_version = snapshot.content_version;
            editing.caret_anchor.text_offset = text.len() as u64;
        }
        if let Some(payload) = self.payload_window.payloads.get_mut(&block_id) {
            payload.content_version = snapshot.content_version;
        }
        self.restore_snapshot_table_focus(block_id, snapshot.focused_table_cell);
        self.selected_block_ids.clear();
        Ok(())
    }

    fn restore_snapshot_table_focus(
        &mut self,
        block_id: BlockId,
        focused: Option<FocusedTableCell>,
    ) {
        let Some(mut focused) = focused.filter(|focused| focused.block_id == block_id) else {
            if self
                .focused_table_cell
                .is_some_and(|focused| focused.block_id == block_id)
            {
                self.focused_table_cell = None;
            }
            return;
        };
        let Some(text) = self.table_cell_plain_text(block_id, focused.row, focused.col) else {
            self.focused_table_cell = None;
            return;
        };
        focused.offset = normalized_grapheme_offset(&text, focused.offset);
        let selected = normalized_grapheme_range(
            &text,
            focused.selected_range_start..focused.selected_range_end,
        );
        focused.selected_range_start = selected.start;
        focused.selected_range_end = selected.end;
        if let (Some(start), Some(end)) = (focused.marked_range_start, focused.marked_range_end) {
            let marked = normalized_grapheme_range(&text, start..end);
            focused.marked_range_start = Some(marked.start);
            focused.marked_range_end = Some(marked.end);
        } else {
            focused.marked_range_start = None;
            focused.marked_range_end = None;
        }
        self.focused_table_cell = Some(focused);
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::TableCell {
                block_id,
                row: focused.row,
                col: focused.col,
            });
            editing.set_selected_range(focused.selected_range(), focused.selection_reversed);
            if let Some(marked_range) = focused.marked_range() {
                editing.set_marked_range(marked_range);
            } else {
                editing.clear_composition();
            }
        }
    }
}
