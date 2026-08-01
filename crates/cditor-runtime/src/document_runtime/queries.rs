use super::*;

impl DocumentRuntime {
    pub fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub fn document_title(&self) -> Option<&str> {
        self.document.metadata.title.as_deref()
    }

    pub fn document_metadata(&self) -> &DocumentMetadata {
        &self.document.metadata
    }

    pub fn page_cover(&self) -> Option<&PageCover> {
        self.document.metadata.cover.as_ref()
    }

    pub fn page_icon(&self) -> Option<&PageIcon> {
        self.document.metadata.icon.as_ref()
    }

    pub fn revision(&self) -> u64 {
        self.document.revision
    }

    pub fn last_committed_transaction_id(&self) -> Option<u64> {
        self.transactions.last_committed_id
    }

    /// Records a committed content change at the document-kernel boundary.
    pub fn note_content_changed(&mut self) -> u64 {
        self.document.revision = self.document.revision.saturating_add(1);
        self.sync_auto_document_title();
        self.document.revision
    }

    /// Derives the display title from the first non-empty H1 block.
    ///
    /// An existing title is left untouched when the document has no H1 text,
    /// so host-provided file names survive documents without headings.
    pub(crate) fn sync_auto_document_title(&mut self) {
        let Some(title) = self.auto_document_title() else {
            return;
        };
        if self.document.metadata.title.as_deref() == Some(title.as_str()) {
            return;
        }
        self.document.metadata.title = Some(title);
    }

    /// Returns the trimmed first non-empty H1 text, if the document has one.
    pub fn auto_document_title(&self) -> Option<String> {
        self.document.index.block_ids.iter().find_map(|block_id| {
            let record = self.document.payload_window.get_shared(*block_id)?;
            if !matches!(&record.kind, RichBlockKind::Heading { level: 1 }) {
                return None;
            }
            let text = record.plain_text();
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        })
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo_events.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo_events.is_empty()
    }

    pub fn document_block_count(&self) -> usize {
        self.document.index.total_count()
    }

    pub fn loaded_payload_count(&self) -> usize {
        self.document.payload_window.payloads.len()
    }

    pub fn visible_block_ids(&self) -> &[BlockId] {
        &self.document.visible_index.visible_block_ids
    }

    pub fn payload_window_range(&self) -> Range<usize> {
        self.document.payload_window.block_range.clone()
    }

    pub fn pending_payload_load_count(&self) -> usize {
        self.document.payload_window.loading.len()
    }

    pub fn dirty_payload_count(&self) -> usize {
        self.document
            .payload_window
            .payloads
            .keys()
            .filter(|block_id| self.document.payload_window.is_dirty(**block_id))
            .count()
    }

    pub fn pending_layout_task_count(&self) -> usize {
        self.layout.pending_measured_heights.len()
    }

    pub fn global_scroll_top(&self) -> f64 {
        self.layout.scroll.global_scroll_top
    }

    pub fn viewport_height(&self) -> f64 {
        self.layout.scroll.viewport_height
    }

    pub fn model_total_height(&self) -> f64 {
        self.layout.scroll.model_total_height
    }

    pub fn page_layout_total_height(&self) -> f64 {
        self.layout.page_layout.total_height()
    }

    pub fn page_layout_snapshot(&self) -> PageLayoutIndex {
        self.layout.page_layout.clone()
    }

    pub fn block_layout_version(&self, block_id: BlockId) -> Option<u64> {
        let index = self.document.index.index_of(block_id)?;
        Some(self.document.index.layout_meta[index].layout_version)
    }

    pub fn block_layout_meta(&self, block_id: BlockId) -> Option<BlockLayoutMeta> {
        let index = self.document.index.index_of(block_id)?;
        Some(self.document.index.layout_meta[index])
    }

    pub fn estimated_document_height(&self) -> f64 {
        self.layout.height_index.total_height()
    }

    pub fn estimated_payload_memory_bytes(&self) -> usize {
        self.document.payload_window.total_estimated_bytes()
    }

    pub fn estimated_text_undo_memory_bytes(&self) -> usize {
        self.estimated_text_history_memory_bytes()
    }

    pub const fn text_undo_memory_budget_bytes(&self) -> usize {
        super::undo_redo::TEXT_UNDO_MAX_ESTIMATED_BYTES
    }

    pub fn document_selection_snapshot(&self) -> Option<DocumentSelection> {
        self.selection.document_selection.or_else(|| {
            let editing = self.editing.session.as_ref()?;
            let InputTarget::BlockText { block_id } = editing.input_target else {
                return None;
            };
            Some(DocumentSelection::caret(
                self.caret_position_for_block(block_id)?,
            ))
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
    fn document_title_derives_from_the_first_non_empty_h1() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "My Doc"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 1 }, "Second H1"),
            ],
            720.0,
        );

        assert_eq!(runtime.document_title(), Some("My Doc"));
    }

    #[test]
    fn document_title_skips_an_empty_h1_and_uses_the_next_heading() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, ""),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 1 }, "Real Title"),
            ],
            720.0,
        );

        assert_eq!(runtime.document_title(), Some("Real Title"));
    }

    #[test]
    fn document_title_follows_first_h1_edits_and_survives_deletion() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "My Doc"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "body"),
            ],
            720.0,
        );

        runtime.focus_block_at_offset(1, 0).unwrap();
        let expected = runtime.input_session_identity().unwrap();
        runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::ReplaceText {
                    range: None,
                    text: "X",
                },
            })
            .unwrap();
        assert_eq!(runtime.document_title(), Some("XMy Doc"));

        runtime.delete_block_by_id(1).unwrap();
        assert_eq!(runtime.document_title(), Some("XMy Doc"));
    }

    #[test]
    fn document_title_keeps_host_title_when_there_is_no_h1() {
        let mut document = RichTextDocument::empty(1);
        document.metadata.title = Some("Untitled".to_owned());
        document.push_root_block(RichBlockRecord::paragraph(1, "body"));

        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);

        assert_eq!(runtime.document_title(), Some("Untitled"));
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

    #[test]
    fn block_layout_version_reads_the_authoritative_index_identity() {
        let mut runtime = DocumentRuntime::empty();
        runtime.document.index.layout_meta[0].layout_version = 37;

        assert_eq!(runtime.block_layout_version(1), Some(37));
        assert_eq!(runtime.block_layout_version(999), None);
    }

    #[test]
    fn complex_block_focus_does_not_fabricate_a_text_selection_snapshot() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Whiteboard,
                payload: default_whiteboard_payload(),
            }],
            720.0,
        );
        runtime.focus_block(1);

        assert!(runtime.document_selection_snapshot().is_none());
    }
}
