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
        let text = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        let content_version = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.content_version)
            .unwrap_or(1);
        Ok(TextSnapshot {
            text,
            content_version,
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
        self.payload_window.insert(current_payload.clone());
        self.text_models.insert(
            current_payload.block_id,
            PieceTableTextModel::new(current_payload.plain_text()),
        );
        if redo {
            for block_id in deleted_ids {
                self.payload_window.payloads.remove(&block_id);
                self.text_models.remove(&block_id);
            }
            for payload in &step.inserted_payloads {
                self.payload_window.insert(payload.clone());
                self.text_models.insert(
                    payload.block_id,
                    PieceTableTextModel::new(payload.plain_text()),
                );
            }
        } else {
            for block_id in inserted_ids {
                self.payload_window.payloads.remove(&block_id);
                self.text_models.remove(&block_id);
            }
            for payload in &step.deleted_payloads {
                self.payload_window.insert(payload.clone());
                self.text_models.insert(
                    payload.block_id,
                    PieceTableTextModel::new(payload.plain_text()),
                );
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
        {
            let model = self
                .text_models
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            model
                .replace_range(0..model.len(), &snapshot.text)
                .map_err(|error| format!("{error:?}"))?;
        }
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.content_version = snapshot.content_version;
            editing.caret_anchor.text_offset = snapshot.text.len() as u64;
        }
        if let Some(payload) = self.payload_window.payloads.get_mut(&block_id) {
            payload.content_version = snapshot.content_version;
            payload.payload = text_payload_for_existing(&payload.payload, &snapshot.text);
        }
        self.selected_block_ids.clear();
        Ok(())
    }
}
