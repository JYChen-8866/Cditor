use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::document::{BLOCK_FLAG_FOLDED, DocumentIndex};
use crate::ids::BlockId;
use crate::rich_text::{RichBlockKind, rich_block_kind_from_tag};
use crate::version::StructureVersion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleDocumentIndex {
    pub visible_block_ids: Vec<BlockId>,
    pub source_structure_version: StructureVersion,
    pub visibility_version: u64,
    pub id_to_visible_index: HashMap<BlockId, usize>,
    folded_block_ids: HashSet<BlockId>,
}

impl VisibleDocumentIndex {
    pub fn from_document_index(document_index: &DocumentIndex) -> Self {
        let folded_block_ids = document_index
            .block_ids
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, block_id)| {
                (document_index.flags[index] & BLOCK_FLAG_FOLDED != 0).then_some(block_id)
            })
            .collect();
        Self::with_folded_blocks(document_index, folded_block_ids, 0)
    }

    pub fn with_folded_blocks(
        document_index: &DocumentIndex,
        folded_block_ids: HashSet<BlockId>,
        visibility_version: u64,
    ) -> Self {
        let visible_block_ids = build_visible_block_ids(document_index, &folded_block_ids);
        let id_to_visible_index = build_visible_lookup(&visible_block_ids);

        Self {
            visible_block_ids,
            source_structure_version: document_index.structure_version,
            visibility_version,
            id_to_visible_index,
            folded_block_ids,
        }
    }

    pub fn total_visible_count(&self) -> usize {
        self.visible_block_ids.len()
    }

    pub fn id_at_visible_index(&self, visible_index: usize) -> Option<BlockId> {
        self.visible_block_ids.get(visible_index).copied()
    }

    pub fn visible_index_of(&self, block_id: BlockId) -> Option<usize> {
        self.id_to_visible_index.get(&block_id).copied()
    }

    pub fn is_visible(&self, block_id: BlockId) -> bool {
        self.id_to_visible_index.contains_key(&block_id)
    }

    pub fn is_folded(&self, block_id: BlockId) -> bool {
        self.folded_block_ids.contains(&block_id)
    }

    pub fn has_foldable_content(&self, document_index: &DocumentIndex, block_id: BlockId) -> bool {
        document_index
            .index_of(block_id)
            .is_some_and(|index| fold_end(document_index, index) > index + 1)
    }

    pub fn fold_end_index(
        &self,
        document_index: &DocumentIndex,
        block_id: BlockId,
    ) -> Option<usize> {
        document_index
            .index_of(block_id)
            .map(|index| fold_end(document_index, index))
    }

    pub fn folded_block_ids(&self) -> &HashSet<BlockId> {
        &self.folded_block_ids
    }

    pub fn toggle_folded(
        &mut self,
        document_index: &DocumentIndex,
        block_id: BlockId,
    ) -> Result<VisibilityUpdate, VisibleDocumentIndexError> {
        if document_index.index_of(block_id).is_none() {
            return Err(VisibleDocumentIndexError::UnknownBlock(block_id));
        }

        let mut next_folded = self.folded_block_ids.clone();
        let folded = if next_folded.remove(&block_id) {
            false
        } else {
            next_folded.insert(block_id);
            true
        };

        let changed = self.apply_folded_blocks(document_index, next_folded)?;
        Ok(VisibilityUpdate {
            visibility_version: self.visibility_version,
            changed,
            folded: Some((block_id, folded)),
        })
    }

    pub fn apply_folded_blocks(
        &mut self,
        document_index: &DocumentIndex,
        folded_block_ids: HashSet<BlockId>,
    ) -> Result<VisibilityChange, VisibleDocumentIndexError> {
        if document_index.structure_version != self.source_structure_version {
            return Err(VisibleDocumentIndexError::StructureVersionMismatch {
                expected: self.source_structure_version,
                actual: document_index.structure_version,
            });
        }

        for block_id in &folded_block_ids {
            if document_index.index_of(*block_id).is_none() {
                return Err(VisibleDocumentIndexError::UnknownBlock(*block_id));
            }
        }

        let before_count = self.visible_block_ids.len();
        self.folded_block_ids = folded_block_ids;
        self.visible_block_ids = build_visible_block_ids(document_index, &self.folded_block_ids);
        self.id_to_visible_index = build_visible_lookup(&self.visible_block_ids);
        self.visibility_version = self.visibility_version.saturating_add(1);

        Ok(VisibilityChange {
            before_count,
            after_count: self.visible_block_ids.len(),
        })
    }

    pub fn rebuild_for_structure(&mut self, document_index: &DocumentIndex) -> VisibilityChange {
        let folded_block_ids = self
            .folded_block_ids
            .iter()
            .copied()
            .filter(|block_id| document_index.index_of(*block_id).is_some())
            .collect();

        let before_count = self.visible_block_ids.len();
        self.source_structure_version = document_index.structure_version;
        self.folded_block_ids = folded_block_ids;
        self.visible_block_ids = build_visible_block_ids(document_index, &self.folded_block_ids);
        self.id_to_visible_index = build_visible_lookup(&self.visible_block_ids);
        self.visibility_version = self.visibility_version.saturating_add(1);

        VisibilityChange {
            before_count,
            after_count: self.visible_block_ids.len(),
        }
    }

    pub fn resolve_scroll_target(
        &self,
        document_index: &DocumentIndex,
        block_id: BlockId,
    ) -> Option<VisibleScrollTarget> {
        if let Some(visible_index) = self.visible_index_of(block_id) {
            return Some(VisibleScrollTarget {
                requested_block_id: block_id,
                target_block_id: block_id,
                visible_index,
                resolved_by: ScrollTargetResolution::ExactVisible,
            });
        }

        if let Some(requested_index) = document_index.index_of(block_id)
            && let Some((owner_id, visible_index)) = self
                .folded_block_ids
                .iter()
                .copied()
                .filter_map(|owner_id| {
                    let owner_index = document_index.index_of(owner_id)?;
                    let visible_index = self.visible_index_of(owner_id)?;
                    (owner_index < requested_index
                        && requested_index < fold_end(document_index, owner_index))
                    .then_some((owner_index, owner_id, visible_index))
                })
                .max_by_key(|(owner_index, _, _)| *owner_index)
                .map(|(_, owner_id, visible_index)| (owner_id, visible_index))
        {
            return Some(VisibleScrollTarget {
                requested_block_id: block_id,
                target_block_id: owner_id,
                visible_index,
                resolved_by: ScrollTargetResolution::NearestVisibleAncestor,
            });
        }

        let mut current = block_id;
        while let Some(index) = document_index.index_of(current) {
            let parent_id = document_index.parent_id_at(index).flatten()?;
            if let Some(visible_index) = self.visible_index_of(parent_id) {
                return Some(VisibleScrollTarget {
                    requested_block_id: block_id,
                    target_block_id: parent_id,
                    visible_index,
                    resolved_by: ScrollTargetResolution::NearestVisibleAncestor,
                });
            }
            current = parent_id;
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityChange {
    pub before_count: usize,
    pub after_count: usize,
}

impl VisibilityChange {
    pub fn delta(&self) -> isize {
        self.after_count as isize - self.before_count as isize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityUpdate {
    pub visibility_version: u64,
    pub changed: VisibilityChange,
    pub folded: Option<(BlockId, bool)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleScrollTarget {
    pub requested_block_id: BlockId,
    pub target_block_id: BlockId,
    pub visible_index: usize,
    pub resolved_by: ScrollTargetResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTargetResolution {
    ExactVisible,
    NearestVisibleAncestor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleDocumentIndexError {
    UnknownBlock(BlockId),
    StructureVersionMismatch {
        expected: StructureVersion,
        actual: StructureVersion,
    },
}

impl Display for VisibleDocumentIndexError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBlock(block_id) => {
                write!(formatter, "unknown block in visible index: {block_id}")
            }
            Self::StructureVersionMismatch { expected, actual } => write!(
                formatter,
                "visible index source structure version mismatch: expected {expected}, actual {actual}"
            ),
        }
    }
}

impl Error for VisibleDocumentIndexError {}

fn build_visible_block_ids(
    document_index: &DocumentIndex,
    folded_block_ids: &HashSet<BlockId>,
) -> Vec<BlockId> {
    let mut visible_block_ids = Vec::with_capacity(document_index.total_count());
    let mut hidden_until = 0usize;

    for index in 0..document_index.total_count() {
        if index < hidden_until {
            continue;
        }
        let block_id = document_index.block_ids[index];

        visible_block_ids.push(block_id);

        if folded_block_ids.contains(&block_id) {
            hidden_until = fold_end(document_index, index);
        }
    }

    visible_block_ids
}

fn fold_end(document_index: &DocumentIndex, index: usize) -> usize {
    let Some(depth) = document_index.depths.get(index).copied() else {
        return index;
    };
    let kind = document_index
        .kind_tags
        .get(index)
        .copied()
        .map(rich_block_kind_from_tag)
        .unwrap_or(RichBlockKind::Paragraph);
    if let RichBlockKind::Heading { level } = kind {
        let mut end = index + 1;
        while end < document_index.total_count() {
            let candidate_depth = document_index.depths[end];
            if candidate_depth < depth {
                break;
            }
            if candidate_depth == depth
                && matches!(
                    rich_block_kind_from_tag(document_index.kind_tags[end]),
                    RichBlockKind::Heading { level: candidate_level } if candidate_level <= level
                )
            {
                break;
            }
            end += 1;
        }
        return end;
    }

    let mut end = index + 1;
    while end < document_index.total_count() && document_index.depths[end] > depth {
        end += 1;
    }
    end
}

fn build_visible_lookup(visible_block_ids: &[BlockId]) -> HashMap<BlockId, usize> {
    visible_block_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(index, block_id)| (block_id, index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::BlockIndexRecord;
    use crate::ids::DocumentId;
    use std::time::Instant;

    const DOCUMENT_ID: DocumentId = 1;
    const PARAGRAPH: u16 = 1;

    #[test]
    fn builds_visible_projection_from_document_index() {
        let document_index = sample_nested_document();
        let visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        assert_eq!(
            visible_index.source_structure_version,
            document_index.structure_version
        );
        assert_eq!(visible_index.visibility_version, 0);
        assert_eq!(
            visible_index.total_visible_count(),
            document_index.total_count()
        );
        for index in 0..document_index.total_count() {
            let block_id = document_index.id_at(index).unwrap();
            assert_eq!(visible_index.id_at_visible_index(index), Some(block_id));
            assert_eq!(visible_index.visible_index_of(block_id), Some(index));
        }
    }

    #[test]
    fn toggle_collapse_hides_descendant_subtree_but_keeps_parent_visible() {
        let document_index = sample_nested_document();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        let update = visible_index.toggle_folded(&document_index, 2).unwrap();

        assert_eq!(update.visibility_version, 1);
        assert_eq!(update.folded, Some((2, true)));
        assert_eq!(update.changed.before_count, 7);
        assert_eq!(update.changed.after_count, 4);
        assert_eq!(visible_index.visible_block_ids, vec![1, 2, 6, 7]);
        assert!(visible_index.is_visible(2));
        assert!(!visible_index.is_visible(3));
        assert!(!visible_index.is_visible(4));
        assert!(!visible_index.is_visible(5));

        let update = visible_index.toggle_folded(&document_index, 2).unwrap();

        assert_eq!(update.visibility_version, 2);
        assert_eq!(update.folded, Some((2, false)));
        assert_eq!(visible_index.visible_block_ids, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn heading_fold_end_points_before_the_next_same_level_heading() {
        let document_index = heading_document();
        let visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        assert_eq!(visible_index.fold_end_index(&document_index, 1), Some(5));
        assert_eq!(visible_index.fold_end_index(&document_index, 3), Some(5));
        assert_eq!(visible_index.fold_end_index(&document_index, 6), Some(7));
    }

    #[test]
    fn hidden_child_scroll_target_resolves_to_nearest_visible_ancestor() {
        let document_index = sample_nested_document();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);
        visible_index.toggle_folded(&document_index, 2).unwrap();

        let target = visible_index
            .resolve_scroll_target(&document_index, 5)
            .unwrap();

        assert_eq!(target.requested_block_id, 5);
        assert_eq!(target.target_block_id, 2);
        assert_eq!(target.visible_index, 1);
        assert_eq!(
            target.resolved_by,
            ScrollTargetResolution::NearestVisibleAncestor
        );

        let visible_target = visible_index
            .resolve_scroll_target(&document_index, 6)
            .unwrap();
        assert_eq!(visible_target.target_block_id, 6);
        assert_eq!(visible_target.visible_index, 2);
        assert_eq!(
            visible_target.resolved_by,
            ScrollTargetResolution::ExactVisible
        );
    }

    #[test]
    fn applies_batch_visibility_update_once() {
        let document_index = sample_nested_document();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);
        let folded = HashSet::from([2, 6]);

        let change = visible_index
            .apply_folded_blocks(&document_index, folded)
            .unwrap();

        assert_eq!(visible_index.visibility_version, 1);
        assert_eq!(change.before_count, 7);
        assert_eq!(change.after_count, 4);
        assert_eq!(visible_index.visible_block_ids, vec![1, 2, 6, 7]);
        assert!(visible_index.is_folded(2));
        assert!(visible_index.is_folded(6));
    }

    #[test]
    fn collapse_and_expand_10k_subtree_are_linear_not_quadratic() {
        let mut records = Vec::with_capacity(10_002);
        records.push(BlockIndexRecord::new(1, None, 0, PARAGRAPH, 0));
        for id in 2..=10_001 {
            records.push(BlockIndexRecord::new(id, Some(1), 1, PARAGRAPH, 0));
        }
        records.push(BlockIndexRecord::new(10_002, None, 0, PARAGRAPH, 0));
        let document_index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        let collapse_started = Instant::now();
        let collapse = visible_index.toggle_folded(&document_index, 1).unwrap();
        let collapse_elapsed = collapse_started.elapsed();

        assert_eq!(collapse.changed.before_count, 10_002);
        assert_eq!(collapse.changed.after_count, 2);
        assert_eq!(visible_index.visible_block_ids, vec![1, 10_002]);
        assert!(
            collapse_elapsed.as_millis() < 800,
            "10k subtree collapse took {collapse_elapsed:?}, expected linear batch update"
        );

        let expand_started = Instant::now();
        let expand = visible_index.toggle_folded(&document_index, 1).unwrap();
        let expand_elapsed = expand_started.elapsed();

        assert_eq!(expand.changed.before_count, 2);
        assert_eq!(expand.changed.after_count, 10_002);
        assert_eq!(visible_index.total_visible_count(), 10_002);
        assert!(
            expand_elapsed.as_millis() < 800,
            "10k subtree expand took {expand_elapsed:?}, expected linear batch update"
        );
    }

    #[test]
    fn folded_heading_hides_its_entire_section_by_heading_level() {
        let kinds = [
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 2 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 3 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 2 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Paragraph,
        ];
        let records = kinds.into_iter().enumerate().map(|(index, kind)| {
            BlockIndexRecord::new(
                index as BlockId + 1,
                None,
                0,
                crate::rich_text::kind_tag_for_rich_block_kind(&kind),
                0,
            )
        });
        let document_index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        assert!(visible_index.has_foldable_content(&document_index, 1));
        visible_index.toggle_folded(&document_index, 1).unwrap();

        assert_eq!(visible_index.visible_block_ids, vec![1, 9, 10]);
    }

    #[test]
    fn folded_h2_stops_at_the_next_h2_or_higher_heading() {
        let kinds = [
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Heading { level: 2 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 3 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 2 },
            RichBlockKind::Paragraph,
        ];
        let records = kinds.into_iter().enumerate().map(|(index, kind)| {
            BlockIndexRecord::new(
                index as BlockId + 1,
                None,
                0,
                crate::rich_text::kind_tag_for_rich_block_kind(&kind),
                0,
            )
        });
        let document_index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        visible_index.toggle_folded(&document_index, 2).unwrap();

        assert_eq!(visible_index.visible_block_ids, vec![1, 2, 6, 7]);
    }

    #[test]
    fn folded_flag_is_restored_when_visible_index_is_built() {
        let records = vec![
            BlockIndexRecord::new(
                1,
                None,
                0,
                crate::rich_text::kind_tag_for_rich_block_kind(&RichBlockKind::Heading {
                    level: 1,
                }),
                BLOCK_FLAG_FOLDED,
            ),
            BlockIndexRecord::new(2, None, 0, PARAGRAPH, 0),
        ];
        let document_index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let visible_index = VisibleDocumentIndex::from_document_index(&document_index);

        assert!(visible_index.is_folded(1));
        assert_eq!(visible_index.visible_block_ids, vec![1]);
    }

    #[test]
    fn hidden_heading_section_target_resolves_to_its_visible_heading() {
        let records = vec![
            BlockIndexRecord::new(
                1,
                None,
                0,
                crate::rich_text::kind_tag_for_rich_block_kind(&RichBlockKind::Heading {
                    level: 1,
                }),
                0,
            ),
            BlockIndexRecord::new(2, None, 0, PARAGRAPH, 0),
            BlockIndexRecord::new(
                3,
                None,
                0,
                crate::rich_text::kind_tag_for_rich_block_kind(&RichBlockKind::Heading {
                    level: 1,
                }),
                0,
            ),
        ];
        let document_index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let mut visible_index = VisibleDocumentIndex::from_document_index(&document_index);
        visible_index.toggle_folded(&document_index, 1).unwrap();

        let target = visible_index
            .resolve_scroll_target(&document_index, 2)
            .unwrap();

        assert_eq!(target.target_block_id, 1);
        assert_eq!(target.visible_index, 0);
        assert_eq!(
            target.resolved_by,
            ScrollTargetResolution::NearestVisibleAncestor
        );
    }

    fn sample_nested_document() -> DocumentIndex {
        let records = vec![
            BlockIndexRecord::new(1, None, 0, PARAGRAPH, 0),
            BlockIndexRecord::new(2, Some(1), 1, PARAGRAPH, 0),
            BlockIndexRecord::new(3, Some(2), 2, PARAGRAPH, 0),
            BlockIndexRecord::new(4, Some(2), 2, PARAGRAPH, 0),
            BlockIndexRecord::new(5, Some(4), 3, PARAGRAPH, 0),
            BlockIndexRecord::new(6, Some(1), 1, PARAGRAPH, 0),
            BlockIndexRecord::new(7, None, 0, PARAGRAPH, 0),
        ];
        DocumentIndex::new(DOCUMENT_ID, records, 9).unwrap()
    }

    fn heading_document() -> DocumentIndex {
        let kinds = [
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 2 },
            RichBlockKind::Paragraph,
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Paragraph,
        ];
        let records = kinds.into_iter().enumerate().map(|(index, kind)| {
            BlockIndexRecord::new(
                index as BlockId + 1,
                None,
                0,
                crate::rich_text::kind_tag_for_rich_block_kind(&kind),
                0,
            )
        });
        DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap()
    }
}
