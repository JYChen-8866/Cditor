use cditor_core::block::{BlockChromeSnapshot, BlockListInfo, is_numbered_list_item_kind};
use cditor_core::document::DocumentIndex;
use cditor_core::rich_text::{RichBlockKind, rich_block_kind_from_tag};
use cditor_core::version::StructureVersion;

#[derive(Debug, Clone, PartialEq)]
pub struct ListProjectionCache {
    structure_version: StructureVersion,
    entries: Vec<BlockListProjectionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockListProjectionEntry {
    pub list_info: BlockListInfo,
    pub chrome: BlockChromeSnapshot,
}

impl ListProjectionCache {
    pub fn build(index: &DocumentIndex) -> Self {
        let mut entries = Vec::with_capacity(index.block_ids.len());
        let mut numbered_counters_by_depth: Vec<usize> = Vec::new();
        for block_index in 0..index.block_ids.len() {
            let kind = rich_block_kind_from_tag(index.kind_tags[block_index]);
            let depth = index.depths[block_index] as usize;
            if numbered_counters_by_depth.len() <= depth {
                numbered_counters_by_depth.resize(depth + 1, 0);
            }
            numbered_counters_by_depth.truncate(depth + 1);

            let numbered_ordinal = if is_numbered_list_item_kind(&kind) {
                numbered_counters_by_depth[depth] =
                    numbered_counters_by_depth[depth].saturating_add(1);
                Some(numbered_counters_by_depth[depth])
            } else {
                numbered_counters_by_depth[depth] = 0;
                None
            };
            let list_info = BlockListInfo {
                depth,
                numbered_ordinal,
            };
            let has_children = next_block_is_child(index, block_index, depth);
            let collapsed = false;
            let chrome = BlockChromeSnapshot::from_kind(&kind, list_info, has_children, collapsed);
            entries.push(BlockListProjectionEntry { list_info, chrome });
        }
        Self {
            structure_version: index.structure_version,
            entries,
        }
    }

    pub fn structure_version(&self) -> StructureVersion {
        self.structure_version
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, block_index: usize) -> Option<&BlockListProjectionEntry> {
        self.entries.get(block_index)
    }

    pub fn is_current_for(&self, index: &DocumentIndex) -> bool {
        self.structure_version == index.structure_version
            && self.entries.len() == index.block_ids.len()
    }
}

pub fn project_block_list_entry(
    index: &DocumentIndex,
    cache: &ListProjectionCache,
    block_index: usize,
) -> BlockListProjectionEntry {
    if cache.is_current_for(index)
        && let Some(entry) = cache.entry(block_index)
    {
        return entry.clone();
    }
    let rebuilt = ListProjectionCache::build(index);
    rebuilt
        .entry(block_index)
        .cloned()
        .unwrap_or_else(|| fallback_entry(index, block_index))
}

fn fallback_entry(index: &DocumentIndex, block_index: usize) -> BlockListProjectionEntry {
    let kind = index
        .kind_tags
        .get(block_index)
        .map(|tag| rich_block_kind_from_tag(*tag))
        .unwrap_or(RichBlockKind::Paragraph);
    let list_info = BlockListInfo::with_depth(
        index.depths.get(block_index).copied().unwrap_or_default() as usize,
    );
    let chrome = BlockChromeSnapshot::from_kind(&kind, list_info, false, false);
    BlockListProjectionEntry { list_info, chrome }
}

fn next_block_is_child(index: &DocumentIndex, block_index: usize, depth: usize) -> bool {
    index
        .depths
        .get(block_index + 1)
        .is_some_and(|next_depth| *next_depth as usize > depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::ids::DocumentId;
    use cditor_core::rich_text::{RichBlockKind, kind_tag_for_rich_block_kind};

    fn index_for(kinds_and_depths: Vec<(RichBlockKind, u16)>) -> DocumentIndex {
        let records = kinds_and_depths
            .into_iter()
            .enumerate()
            .map(|(i, (kind, depth))| {
                BlockIndexRecord::new(
                    (i + 1) as u64,
                    None,
                    depth,
                    kind_tag_for_rich_block_kind(&kind),
                    0,
                )
            })
            .collect::<Vec<_>>();
        DocumentIndex::new(1 as DocumentId, records, 1).unwrap()
    }

    #[test]
    fn numbered_ordinals_increment_for_contiguous_same_depth_items() {
        let index = index_for(vec![
            (RichBlockKind::NumberedList, 0),
            (RichBlockKind::NumberedList, 0),
            (RichBlockKind::NumberedList, 0),
        ]);
        let cache = ListProjectionCache::build(&index);

        assert_eq!(cache.entry(0).unwrap().list_info.numbered_ordinal, Some(1));
        assert_eq!(cache.entry(1).unwrap().list_info.numbered_ordinal, Some(2));
        assert_eq!(cache.entry(2).unwrap().list_info.numbered_ordinal, Some(3));
    }

    #[test]
    fn numbered_ordinals_restart_after_non_numbered_at_same_depth() {
        let index = index_for(vec![
            (RichBlockKind::NumberedList, 0),
            (RichBlockKind::BulletedList, 0),
            (RichBlockKind::NumberedList, 0),
            (RichBlockKind::Todo { checked: false }, 0),
            (RichBlockKind::NumberedList, 0),
        ]);
        let cache = ListProjectionCache::build(&index);

        assert_eq!(cache.entry(0).unwrap().list_info.numbered_ordinal, Some(1));
        assert_eq!(cache.entry(2).unwrap().list_info.numbered_ordinal, Some(1));
        assert_eq!(cache.entry(4).unwrap().list_info.numbered_ordinal, Some(1));
    }

    #[test]
    fn nested_numbered_ordinals_are_independent_by_depth() {
        let index = index_for(vec![
            (RichBlockKind::NumberedList, 0),
            (RichBlockKind::NumberedList, 1),
            (RichBlockKind::NumberedList, 1),
            (RichBlockKind::NumberedList, 0),
        ]);
        let cache = ListProjectionCache::build(&index);

        assert_eq!(cache.entry(0).unwrap().list_info.numbered_ordinal, Some(1));
        assert_eq!(cache.entry(1).unwrap().list_info.numbered_ordinal, Some(1));
        assert_eq!(cache.entry(2).unwrap().list_info.numbered_ordinal, Some(2));
        assert_eq!(cache.entry(3).unwrap().list_info.numbered_ordinal, Some(2));
        assert!(cache.entry(0).unwrap().chrome.has_children);
    }
}
