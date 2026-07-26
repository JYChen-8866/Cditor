CREATE VIRTUAL TABLE block_fts USING fts5(
    workspace_id UNINDEXED,
    document_id UNINDEXED,
    block_id UNINDEXED,
    content_version UNINDEXED,
    plain_text,
    tokenize = 'unicode61'
);

CREATE TABLE block_fts_state (
    document_id BLOB NOT NULL,
    block_id BLOB NOT NULL,
    content_version INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, block_id),
    FOREIGN KEY(document_id, block_id)
        REFERENCES blocks(document_id, id) ON DELETE CASCADE
);

CREATE TABLE document_links (
    source_document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    source_block_id BLOB NOT NULL,
    target_document_id BLOB NOT NULL,
    target_block_id BLOB,
    link_kind TEXT NOT NULL CHECK(link_kind IN ('inline_link', 'embed')),
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(source_document_id, source_block_id)
        REFERENCES blocks(document_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_document_links_identity
ON document_links(
    source_document_id,
    source_block_id,
    target_document_id,
    COALESCE(hex(target_block_id), ''),
    link_kind
);

CREATE INDEX idx_document_links_target
ON document_links(target_document_id, target_block_id);
