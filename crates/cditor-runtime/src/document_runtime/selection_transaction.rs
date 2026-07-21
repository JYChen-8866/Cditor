use cditor_core::edit::NormalizedSelection;

use super::*;

pub(super) struct CrossBlockReplacementPlan {
    pub(super) selection: NormalizedSelection,
    pub(super) replacement_index: usize,
    deleted_records: Vec<BlockIndexRecord>,
    deleted_payloads: Vec<BlockPayloadRecord>,
    preserved_records: Vec<BlockIndexRecord>,
    preserved_payloads: Vec<BlockPayloadRecord>,
}

impl CrossBlockReplacementPlan {
    pub(super) fn structure_operations(
        &self,
        mut replacement_records: Vec<BlockIndexRecord>,
        mut replacement_payloads: Vec<BlockPayloadRecord>,
    ) -> Result<(Vec<EditOperation>, Vec<EditOperation>), String> {
        if replacement_records.len() != replacement_payloads.len() {
            return Err("cross-block replacement record/payload count mismatch".to_owned());
        }
        replacement_records.extend(self.preserved_records.clone());
        replacement_payloads.extend(self.preserved_payloads.clone());
        let replacement_len = replacement_records.len();
        let mut ops = vec![EditOperation::DeleteBlockRange {
            range: self.replacement_index..self.replacement_index + self.deleted_records.len(),
        }];
        if replacement_len > 0 {
            ops.push(EditOperation::InsertBlocks {
                index: self.replacement_index,
                blocks: replacement_records,
                payloads: replacement_payloads,
            });
        }
        let mut inverse_ops = Vec::with_capacity(2);
        if replacement_len > 0 {
            inverse_ops.push(EditOperation::DeleteBlockRange {
                range: self.replacement_index..self.replacement_index + replacement_len,
            });
        }
        inverse_ops.push(EditOperation::InsertBlocks {
            index: self.replacement_index,
            blocks: self.deleted_records.clone(),
            payloads: self.deleted_payloads.clone(),
        });
        Ok((ops, inverse_ops))
    }
}

impl DocumentRuntime {
    pub(super) fn plan_cross_block_replacement(
        &self,
        selection: DocumentSelection,
    ) -> Result<CrossBlockReplacementPlan, String> {
        let selection = selection
            .normalize(&self.index)
            .map_err(|error| format!("{error:?}"))?;
        let start = self
            .index
            .index_of(selection.start.block_id)
            .ok_or_else(|| "selection start block is missing".to_owned())?;
        let end = self
            .index
            .index_of(selection.end.block_id)
            .ok_or_else(|| "selection end block is missing".to_owned())?;
        if start >= end {
            return Err("cross-block replacement requires distinct ordered blocks".to_owned());
        }

        let records = self.index_records();
        let replacement_index = start + 1;
        let semantic_end = end + 1;
        let mut complete_end = semantic_end;
        let mut cursor = replacement_index;
        while cursor < complete_end {
            complete_end = complete_end.max(super::transaction_apply_structure::subtree_end(
                &records, cursor,
            ));
            cursor += 1;
        }
        let deleted_records = records[replacement_index..complete_end].to_vec();
        let deleted_payloads = deleted_records
            .iter()
            .map(|record| {
                self.payload_window
                    .get(record.id)
                    .cloned()
                    .ok_or_else(|| format!("selected block payload {} is not hydrated", record.id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let preserved_records = reparent_preserved_tail(
            &records[semantic_end..complete_end],
            records[start].parent_id,
            records[start].depth,
        );
        let preserved_payloads = preserved_records
            .iter()
            .map(|record| {
                self.payload_window
                    .get(record.id)
                    .cloned()
                    .ok_or_else(|| format!("preserved block payload {} is not hydrated", record.id))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CrossBlockReplacementPlan {
            selection,
            replacement_index,
            deleted_records,
            deleted_payloads,
            preserved_records,
            preserved_payloads,
        })
    }
}

fn reparent_preserved_tail(
    records: &[BlockIndexRecord],
    parent_id: Option<BlockId>,
    depth: u16,
) -> Vec<BlockIndexRecord> {
    let ids = records
        .iter()
        .map(|record| record.id)
        .collect::<HashSet<_>>();
    let mut root_depth = None;
    records
        .iter()
        .copied()
        .map(|mut record| {
            if record.parent_id.is_none_or(|parent| !ids.contains(&parent)) {
                root_depth = Some(record.depth);
                record.parent_id = parent_id;
                record.depth = depth;
            } else if let Some(root_depth) = root_depth {
                record.depth = depth.saturating_add(record.depth.saturating_sub(root_depth));
            }
            record
        })
        .collect()
}
