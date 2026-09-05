use super::*;

/// Sets `prev_id`/`next_id` on the given sibling block ids (already in
/// document order), looking the blocks up in the reconstructed list.
fn link_siblings(blocks: &mut [RichBlockRecord], parent_id: Option<BlockId>, siblings: &[BlockId]) {
    for (index, block_id) in siblings.iter().copied().enumerate() {
        let Some(block) = blocks.iter_mut().find(|block| block.id == block_id) else {
            continue;
        };
        block.prev_id = index.checked_sub(1).map(|previous| siblings[previous]);
        block.next_id = siblings.get(index + 1).copied();
        block.parent_id = parent_id;
    }
}

impl DocumentRuntime {
    /// Returns the required first-root document-name block without
    /// materializing the document or scanning its payload window.
    pub fn document_title_block_id(&self) -> Option<BlockId> {
        let block_id = self.document.index.block_ids.first().copied()?;
        self.is_document_title_block(block_id).then_some(block_id)
    }

    pub(crate) fn is_document_title_block(&self, block_id: BlockId) -> bool {
        self.document
            .payload_window
            .get(block_id)
            .is_some_and(|record| record.kind.is_document_title())
    }

    /// Materializes the complete rich-text document model behind this runtime.
    ///
    /// The runtime normally keeps the document decomposed for large-document
    /// performance; this reconstruction is only for whole-document exports.
    /// Blocks whose heavyweight payload was evicted from the in-memory cache
    /// by cache maintenance are omitted (they are placeholders), so a
    /// complete export should run on a fresh session whose payload window
    /// covers the whole document.
    pub fn rich_text_document(&self) -> RichTextDocument {
        let mut blocks = Vec::with_capacity(self.document.index.block_ids.len());
        let mut root_blocks = Vec::new();
        let mut children: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

        for (index, block_id) in self.document.index.block_ids.iter().copied().enumerate() {
            let Some(payload) = self.document.payload_window.get(block_id) else {
                continue;
            };
            let parent_id = self.document.index.parent_ids[index];
            let mut block =
                RichBlockRecord::new(block_id, payload.kind.clone(), payload.payload.clone());
            block.document_id = self.document_id;
            block.parent_id = parent_id;
            block.depth = self.document.index.depths[index];
            block.attrs = self
                .document
                .block_attrs
                .get(&block_id)
                .cloned()
                .unwrap_or_default();
            block.content_version = payload.content_version;
            block.structure_version = self.document.index.structure_version;
            block.measured_height = self.document.index.layout_meta[index].measured_height;
            block.estimated_height = self.document.index.layout_meta[index].estimated_height;
            match parent_id {
                Some(parent) => children.entry(parent).or_default().push(block_id),
                None => root_blocks.push(block_id),
            }
            blocks.push(block);
        }

        // Restore sibling links and children lists from the structural order.
        for block in &mut blocks {
            block.children = children.remove(&block.id).unwrap_or_default();
        }
        for (parent_id, siblings) in children {
            link_siblings(&mut blocks, Some(parent_id), &siblings);
        }
        link_siblings(&mut blocks, None, &root_blocks);

        let mut metadata = self.document.metadata.clone();
        metadata.name = self.document_name().map(ToOwned::to_owned);
        metadata.title = None;

        RichTextDocument {
            id: self.document_id,
            version: cditor_core::rich_text::document::CURRENT_RICH_TEXT_FORMAT_VERSION,
            metadata,
            root_blocks,
            blocks,
            structure_version: self.document.index.structure_version,
        }
    }

    pub fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub fn document_title(&self) -> Option<&str> {
        self.document_name()
    }

    pub fn document_name(&self) -> Option<&str> {
        if let Some(title) = self.document.index.block_ids.iter().find_map(|block_id| {
            self.document
                .payload_window
                .get(*block_id)
                .filter(|record| matches!(record.kind, RichBlockKind::DocumentTitle))
                .and_then(|_| self.document.text_models.get(block_id))
                .map(|model| model.text())
        }) {
            return (!title.trim().is_empty()).then_some(title);
        }
        self.document
            .metadata
            .name
            .as_deref()
            .or(self.document.metadata.title.as_deref())
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
        self.document.revision
    }

    /// Kept as a compatibility query. H1 blocks no longer define the name.
    pub fn auto_document_title(&self) -> Option<String> {
        None
    }

    pub fn set_document_name(&mut self, name: impl Into<String>) -> bool {
        let name = name.into();
        if let Some(block_id) = self
            .document
            .index
            .block_ids
            .iter()
            .copied()
            .find(|block_id| {
                self.document
                    .payload_window
                    .get(*block_id)
                    .is_some_and(|record| matches!(record.kind, RichBlockKind::DocumentTitle))
            })
        {
            let current = self
                .document
                .payload_window
                .get(block_id)
                .map(BlockPayloadRecord::plain_text)
                .unwrap_or_default();
            if current == name {
                return false;
            }
            if let Some(record) = self.document.payload_window.get_mut(block_id) {
                record.payload = BlockPayload::RichText {
                    spans: vec![InlineSpan::plain(name.clone())],
                };
                record.content_version = record.content_version.saturating_add(1);
                sync_text_model_for_payload(&mut self.document.text_models, record);
            }
            self.document.metadata.name = (!name.trim().is_empty()).then_some(name);
            self.document.metadata.title = None;
            self.note_content_changed();
            return true;
        }
        let name = (!name.trim().is_empty()).then_some(name);
        if self.document.metadata.name == name {
            return false;
        }
        self.document.metadata.name = name;
        self.document.metadata.title = None;
        self.document.revision = self.document.revision.saturating_add(1);
        true
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
    fn document_name_is_independent_from_the_first_h1() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "My Doc"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 1 }, "Second H1"),
            ],
            720.0,
        );

        assert_eq!(runtime.document_name(), None);
        assert!(runtime.set_document_name("Named page"));
        assert_eq!(runtime.document_name(), Some("Named page"));
    }

    #[test]
    fn h1_content_does_not_supply_a_document_name() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, ""),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 1 }, "Real Title"),
            ],
            720.0,
        );

        assert_eq!(runtime.document_name(), None);
    }

    #[test]
    fn document_name_does_not_follow_h1_edits_or_deletion() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "My Doc"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "body"),
            ],
            720.0,
        );

        assert!(runtime.set_document_name("Stable name"));
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
        assert_eq!(runtime.document_name(), Some("Stable name"));

        runtime.delete_block_by_id(1).unwrap();
        assert_eq!(runtime.document_name(), Some("Stable name"));
    }

    #[test]
    fn legacy_title_is_migrated_to_document_name() {
        let mut document = RichTextDocument::empty(1);
        document.metadata.title = Some("Untitled".to_owned());
        document.push_root_block(RichBlockRecord::paragraph(1, "body"));

        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);

        assert_eq!(runtime.document_name(), Some("Untitled"));
        assert_eq!(
            runtime.document_metadata().name.as_deref(),
            Some("Untitled")
        );
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
        runtime.document.index.layout_meta[1].layout_version = 37;

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
