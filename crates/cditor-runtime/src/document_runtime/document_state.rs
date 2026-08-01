use super::*;

/// Content truth and document-derived indexes owned by the runtime.
///
/// This state stays private to the runtime facade. Callers observe it through
/// focused queries so document mutations continue to flow through commands.
#[derive(Debug)]
pub(super) struct DocumentState {
    pub(super) metadata: DocumentMetadata,
    pub(super) revision: u64,
    pub(super) index: DocumentIndex,
    pub(super) visible_index: VisibleDocumentIndex,
    pub(super) payload_window: PayloadWindow,
    pub(super) block_attrs: HashMap<BlockId, BlockAttrs>,
    pub(super) collection_records: HashMap<CollectionId, Vec<CollectionRecordSnapshot>>,
    pub(super) comment_threads: HashMap<CommentThreadId, CommentThreadSnapshot>,
    pub(super) assets: HashMap<AssetId, AssetSnapshot>,
    pub(super) block_asset_ids: HashMap<BlockId, BTreeSet<AssetId>>,
    pub(super) table_runtimes: HashMap<BlockId, TableRuntime>,
    pub(super) text_models: HashMap<BlockId, PieceTableTextModel>,
    pub(super) list_projection_cache: ListProjectionCache,
    pub(super) demo_payload_count: Option<usize>,
}

#[cfg(test)]
mod tests {
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;

    #[test]
    fn extraction_preserves_constructed_document_truth() {
        let runtime = DocumentRuntime::from_payloads(
            7,
            vec![
                BlockPayloadRecord::rich_text(10, RichBlockKind::Paragraph, "alpha"),
                BlockPayloadRecord::rich_text(11, RichBlockKind::Paragraph, "beta"),
            ],
            720.0,
        );

        assert_eq!(runtime.document_id, 7);
        assert_eq!(runtime.revision(), 1);
        assert_eq!(runtime.document.index.block_ids, vec![10, 11]);
        assert_eq!(runtime.visible_block_ids(), &[10, 11]);
        assert_eq!(runtime.payload_window_range(), 0..2);
        assert_eq!(
            runtime.block_payload_record(10).unwrap().plain_text(),
            "alpha"
        );
    }

    #[test]
    fn extraction_preserves_dispatch_transaction_contract() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        for ch in "hello".chars() {
            runtime.insert_char(ch).unwrap();
        }
        runtime.set_document_text_selection(1, 0, 1, 5).unwrap();
        let before_revision = runtime.revision();
        let before_transaction = runtime.last_committed_transaction_id();
        let before_text = runtime.block_payload_record(1).unwrap().plain_text();

        let outcome = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::ToggleBold,
                CommandSource::Toolbar,
            ))
            .unwrap();

        assert_eq!(runtime.revision(), before_revision + 1);
        assert_eq!(outcome.transaction_ids.len(), 1);
        assert_ne!(runtime.last_committed_transaction_id(), before_transaction);
        assert_eq!(
            runtime.last_committed_transaction_id(),
            outcome.transaction_ids.last().copied()
        );
        assert_eq!(
            runtime.block_payload_record(1).unwrap().plain_text(),
            before_text
        );
    }
}
