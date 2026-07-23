use super::transaction_apply::StagingState;
use super::transaction_apply_payload::split_payload_text;
use super::transaction_apply_structure::{position_of, subtree_end};
use super::*;

impl DocumentRuntime {
    pub(super) fn stage_split_block(
        &self,
        staging: &mut StagingState,
        block_id: BlockId,
        offset: usize,
        new_block_id: BlockId,
    ) -> Result<(), String> {
        if position_of(&staging.records, new_block_id).is_some() {
            return Err(format!("new block id {new_block_id} already exists"));
        }
        let position = position_of(&staging.records, block_id)
            .ok_or_else(|| format!("block {block_id} does not exist"))?;
        let (leading, trailing, kind) = {
            let record = self.staged_payload(staging, block_id)?;
            let (leading, trailing) = split_payload_text(record, offset)?;
            (leading, trailing, record.kind.clone())
        };
        let record = self.staged_payload(staging, block_id)?;
        record.payload = leading;

        let source = staging.records[position];
        let insert_at = subtree_end(&staging.records, position);
        let new_record = BlockIndexRecord::new(
            new_block_id,
            source.parent_id,
            source.depth,
            source.kind_tag,
            0,
        );
        staging.records.insert(insert_at, new_record);
        staging.payloads.insert(
            new_block_id,
            BlockPayloadRecord {
                block_id: new_block_id,
                content_version: 0,
                kind,
                payload: trailing,
            },
        );
        staging.touched.insert(new_block_id);
        staging.structure_changed = true;
        Ok(())
    }

    pub(super) fn stage_merge_blocks(
        &self,
        staging: &mut StagingState,
        previous: BlockId,
        current: BlockId,
    ) -> Result<(), String> {
        let current_position = position_of(&staging.records, current)
            .ok_or_else(|| format!("block {current} does not exist"))?;
        if subtree_end(&staging.records, current_position) != current_position + 1 {
            return Err(format!("block {current} has children and cannot be merged"));
        }
        let current_text = self.staged_payload(staging, current)?.plain_text();
        {
            let record = self.staged_payload(staging, previous)?;
            record.payload = append_plain_text_to_payload(record.payload.clone(), current_text);
        }
        staging.records.remove(current_position);
        staging.payloads.remove(&current);
        staging.touched.remove(&current);
        staging.deleted.insert(current);
        staging.structure_changed = true;
        Ok(())
    }
}
