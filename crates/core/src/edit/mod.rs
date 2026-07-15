use std::collections::VecDeque;
use std::ops::Range;
use std::path::PathBuf;

use crate::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
use crate::ids::BlockId;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

pub type TransactionId = u64;
pub type SnapshotId = u64;
pub type TextOffset = usize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScrollAnchor {
    pub block_id: BlockId,
    pub offset_in_block: f64,
    pub viewport_y: f64,
}

mod selection;
mod text_offsets;
mod transactions;
mod undo;

pub use selection::{
    AccessibilitySelectionProjection, BlockSelectionFragment, DocumentSelection,
    NormalizedSelection, SelectionRange, SelectionResolveError, TextAffinity, TextPosition,
};
pub use text_offsets::{
    BidiDirection, BidiRun, GraphemeIndex, InternalTextOffset, PlatformUtf16Offset,
    TextOffsetError, TextOffsetMap,
};
pub use transactions::{EditOperation, EditTransaction, EditTransactionKind, TableEditOperation};
pub use undo::{
    NonUndoableEditEvent, UndoGroupBoundary, UndoGroupingPolicy, UndoPayload, UndoStack, UndoStep,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_typing_merges_by_time_and_selection_continuity() {
        let mut undo = UndoStack::default();
        let first = typing_tx(1, 0, 42, 0, "a");
        let second = typing_tx(2, 100, 42, 1, "b");

        assert_eq!(undo.record_transaction(first), None);
        assert_eq!(undo.record_transaction(second), None);

        assert_eq!(undo.undo_len(), 1);
        let step = undo.last_undo_step().unwrap();
        let tx = step.inline_transaction().unwrap();
        assert_eq!(tx.ops.len(), 2);
        assert_eq!(tx.inverse_ops.len(), 2);
        assert_eq!(tx.before_selection, Some(selection(42, 0)));
        assert_eq!(tx.after_selection, Some(selection(42, 2)));
    }

    #[test]
    fn selection_change_and_time_gap_create_boundaries() {
        let mut undo = UndoStack::default();
        undo.record_transaction(typing_tx(1, 0, 42, 0, "a"));

        let mut selection_jump = typing_tx(2, 100, 42, 10, "b");
        selection_jump.before_selection = Some(selection(42, 99));
        assert_eq!(
            undo.record_transaction(selection_jump),
            Some(UndoGroupBoundary::SelectionChange)
        );

        let gap = typing_tx(3, 5_000, 42, 11, "c");
        assert_eq!(
            undo.record_transaction(gap),
            Some(UndoGroupBoundary::TimeGap)
        );
        assert_eq!(undo.undo_len(), 3);
    }

    #[test]
    fn composition_commit_is_independent_undo_step() {
        let mut undo = UndoStack::default();
        let mut ime = EditTransaction::insert_text(1, 10, 42, 0, "你好")
            .with_selection(Some(selection(42, 0)), Some(selection(42, 6)));
        ime.kind = EditTransactionKind::CompositionCommit;

        assert_eq!(
            undo.record_transaction(ime),
            Some(UndoGroupBoundary::CompositionCommit)
        );
        assert_eq!(undo.undo_len(), 1);
    }

    #[test]
    fn paste_10k_blocks_uses_snapshot_payload_instead_of_inline_blocks() {
        let mut undo = UndoStack::default();
        let blocks = (0..10_000)
            .map(|index| BlockIndexRecord::new(index as BlockId + 1, None, 0, 1, 0))
            .collect::<Vec<_>>();
        let tx = EditTransaction::paste_blocks(1, 0, 0, blocks);

        assert_eq!(undo.record_transaction(tx), Some(UndoGroupBoundary::Paste));

        let step = undo.last_undo_step().unwrap();
        assert!(matches!(
            step.payload,
            UndoPayload::BlockRangeSnapshot {
                snapshot_id: 1,
                block_count: 10_000
            }
        ));
    }

    #[test]
    fn delete_50k_blocks_undo_does_not_hold_inline_payload() {
        let mut undo = UndoStack::default();
        let tx = EditTransaction::new(
            1,
            EditTransactionKind::BlockStructureChange,
            0,
            vec![EditOperation::DeleteBlockRange { range: 0..50_000 }],
            vec![EditOperation::InsertBlocks {
                index: 0,
                blocks: Vec::new(),
            }],
        );

        assert_eq!(
            undo.record_transaction(tx),
            Some(UndoGroupBoundary::BlockStructureChange)
        );

        let step = undo.last_undo_step().unwrap();
        assert!(matches!(
            step.payload,
            UndoPayload::BlockRangeSnapshot {
                snapshot_id: 1,
                block_count: 50_000
            }
        ));
    }

    #[test]
    fn table_edit_operations_affect_their_table_block() {
        use crate::rich_text::{TableCellAlign, TablePayload, TableRange, TableTrackSize};

        let resize = EditOperation::Table(TableEditOperation::ResizeColumn {
            block_id: 42,
            column: 1,
            old_width: TableTrackSize::Auto,
            new_width: TableTrackSize::Px(180),
        });
        assert_eq!(resize.affected_blocks(), vec![42]);

        let merge = EditOperation::Table(TableEditOperation::MergeCells {
            block_id: 43,
            range: TableRange::normalized(0, 0, 1, 1),
            before: TablePayload::default(),
            after: TablePayload::default(),
        });
        assert_eq!(merge.affected_blocks(), vec![43]);

        let align = EditOperation::Table(TableEditOperation::SetCellAlign {
            block_id: 44,
            range: TableRange::normalized(0, 1, 2, 1),
            old_aligns: vec![vec![TableCellAlign::Left], vec![TableCellAlign::Right]],
            new_align: TableCellAlign::Center,
        });
        assert_eq!(align.affected_blocks(), vec![44]);
    }

    #[test]
    fn inverse_transaction_restores_selection_and_anchor_once() {
        let anchor_before = ScrollAnchor {
            block_id: 42,
            offset_in_block: 10.0,
            viewport_y: 100.0,
        };
        let anchor_after = ScrollAnchor {
            block_id: 42,
            offset_in_block: 20.0,
            viewport_y: 100.0,
        };
        let tx = EditTransaction::insert_text(1, 0, 42, 0, "a")
            .with_selection(Some(selection(42, 0)), Some(selection(42, 1)))
            .with_anchor(Some(anchor_before), Some(anchor_after));

        let inverse = tx.inverse_transaction(2, 10);

        assert_eq!(inverse.ops, tx.inverse_ops);
        assert_eq!(inverse.inverse_ops, tx.ops);
        assert_eq!(inverse.before_selection, tx.after_selection);
        assert_eq!(inverse.after_selection, tx.before_selection);
        assert_eq!(inverse.before_anchor, tx.after_anchor);
        assert_eq!(inverse.after_anchor, tx.before_anchor);

        let mut step = UndoStep {
            payload: UndoPayload::InlineSmall(tx),
            boundary: None,
            selection_restore_count: 0,
            anchor_restore_count: 0,
        };
        assert!(step.restore_user_position_once());
        assert!(!step.restore_user_position_once());
    }

    #[test]
    fn background_events_never_enter_undo_stack() {
        let mut undo = UndoStack::default();

        undo.record_non_undoable_event(NonUndoableEditEvent::HeightCorrection);
        undo.record_non_undoable_event(NonUndoableEditEvent::SyntaxHighlight);
        undo.record_non_undoable_event(NonUndoableEditEvent::FtsUpdate);
        undo.record_non_undoable_event(NonUndoableEditEvent::CacheWrite);

        assert_eq!(undo.undo_len(), 0);
    }

    #[test]
    fn text_offset_map_handles_emoji_zwj_as_single_grapheme() {
        let text = "a👨‍👩‍👧‍👦b";
        let map = TextOffsetMap::build(text);
        let emoji_start = InternalTextOffset(1);
        let emoji_end = InternalTextOffset(text.len() - 1);

        assert!(map.is_grapheme_boundary(emoji_start));
        assert!(map.is_grapheme_boundary(emoji_end));
        assert_eq!(
            map.backspace_range(emoji_end).unwrap(),
            Some(emoji_start..emoji_end)
        );
        assert_eq!(
            map.delete_range(emoji_start).unwrap(),
            Some(emoji_start..emoji_end)
        );
        assert!(
            EditOperation::DeleteText {
                block_id: 42,
                range: emoji_start.0..emoji_end.0,
            }
            .validate_text_range(&map)
            .is_ok()
        );
    }

    #[test]
    fn text_offset_map_rejects_combining_mark_middle_boundary() {
        let text = "e\u{301}x";
        let map = TextOffsetMap::build(text);
        let middle_of_grapheme = InternalTextOffset("e".len());
        let first_cluster_end = InternalTextOffset("e\u{301}".len());

        assert!(!map.is_grapheme_boundary(middle_of_grapheme));
        assert_eq!(
            map.backspace_range(first_cluster_end).unwrap(),
            Some(InternalTextOffset(0)..first_cluster_end)
        );
        assert_eq!(
            EditOperation::DeleteText {
                block_id: 42,
                range: 0..middle_of_grapheme.0,
            }
            .validate_text_range(&map),
            Err(TextOffsetError::NotGraphemeBoundary(middle_of_grapheme))
        );
    }

    #[test]
    fn cjk_internal_and_utf16_offsets_match_at_char_boundaries() {
        let text = "你好吗";
        let map = TextOffsetMap::build(text);

        assert_eq!(
            map.internal_to_utf16(InternalTextOffset("你".len()))
                .unwrap(),
            PlatformUtf16Offset(1)
        );
        assert_eq!(
            map.utf16_to_internal(PlatformUtf16Offset(2)).unwrap(),
            InternalTextOffset("你好".len())
        );
        assert_eq!(
            map.grapheme_index_of(InternalTextOffset("你好".len()))
                .unwrap(),
            GraphemeIndex(2)
        );
    }

    #[test]
    fn text_offset_map_normalizes_invalid_cjk_byte_ranges() {
        let text = "萨德";
        let map = TextOffsetMap::build(text);

        assert_eq!(
            map.normalize_internal_range(InternalTextOffset(2)..InternalTextOffset(2)),
            InternalTextOffset(0)..InternalTextOffset(0)
        );
        assert_eq!(
            map.normalize_internal_range(InternalTextOffset(1)..InternalTextOffset(2)),
            InternalTextOffset(0)..InternalTextOffset("萨".len())
        );
    }

    #[test]
    fn text_offset_map_normalizes_combining_and_emoji_ranges_by_grapheme() {
        let combining = "e\u{301}x";
        let combining_map = TextOffsetMap::build(combining);
        assert_eq!(
            combining_map.normalize_internal_range(InternalTextOffset(1)..InternalTextOffset(2)),
            InternalTextOffset(0)..InternalTextOffset("e\u{301}".len())
        );

        let emoji = "a👨‍👩‍👧‍👦b";
        let emoji_map = TextOffsetMap::build(emoji);
        assert_eq!(
            emoji_map.normalize_internal_range(InternalTextOffset(2)..InternalTextOffset(4)),
            InternalTextOffset(1)..InternalTextOffset(emoji.len() - 1)
        );
    }

    #[test]
    fn rtl_ltr_mixed_text_builds_bidi_runs() {
        let text = "abc שלום def";
        let map = TextOffsetMap::build(text);

        assert!(
            map.bidi_runs()
                .iter()
                .any(|run| run.direction == BidiDirection::Ltr)
        );
        assert!(
            map.bidi_runs()
                .iter()
                .any(|run| run.direction == BidiDirection::Rtl)
        );
    }

    #[test]
    fn ime_marked_range_converts_from_utf16_to_internal_grapheme_range() {
        let text = "a😀中";
        let map = TextOffsetMap::build(text);

        let range = map
            .utf16_range_to_internal_range(PlatformUtf16Offset(1)..PlatformUtf16Offset(3))
            .unwrap();

        assert_eq!(
            range,
            InternalTextOffset(1)..InternalTextOffset("a😀".len())
        );
        assert_eq!(
            map.utf16_to_internal(PlatformUtf16Offset(2)),
            Err(TextOffsetError::InvalidUtf16Offset(PlatformUtf16Offset(2)))
        );
    }

    #[test]
    fn reversed_anchor_focus_normalizes_by_document_order_and_offset() {
        let index = document_index(5);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(4, 1),
            focus: TextPosition::downstream(2, 3),
        };

        let normalized = selection.normalize(&index).unwrap();

        assert_eq!(normalized.start.block_id, 2);
        assert_eq!(normalized.end.block_id, 4);
        assert!(normalized.is_reversed);
    }

    #[test]
    fn cross_page_selection_fragments_only_current_visible_window() {
        let index = document_index(100);
        let visible = VisibleDocumentIndex::from_document_index(&index);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(10, 2),
            focus: TextPosition::downstream(90, 5),
        }
        .normalize(&index)
        .unwrap();

        let fragments = selection
            .visible_selection_fragments(30..35, &index, &visible, |_| 10)
            .unwrap();

        assert_eq!(fragments.len(), 5);
        assert!(
            fragments
                .iter()
                .all(|fragment| fragment.range == SelectionRange::Full)
        );
        assert_eq!(fragments[0].block_id, 31);
        assert_eq!(fragments[4].block_id, 35);
    }

    #[test]
    fn start_and_end_blocks_get_partial_fragments() {
        let index = document_index(5);
        let visible = VisibleDocumentIndex::from_document_index(&index);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(2, 3),
            focus: TextPosition::downstream(4, 1),
        }
        .normalize(&index)
        .unwrap();

        let fragments = selection
            .visible_selection_fragments(0..5, &index, &visible, |_| 10)
            .unwrap();

        assert_eq!(
            fragments[0],
            BlockSelectionFragment {
                block_id: 2,
                range: SelectionRange::Partial(3..10),
            }
        );
        assert_eq!(
            fragments[1],
            BlockSelectionFragment {
                block_id: 3,
                range: SelectionRange::Full,
            }
        );
        assert_eq!(
            fragments[2],
            BlockSelectionFragment {
                block_id: 4,
                range: SelectionRange::Partial(0..1),
            }
        );
    }

    #[test]
    fn hidden_subtree_selection_degrades_endpoint_to_visible_ancestor() {
        use std::collections::HashSet;

        let records = vec![
            BlockIndexRecord::new(1, None, 0, 1, 0),
            BlockIndexRecord::new(2, Some(1), 1, 1, 0),
            BlockIndexRecord::new(3, Some(2), 2, 1, 0),
            BlockIndexRecord::new(4, None, 0, 1, 0),
        ];
        let index = DocumentIndex::new(1, records, 1).unwrap();
        let visible = VisibleDocumentIndex::with_folded_blocks(&index, HashSet::from([2]), 1);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(3, 7),
            focus: TextPosition::downstream(4, 1),
        };

        let degraded = selection.degrade_hidden_endpoints(&index, &visible);

        assert_eq!(degraded.anchor.block_id, 2);
        assert_eq!(degraded.anchor.offset, 0);
        assert_eq!(degraded.focus.block_id, 4);
    }

    #[test]
    fn accessibility_projection_does_not_require_ui_entity_hydration() {
        let index = document_index(100);
        let normalized = DocumentSelection {
            anchor: TextPosition::downstream(20, 0),
            focus: TextPosition::downstream(80, 0),
        }
        .normalize(&index)
        .unwrap();

        let projection = normalized.accessibility_projection(&index, 50, 2).unwrap();

        assert_eq!(projection.focused_block_id, 50);
        assert_eq!(projection.semantic_block_range, 19..80);
        assert!(!projection.hydrated_ui_entities_required);
    }

    fn typing_tx(
        id: TransactionId,
        timestamp: u64,
        block_id: BlockId,
        offset: usize,
        text: &str,
    ) -> EditTransaction {
        EditTransaction::insert_text(id, timestamp, block_id, offset, text).with_selection(
            Some(selection(block_id, offset)),
            Some(selection(block_id, offset + text.len())),
        )
    }

    fn selection(block_id: BlockId, offset: usize) -> DocumentSelection {
        DocumentSelection::caret(TextPosition::downstream(block_id, offset))
    }

    fn document_index(count: usize) -> DocumentIndex {
        let records = (0..count)
            .map(|index| BlockIndexRecord::new(index as BlockId + 1, None, 0, 1, 0))
            .collect::<Vec<_>>();
        DocumentIndex::new(1, records, 1).unwrap()
    }
}
