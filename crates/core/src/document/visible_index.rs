use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::document::DocumentIndex;
use crate::ids::BlockId;
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
        Self::with_folded_blocks(document_index, HashSet::new(), 0)
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
    let mut hidden_depth: Option<u16> = None;

    for index in 0..document_index.total_count() {
        let block_id = document_index.block_ids[index];
        let depth = document_index.depths[index];

        if let Some(hidden_parent_depth) = hidden_depth {
            if depth > hidden_parent_depth {
                continue;
            }
            hidden_depth = None;
        }

        visible_block_ids.push(block_id);

        if folded_block_ids.contains(&block_id) {
            hidden_depth = Some(depth);
        }
    }

    visible_block_ids
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
}
