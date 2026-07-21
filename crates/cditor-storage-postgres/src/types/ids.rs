use super::*;

pub type PgDocumentId = Uuid;
pub type PgBlockId = Uuid;

const DOCUMENT_ID_NAMESPACE: u128 = 0x1000_0000_0000_0000_0000_0000_0000_0000;
const BLOCK_ID_NAMESPACE: u128 = 0x2000_0000_0000_0000_0000_0000_0000_0000;
const LOW_64_BITS: u128 = u64::MAX as u128;
const NAMESPACE_MASK: u128 = !LOW_64_BITS;

pub fn pg_document_id_from_runtime(id: DocumentId) -> PgDocumentId {
    Uuid::from_u128(DOCUMENT_ID_NAMESPACE | id as u128)
}

pub fn pg_block_id_from_runtime(id: BlockId) -> PgBlockId {
    Uuid::from_u128(BLOCK_ID_NAMESPACE | id as u128)
}

pub fn runtime_document_id_from_pg(id: PgDocumentId) -> Option<DocumentId> {
    let raw = id.as_u128();
    ((raw & NAMESPACE_MASK) == DOCUMENT_ID_NAMESPACE).then_some((raw & LOW_64_BITS) as u64)
}

pub fn runtime_block_id_from_pg(id: PgBlockId) -> Option<BlockId> {
    let raw = id.as_u128();
    ((raw & NAMESPACE_MASK) == BLOCK_ID_NAMESPACE).then_some((raw & LOW_64_BITS) as u64)
}
