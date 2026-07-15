use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::ids::{BlockId, DocumentId};
use crate::layout::BlockLayoutMeta;
use crate::version::StructureVersion;
use serde::{Deserialize, Serialize};

pub type BlockKindTag = u16;
pub type BlockFlags = u32;

pub const BLOCK_FLAG_FOLDED: BlockFlags = 1 << 0;
pub const BLOCK_FLAG_LOCKED: BlockFlags = 1 << 1;
pub const BLOCK_FLAG_HAS_STRUCTURAL_CHILDREN: BlockFlags = 1 << 2;

pub trait DocumentIndexStore {
    fn load_document_index_records(&self, document_id: DocumentId) -> Vec<BlockIndexRecord>;
    fn document_structure_version(&self, document_id: DocumentId) -> StructureVersion;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BlockIndexRecord {
    pub id: BlockId,
    pub parent_id: Option<BlockId>,
    pub depth: u16,
    pub kind_tag: BlockKindTag,
    pub flags: BlockFlags,
    pub layout_meta: BlockLayoutMeta,
}

impl BlockIndexRecord {
    pub fn new(
        id: BlockId,
        parent_id: Option<BlockId>,
        depth: u16,
        kind_tag: BlockKindTag,
        flags: BlockFlags,
    ) -> Self {
        Self {
            id,
            parent_id,
            depth,
            kind_tag,
            flags,
            layout_meta: BlockLayoutMeta::new(id, BlockLayoutMeta::DEFAULT_ESTIMATED_HEIGHT),
        }
    }

    pub fn with_layout_meta(mut self, layout_meta: BlockLayoutMeta) -> Self {
        self.layout_meta = layout_meta;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentIndex {
    pub document_id: DocumentId,
    pub block_ids: Vec<BlockId>,
    pub parent_ids: Vec<Option<BlockId>>,
    pub depths: Vec<u16>,
    pub kind_tags: Vec<BlockKindTag>,
    pub flags: Vec<BlockFlags>,
    pub layout_meta: Vec<BlockLayoutMeta>,
    pub id_to_index: HashMap<BlockId, usize>,
    pub structure_version: StructureVersion,
}

impl DocumentIndex {
    pub fn new(
        document_id: DocumentId,
        records: impl IntoIterator<Item = BlockIndexRecord>,
        structure_version: StructureVersion,
    ) -> Result<Self, DocumentIndexBuildError> {
        let records = records.into_iter();
        let (lower_bound, _) = records.size_hint();

        let mut block_ids = Vec::with_capacity(lower_bound);
        let mut parent_ids = Vec::with_capacity(lower_bound);
        let mut depths = Vec::with_capacity(lower_bound);
        let mut kind_tags = Vec::with_capacity(lower_bound);
        let mut flags = Vec::with_capacity(lower_bound);
        let mut layout_meta = Vec::with_capacity(lower_bound);
        let mut id_to_index = HashMap::with_capacity(lower_bound);

        for record in records {
            let index = block_ids.len();
            if id_to_index.insert(record.id, index).is_some() {
                return Err(DocumentIndexBuildError::DuplicateBlockId(record.id));
            }
            if record.layout_meta.block_id != record.id {
                return Err(DocumentIndexBuildError::LayoutMetaBlockIdMismatch {
                    record_id: record.id,
                    meta_block_id: record.layout_meta.block_id,
                });
            }

            block_ids.push(record.id);
            parent_ids.push(record.parent_id);
            depths.push(record.depth);
            kind_tags.push(record.kind_tag);
            flags.push(record.flags);
            layout_meta.push(record.layout_meta);
        }

        Ok(Self {
            document_id,
            block_ids,
            parent_ids,
            depths,
            kind_tags,
            flags,
            layout_meta,
            id_to_index,
            structure_version,
        })
    }

    pub fn from_store(
        document_id: DocumentId,
        store: &impl DocumentIndexStore,
    ) -> Result<Self, DocumentIndexBuildError> {
        Self::new(
            document_id,
            store.load_document_index_records(document_id),
            store.document_structure_version(document_id),
        )
    }

    pub fn total_count(&self) -> usize {
        self.block_ids.len()
    }

    pub fn id_at(&self, index: usize) -> Option<BlockId> {
        self.block_ids.get(index).copied()
    }

    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.id_to_index.get(&id).copied()
    }

    pub fn compare_position(&self, a: BlockId, b: BlockId) -> Option<Ordering> {
        let a_index = self.index_of(a)?;
        let b_index = self.index_of(b)?;
        Some(a_index.cmp(&b_index))
    }

    pub fn parent_id_at(&self, index: usize) -> Option<Option<BlockId>> {
        self.parent_ids.get(index).copied()
    }

    pub fn depth_at(&self, index: usize) -> Option<u16> {
        self.depths.get(index).copied()
    }

    pub fn kind_tag_at(&self, index: usize) -> Option<BlockKindTag> {
        self.kind_tags.get(index).copied()
    }

    pub fn flags_at(&self, index: usize) -> Option<BlockFlags> {
        self.flags.get(index).copied()
    }

    pub fn meta_at(&self, index: usize) -> Option<&BlockLayoutMeta> {
        self.layout_meta.get(index)
    }

    pub fn update_height(
        &mut self,
        index: usize,
        height: f64,
    ) -> Result<(), DocumentIndexUpdateError> {
        let Some(meta) = self.layout_meta.get_mut(index) else {
            return Err(DocumentIndexUpdateError::IndexOutOfBounds {
                index,
                len: self.layout_meta.len(),
            });
        };
        meta.update_height(height);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentIndexBuildError {
    DuplicateBlockId(BlockId),
    LayoutMetaBlockIdMismatch {
        record_id: BlockId,
        meta_block_id: BlockId,
    },
}

impl Display for DocumentIndexBuildError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateBlockId(block_id) => {
                write!(
                    formatter,
                    "duplicate block id in document index: {block_id}"
                )
            }
            Self::LayoutMetaBlockIdMismatch {
                record_id,
                meta_block_id,
            } => write!(
                formatter,
                "block index record {record_id} has layout meta for block {meta_block_id}"
            ),
        }
    }
}

impl Error for DocumentIndexBuildError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentIndexUpdateError {
    IndexOutOfBounds { index: usize, len: usize },
}

impl Display for DocumentIndexUpdateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexOutOfBounds { index, len } => {
                write!(
                    formatter,
                    "document index update out of bounds: index {index}, len {len}"
                )
            }
        }
    }
}

impl Error for DocumentIndexUpdateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const DOCUMENT_ID: DocumentId = 1;
    const PARAGRAPH: BlockKindTag = 1;
    const HEADING: BlockKindTag = 2;

    #[derive(Debug, Clone)]
    struct MockStore {
        structure_version: StructureVersion,
        records: Vec<BlockIndexRecord>,
    }

    impl DocumentIndexStore for MockStore {
        fn load_document_index_records(&self, _document_id: DocumentId) -> Vec<BlockIndexRecord> {
            self.records.clone()
        }

        fn document_structure_version(&self, _document_id: DocumentId) -> StructureVersion {
            self.structure_version
        }
    }

    #[test]
    fn builds_sequential_block_index() {
        let records = (0..10).map(|index| BlockIndexRecord::new(index + 1, None, 0, PARAGRAPH, 0));

        let index = DocumentIndex::new(DOCUMENT_ID, records, 7).unwrap();

        assert_eq!(index.document_id, DOCUMENT_ID);
        assert_eq!(index.structure_version, 7);
        assert_eq!(index.total_count(), 10);
        for block_index in 0..index.total_count() {
            let block_id = index.id_at(block_index).unwrap();
            assert_eq!(index.index_of(block_id), Some(block_index));
            assert_eq!(index.depth_at(block_index), Some(0));
            assert_eq!(index.kind_tag_at(block_index), Some(PARAGRAPH));
            assert_eq!(index.meta_at(block_index).unwrap().block_id, block_id);
        }
        assert_eq!(index.compare_position(1, 10), Some(Ordering::Less));
        assert_eq!(index.compare_position(6, 6), Some(Ordering::Equal));
        assert_eq!(index.compare_position(10, 1), Some(Ordering::Greater));
        assert_eq!(index.compare_position(10, 999), None);
    }

    #[test]
    fn builds_nested_block_index_without_payload_hydration() {
        let records = vec![
            BlockIndexRecord::new(1, None, 0, HEADING, 0),
            BlockIndexRecord::new(2, Some(1), 1, PARAGRAPH, 0),
            BlockIndexRecord::new(3, Some(1), 1, PARAGRAPH, 0),
            BlockIndexRecord::new(4, Some(3), 2, PARAGRAPH, 0),
            BlockIndexRecord::new(5, None, 0, HEADING, 0),
        ];

        let store = MockStore {
            structure_version: 42,
            records,
        };

        let index = DocumentIndex::from_store(DOCUMENT_ID, &store).unwrap();

        assert_eq!(index.structure_version, 42);
        assert_eq!(index.parent_id_at(0), Some(None));
        assert_eq!(index.parent_id_at(1), Some(Some(1)));
        assert_eq!(index.parent_id_at(3), Some(Some(3)));
        assert_eq!(index.depth_at(3), Some(2));
        assert_eq!(index.compare_position(4, 5), Some(Ordering::Less));
    }

    #[test]
    fn updates_layout_height_meta_without_payload() {
        let records = vec![BlockIndexRecord::new(1, None, 0, PARAGRAPH, 0)];
        let mut index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();

        index.update_height(0, 48.5).unwrap();

        let meta = index.meta_at(0).unwrap();
        assert_eq!(meta.measured_height, Some(48.5));
        assert!(!meta.dirty);
        assert_eq!(meta.effective_height(), 48.5);
    }

    #[test]
    fn rejects_duplicate_block_ids() {
        let records = vec![
            BlockIndexRecord::new(1, None, 0, PARAGRAPH, 0),
            BlockIndexRecord::new(1, None, 0, PARAGRAPH, 0),
        ];

        assert_eq!(
            DocumentIndex::new(DOCUMENT_ID, records, 1),
            Err(DocumentIndexBuildError::DuplicateBlockId(1))
        );
    }

    #[test]
    fn rejects_layout_meta_mismatch() {
        let records = vec![
            BlockIndexRecord::new(1, None, 0, PARAGRAPH, 0)
                .with_layout_meta(BlockLayoutMeta::new(2, 24.0)),
        ];

        assert_eq!(
            DocumentIndex::new(DOCUMENT_ID, records, 1),
            Err(DocumentIndexBuildError::LayoutMetaBlockIdMismatch {
                record_id: 1,
                meta_block_id: 2,
            })
        );
    }

    #[test]
    fn random_insert_delete_move_rebuild_keeps_id_mapping_correct() {
        let mut ids: Vec<BlockId> = (1..=2_000).collect();
        let mut rng = Lcg::new(0xC_D1_70_2);
        let mut next_id = 2_001;

        for _ in 0..1_000 {
            match rng.next_usize(3) {
                0 => {
                    let at = rng.next_usize(ids.len() + 1);
                    ids.insert(at, next_id);
                    next_id += 1;
                }
                1 if !ids.is_empty() => {
                    let at = rng.next_usize(ids.len());
                    ids.remove(at);
                }
                _ if ids.len() > 1 => {
                    let from = rng.next_usize(ids.len());
                    let id = ids.remove(from);
                    let to = rng.next_usize(ids.len() + 1);
                    ids.insert(to, id);
                }
                _ => {}
            }

            let records = ids
                .iter()
                .copied()
                .map(|id| BlockIndexRecord::new(id, None, 0, PARAGRAPH, 0));
            let index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
            assert_index_mapping(&index, &ids);
        }
    }

    #[test]
    fn builds_100k_blocks_within_acceptable_budget() {
        let records =
            (0..100_000).map(|index| BlockIndexRecord::new(index + 1, None, 0, PARAGRAPH, 0));

        let started = Instant::now();
        let index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(index.total_count(), 100_000);
        assert!(
            elapsed.as_millis() < 800,
            "DocumentIndex 100k build took {elapsed:?}, expected < 800ms acceptable budget"
        );
    }

    fn assert_index_mapping(index: &DocumentIndex, ids: &[BlockId]) {
        assert_eq!(index.total_count(), ids.len());
        for (position, block_id) in ids.iter().copied().enumerate() {
            assert_eq!(index.id_at(position), Some(block_id));
            assert_eq!(index.index_of(block_id), Some(position));
        }

        if ids.len() >= 2 {
            let first = ids[0];
            let last = ids[ids.len() - 1];
            assert_eq!(index.compare_position(first, last), Some(Ordering::Less));
            assert_eq!(index.compare_position(last, first), Some(Ordering::Greater));
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct Lcg(u64);

    impl Lcg {
        const fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }

        fn next_usize(&mut self, upper_bound: usize) -> usize {
            if upper_bound == 0 {
                0
            } else {
                (self.next_u64() as usize) % upper_bound
            }
        }
    }
}
