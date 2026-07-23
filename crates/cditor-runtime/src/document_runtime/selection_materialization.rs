use super::*;
use cditor_core::edit::UnifiedDocumentSelection;

/// Identifies the payloads required to execute a selection operation.
///
/// The request is intentionally tied to both the document structure and the
/// exact unified selection. A storage response must be discarded when either
/// changes while the request is in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionMaterializationRequest {
    pub document_id: DocumentId,
    pub structure_version: u64,
    pub selection: UnifiedDocumentSelection,
    pub block_ids: Vec<BlockId>,
    pub payload_window_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMaterializationApplyDecision {
    Applied,
    DiscardedStale,
}

impl DocumentRuntime {
    /// Returns the missing payloads needed by copy/cut/delete for the current
    /// selection. `None` means there is no active selection or everything is
    /// already resident.
    pub fn selection_materialization_request(&self) -> Option<SelectionMaterializationRequest> {
        let selection = self.unified_document_selection_snapshot()?;
        let mut ids = Vec::new();

        if !self.selected_block_ids.is_empty() {
            ids = self.selected_block_subtree_ids();
            if let (Some(first), Some(last)) = (ids.first(), ids.last()) {
                let start = self.document.index.index_of(*first)?;
                let end = self.document.index.index_of(*last)?.saturating_add(1);
                let survivor = if start == 0 && end == self.document.index.total_count() {
                    Some(0)
                } else if start > 0 {
                    Some(start - 1)
                } else if end < self.document.index.total_count() {
                    Some(end)
                } else {
                    None
                };
                if let Some(index) = survivor {
                    ids.push(self.document.index.block_ids[index]);
                }
            }
        } else if let Some(normalized) = self
            .document_selection
            .and_then(|selection| selection.normalize(&self.document.index).ok())
        {
            let start = self.document.index.index_of(normalized.start.block_id)?;
            let end = self.document.index.index_of(normalized.end.block_id)?;
            ids.extend(self.document.index.block_ids[start..=end].iter().copied());
        } else if let Some(block_id) = self.focused_block_id() {
            ids.push(block_id);
        }

        ids.sort_unstable_by_key(|block_id| {
            self.document
                .index
                .index_of(*block_id)
                .unwrap_or(usize::MAX)
        });
        ids.dedup();
        let missing = ids
            .into_iter()
            .filter(|block_id| !self.document.payload_window.payloads.contains_key(block_id))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return None;
        }

        Some(SelectionMaterializationRequest {
            document_id: self.document_id,
            structure_version: self.structure_version(),
            selection,
            block_ids: missing,
            payload_window_generation: self.payload_window_generation,
        })
    }

    pub(super) fn selected_block_subtree_ids(&self) -> Vec<BlockId> {
        let mut roots = self
            .selected_block_ids
            .iter()
            .filter_map(|block_id| self.document.index.index_of(*block_id))
            .filter(|index| {
                let mut parent = self.document.index.parent_ids[*index];
                while let Some(parent_id) = parent {
                    if self.selected_block_ids.contains(&parent_id) {
                        return false;
                    }
                    parent = self
                        .document
                        .index
                        .index_of(parent_id)
                        .and_then(|position| self.document.index.parent_ids[position]);
                }
                true
            })
            .collect::<Vec<_>>();
        roots.sort_unstable();
        roots
            .into_iter()
            .flat_map(|root| {
                self.document.index.block_ids[root..self.subtree_end(root)]
                    .iter()
                    .copied()
            })
            .collect()
    }

    pub fn selection_request_is_current(&self, request: &SelectionMaterializationRequest) -> bool {
        request.document_id == self.document_id
            && request.structure_version == self.structure_version()
            && request.payload_window_generation == self.payload_window_generation
            && self.unified_document_selection_snapshot() == Some(request.selection.clone())
    }

    pub fn apply_selection_materialization_result(
        &mut self,
        request: &SelectionMaterializationRequest,
        records: Vec<BlockPayloadRecord>,
        missing_block_ids: &[BlockId],
    ) -> SelectionMaterializationApplyDecision {
        if !self.selection_request_is_current(request) {
            return SelectionMaterializationApplyDecision::DiscardedStale;
        }
        let expected = request.block_ids.iter().copied().collect::<HashSet<_>>();
        for record in records {
            if expected.contains(&record.block_id)
                && !self
                    .document
                    .payload_window
                    .payloads
                    .contains_key(&record.block_id)
            {
                let mut record = normalize_payload_record_for_kind(record);
                self.sync_table_runtime_from_loaded_record(&mut record);
                self.document.payload_window.insert_loaded(record);
            }
        }
        for block_id in missing_block_ids {
            if expected.contains(block_id) {
                self.document
                    .payload_window
                    .mark_failed(*block_id, "payload missing from store");
            }
        }
        SelectionMaterializationApplyDecision::Applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::edit::{SelectionEndpoint, UnifiedDocumentSelection};
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};

    #[test]
    fn request_contains_missing_payloads_for_cross_block_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=4)
                .map(|id| {
                    BlockPayloadRecord::rich_text(
                        id,
                        RichBlockKind::Paragraph,
                        format!("block-{id}"),
                    )
                })
                .collect(),
            800.0,
        );
        runtime
            .set_document_selection(DocumentSelection {
                anchor: TextPosition {
                    block_id: 1,
                    offset: 1,
                    affinity: TextAffinity::Downstream,
                },
                focus: TextPosition {
                    block_id: 4,
                    offset: 2,
                    affinity: TextAffinity::Downstream,
                },
            })
            .unwrap();
        runtime.document.payload_window.payloads.remove(&2);
        runtime.document.payload_window.payloads.remove(&3);

        let request = runtime.selection_materialization_request().unwrap();
        assert_eq!(request.block_ids, vec![2, 3]);
        assert_eq!(request.structure_version, runtime.structure_version());
        assert_eq!(
            request.selection,
            UnifiedDocumentSelection {
                anchor: SelectionEndpoint::Text(TextPosition {
                    block_id: 1,
                    offset: 1,
                    affinity: TextAffinity::Downstream,
                }),
                focus: SelectionEndpoint::Text(TextPosition {
                    block_id: 4,
                    offset: 2,
                    affinity: TextAffinity::Downstream,
                }),
            }
        );
    }

    #[test]
    fn request_becomes_stale_when_selection_changes() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "one",
            )],
            800.0,
        );
        runtime.focus_block_at_offset(1, 0).unwrap();
        runtime.select_all_command();
        runtime.document.payload_window.payloads.remove(&1);
        let request = runtime.selection_materialization_request().unwrap();
        runtime
            .set_document_selection(DocumentSelection::caret(TextPosition::downstream(1, 0)))
            .unwrap();
        assert!(!runtime.selection_request_is_current(&request));
    }

    #[test]
    fn apply_result_hydrates_only_current_selection_request() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "one"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "two"),
            ],
            800.0,
        );
        runtime
            .set_document_selection(DocumentSelection {
                anchor: TextPosition::downstream(1, 0),
                focus: TextPosition::downstream(2, 3),
            })
            .unwrap();
        let record = runtime.document.payload_window.payloads.remove(&2).unwrap();
        let request = runtime.selection_materialization_request().unwrap();

        assert_eq!(
            runtime.apply_selection_materialization_result(&request, vec![record.clone()], &[]),
            SelectionMaterializationApplyDecision::Applied
        );
        assert!(runtime.selection_materialization_request().is_none());

        runtime.document.payload_window.payloads.remove(&2);
        runtime
            .set_document_selection(DocumentSelection::caret(TextPosition::downstream(1, 0)))
            .unwrap();
        assert_eq!(
            runtime.apply_selection_materialization_result(&request, vec![record], &[]),
            SelectionMaterializationApplyDecision::DiscardedStale
        );
        assert!(!runtime.document.payload_window.payloads.contains_key(&2));
    }

    #[test]
    fn whole_block_request_includes_unloaded_descendants_of_selected_root() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "parent"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "child"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "survivor"),
            ],
            800.0,
        );
        runtime.document.index.parent_ids[1] = Some(1);
        runtime.document.index.depths[1] = 1;
        runtime.selected_block_ids.insert(1);
        runtime.document.payload_window.payloads.remove(&2);

        let request = runtime.selection_materialization_request().unwrap();
        assert_eq!(request.block_ids, vec![2]);
    }
}
