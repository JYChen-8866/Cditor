use super::*;

impl DocumentRuntime {
    pub fn document_title(&self) -> Option<&str> {
        self.document_title.as_deref()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Records a committed content change at the document-kernel boundary.
    pub fn note_content_changed(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_events.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_events.is_empty()
    }

    pub fn document_block_count(&self) -> usize {
        self.index.total_count()
    }

    pub fn loaded_payload_count(&self) -> usize {
        self.payload_window.payloads.len()
    }

    pub fn dirty_payload_count(&self) -> usize {
        self.payload_window
            .payloads
            .keys()
            .filter(|block_id| self.payload_window.is_dirty(**block_id))
            .count()
    }

    pub fn pending_layout_task_count(&self) -> usize {
        self.pending_measured_heights.len()
    }

    pub fn estimated_document_height(&self) -> f64 {
        self.height_index.total_height()
    }

    pub fn estimated_payload_memory_bytes(&self) -> usize {
        self.payload_window.total_estimated_bytes()
    }

    pub fn document_selection_snapshot(&self) -> Option<DocumentSelection> {
        self.document_selection.or_else(|| {
            let block_id = self.focused_block_id()?;
            let offset = self.caret_offset_for_block(block_id)?;
            Some(DocumentSelection::caret(TextPosition::downstream(
                block_id, offset,
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_revision_is_monotonic_for_content_changes() {
        let mut runtime = DocumentRuntime::empty();
        let initial = runtime.revision();

        let first = runtime.note_content_changed();
        let second = runtime.note_content_changed();

        assert_eq!(first, initial + 1);
        assert_eq!(second, first + 1);
    }

    #[test]
    fn undo_and_redo_capabilities_follow_runtime_stacks() {
        let mut runtime = DocumentRuntime::empty();
        assert!(!runtime.can_undo());
        assert!(!runtime.can_redo());

        runtime.focus_block_at_offset(1, 0).unwrap();
        runtime.insert_char('x').unwrap();
        assert!(runtime.can_undo());

        runtime.undo_focused_block().unwrap();
        assert!(runtime.can_redo());
    }
}
