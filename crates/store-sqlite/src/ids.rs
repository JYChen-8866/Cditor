use cditor_core::ids::{BlockId, DocumentId};
use uuid::Uuid;

const DOCUMENT_ID_NAMESPACE: u128 = 0x1000_0000_0000_0000_0000_0000_0000_0000;
const BLOCK_ID_NAMESPACE: u128 = 0x2000_0000_0000_0000_0000_0000_0000_0000;
const LOW_64_BITS: u128 = u64::MAX as u128;
const NAMESPACE_MASK: u128 = !LOW_64_BITS;

pub(crate) fn document_id_to_sqlite(id: DocumentId) -> Uuid {
    Uuid::from_u128(DOCUMENT_ID_NAMESPACE | id as u128)
}

pub(crate) fn block_id_to_sqlite(id: BlockId) -> Uuid {
    Uuid::from_u128(BLOCK_ID_NAMESPACE | id as u128)
}

pub(crate) fn document_id_from_sqlite(id: Uuid) -> Option<DocumentId> {
    let raw = id.as_u128();
    ((raw & NAMESPACE_MASK) == DOCUMENT_ID_NAMESPACE).then_some((raw & LOW_64_BITS) as u64)
}

pub(crate) fn block_id_from_sqlite(id: Uuid) -> Option<BlockId> {
    let raw = id.as_u128();
    ((raw & NAMESPACE_MASK) == BLOCK_ID_NAMESPACE).then_some((raw & LOW_64_BITS) as u64)
}
