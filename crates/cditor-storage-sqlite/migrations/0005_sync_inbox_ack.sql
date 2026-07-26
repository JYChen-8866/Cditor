-- P7-003 / P8 sync durability: idempotent inbound batches and local cursors.

CREATE TABLE sync_inbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id BLOB NOT NULL,
    batch_id TEXT NOT NULL,
    server_cursor TEXT NOT NULL,
    envelope_json TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    applied_at INTEGER,
    UNIQUE(document_id, batch_id)
);

CREATE INDEX idx_inbox_document_pending
    ON sync_inbox(document_id, applied_at, id);

CREATE TABLE sync_ack_cursors (
    document_id BLOB PRIMARY KEY NOT NULL,
    pushed_outbox_id INTEGER,
    pulled_cursor TEXT,
    updated_at INTEGER NOT NULL
);
