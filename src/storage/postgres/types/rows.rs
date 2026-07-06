use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRow {
    pub id: PgDocumentId,
    pub workspace_id: Uuid,
    pub title: String,
    pub structure_version: i64,
    pub content_version: i64,
    pub layout_version: i64,
    pub schema_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRow {
    pub id: PgBlockId,
    pub document_id: PgDocumentId,
    pub parent_id: Option<PgBlockId>,
    pub prev_id: Option<PgBlockId>,
    pub next_id: Option<PgBlockId>,
    pub sort_key: String,
    pub depth: i32,
    pub kind: String,
    pub flags: i32,
    pub content_version: i64,
    pub structure_version: i64,
    pub attrs_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPayloadRow {
    pub block_id: PgBlockId,
    pub document_id: PgDocumentId,
    pub payload_format: String,
    pub payload_json: Option<serde_json::Value>,
    pub plain_text: String,
    pub content_hash: Option<String>,
    pub content_version: i64,
    pub byte_len: i64,
    pub inline_run_count: i32,
}
