use crate::core::document::BlockIndexRecord;
use crate::core::ids::DocumentId;
use crate::core::version::StructureVersion;

pub trait DocumentIndexStore {
    fn load_document_index_records(&self, document_id: DocumentId) -> Vec<BlockIndexRecord>;
    fn document_structure_version(&self, document_id: DocumentId) -> StructureVersion;
}
