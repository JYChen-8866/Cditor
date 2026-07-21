CREATE TABLE undo_blobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id BLOB NOT NULL,
    snapshot_id INTEGER NOT NULL,
    block_count INTEGER NOT NULL,
    codec TEXT NOT NULL DEFAULT 'operation-envelope-json-v1',
    payload BLOB NOT NULL,
    checksum TEXT NOT NULL,
    encoded_len INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    last_accessed_at INTEGER NOT NULL,
    UNIQUE(document_id, snapshot_id)
);

CREATE INDEX idx_undo_blobs_document_access
    ON undo_blobs(document_id, last_accessed_at DESC, id DESC);
