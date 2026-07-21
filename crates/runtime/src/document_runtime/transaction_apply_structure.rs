//! Pure preorder staging helpers for external transaction application.

use std::collections::BTreeSet;

use super::transaction_apply::StagingState;
use super::*;

pub(super) fn position_of(records: &[BlockIndexRecord], block_id: BlockId) -> Option<usize> {
    records.iter().position(|record| record.id == block_id)
}

pub(super) fn subtree_end(records: &[BlockIndexRecord], index: usize) -> usize {
    let depth = records[index].depth;
    let mut end = index + 1;
    while end < records.len() && records[end].depth > depth {
        end += 1;
    }
    end
}

pub(super) fn ensure_complete_subtrees(
    records: &[BlockIndexRecord],
    range: Range<usize>,
) -> Result<(), String> {
    let ids: BTreeSet<BlockId> = records[range.clone()]
        .iter()
        .map(|record| record.id)
        .collect();
    for (index, record) in records.iter().enumerate() {
        if range.contains(&index) {
            continue;
        }
        if let Some(parent) = record.parent_id
            && ids.contains(&parent)
        {
            return Err(format!(
                "range {range:?} splits a subtree: block {} keeps a deleted/moved parent {parent}",
                record.id
            ));
        }
    }
    Ok(())
}

pub(super) fn stage_insert_blocks(
    staging: &mut StagingState,
    index: usize,
    blocks: &[BlockIndexRecord],
    payloads: &[BlockPayloadRecord],
) -> Result<(), String> {
    if index > staging.records.len() {
        return Err(format!(
            "insert index {index} out of bounds ({} records)",
            staging.records.len()
        ));
    }
    for block in blocks {
        if position_of(&staging.records, block.id).is_some() {
            return Err(format!("block id {} already exists", block.id));
        }
    }
    if !payloads.is_empty() && payloads.len() != blocks.len() {
        return Err(format!(
            "insert carries {} blocks but {} payloads",
            blocks.len(),
            payloads.len()
        ));
    }
    let payload_by_id = if payloads.is_empty() {
        None
    } else {
        let mut by_id = std::collections::HashMap::with_capacity(payloads.len());
        for payload in payloads {
            if by_id.insert(payload.block_id, payload).is_some() {
                return Err(format!(
                    "insert carries duplicate payload for block {}",
                    payload.block_id
                ));
            }
        }
        for block in blocks {
            let payload = by_id
                .get(&block.id)
                .ok_or_else(|| format!("insert is missing payload for block {}", block.id))?;
            if kind_tag_for_rich_block_kind(&payload.kind) != block.kind_tag {
                return Err(format!(
                    "insert block {} kind tag does not match its payload kind",
                    block.id
                ));
            }
        }
        Some(by_id)
    };
    staging.records.splice(index..index, blocks.iter().copied());
    for block in blocks {
        let payload = payload_by_id
            .as_ref()
            .and_then(|payloads| payloads.get(&block.id))
            .map(|payload| (*payload).clone())
            .unwrap_or_else(|| BlockPayloadRecord {
                block_id: block.id,
                content_version: 0,
                kind: cditor_core::rich_text::rich_block_kind_from_tag(block.kind_tag),
                payload: BlockPayload::RichText { spans: Vec::new() },
            });
        staging.payloads.entry(block.id).or_insert(payload);
        staging.touched.insert(block.id);
        staging.inserted.insert(block.id);
        staging.deleted.remove(&block.id);
    }
    staging.structure_changed = true;
    Ok(())
}

pub(super) fn stage_move_to_parent(
    staging: &mut StagingState,
    block_id: BlockId,
    parent_id: Option<BlockId>,
    sibling_index: usize,
) -> Result<(), String> {
    let start = position_of(&staging.records, block_id)
        .ok_or_else(|| format!("block {block_id} does not exist"))?;
    let end = subtree_end(&staging.records, start);
    let mut subtree: Vec<BlockIndexRecord> = staging.records.drain(start..end).collect();

    let (parent_depth, insert_at) = match parent_id {
        Some(parent) => {
            let Some(parent_position) = position_of(&staging.records, parent) else {
                staging.records.splice(start..start, subtree);
                return Err(format!(
                    "target parent {parent} does not exist or is inside the moved subtree"
                ));
            };
            let parent_depth = staging.records[parent_position].depth;
            let insert_at = child_insertion_position(
                &staging.records,
                parent_position,
                parent_depth,
                sibling_index,
            );
            (i32::from(parent_depth), insert_at)
        }
        None => (-1, root_insertion_position(&staging.records, sibling_index)),
    };

    let depth_delta = parent_depth + 1 - i32::from(subtree[0].depth);
    for record in &mut subtree {
        let new_depth = i32::from(record.depth) + depth_delta;
        if !(0..=i32::from(u16::MAX)).contains(&new_depth) {
            let restore_at = start.min(staging.records.len());
            staging.records.splice(restore_at..restore_at, subtree);
            return Err("depth overflow while moving subtree".to_owned());
        }
        record.depth = new_depth as u16;
    }
    subtree[0].parent_id = parent_id;
    let insert_at = insert_at.min(staging.records.len());
    staging.records.splice(insert_at..insert_at, subtree);
    staging.structure_changed = true;
    Ok(())
}

fn child_insertion_position(
    records: &[BlockIndexRecord],
    parent_position: usize,
    parent_depth: u16,
    sibling_index: usize,
) -> usize {
    let child_depth = parent_depth + 1;
    let mut position = parent_position + 1;
    let mut seen = 0usize;
    while position < records.len() && records[position].depth > parent_depth {
        if records[position].depth == child_depth {
            if seen == sibling_index {
                return position;
            }
            seen += 1;
            position = subtree_end(records, position);
        } else {
            position += 1;
        }
    }
    position
}

fn root_insertion_position(records: &[BlockIndexRecord], sibling_index: usize) -> usize {
    let mut position = 0usize;
    let mut seen = 0usize;
    while position < records.len() {
        if records[position].depth == 0 {
            if seen == sibling_index {
                return position;
            }
            seen += 1;
            position = subtree_end(records, position);
        } else {
            position += 1;
        }
    }
    position
}

pub(super) fn validate_preorder(records: &[BlockIndexRecord]) -> Result<(), String> {
    let mut seen: BTreeSet<BlockId> = BTreeSet::new();
    let mut ancestry: Vec<(BlockId, u16)> = Vec::new();
    for record in records {
        if !seen.insert(record.id) {
            return Err(format!("duplicate block id {}", record.id));
        }
        while ancestry
            .last()
            .is_some_and(|(_, depth)| *depth >= record.depth)
        {
            ancestry.pop();
        }
        match record.parent_id {
            None if record.depth != 0 => {
                return Err(format!(
                    "root block {} must have depth 0, got {}",
                    record.id, record.depth
                ));
            }
            None => {}
            Some(parent) => {
                let Some((ancestor, ancestor_depth)) = ancestry.last() else {
                    return Err(format!(
                        "block {} has parent {parent} but no ancestor",
                        record.id
                    ));
                };
                if *ancestor != parent || *ancestor_depth + 1 != record.depth {
                    return Err(format!(
                        "block {} parent/depth mismatch: parent {parent}, nearest ancestor {ancestor} at depth {ancestor_depth}",
                        record.id
                    ));
                }
            }
        }
        ancestry.push((record.id, record.depth));
    }
    Ok(())
}
