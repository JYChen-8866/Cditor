CREATE TABLE workspaces (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE documents (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    structure_version INTEGER NOT NULL DEFAULT 1,
    content_version INTEGER NOT NULL DEFAULT 1,
    layout_version INTEGER NOT NULL DEFAULT 0,
    schema_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

CREATE TABLE blocks (
    id BLOB NOT NULL,
    document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    parent_id BLOB,
    sort_key TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 0,
    kind_tag INTEGER NOT NULL,
    flags INTEGER NOT NULL DEFAULT 0,
    content_version INTEGER NOT NULL DEFAULT 1,
    structure_version INTEGER NOT NULL DEFAULT 1,
    estimated_height REAL NOT NULL DEFAULT 24,
    measured_height REAL,
    width_bucket INTEGER NOT NULL DEFAULT 0,
    layout_version INTEGER NOT NULL DEFAULT 0,
    layout_dirty INTEGER NOT NULL DEFAULT 1 CHECK(layout_dirty IN (0, 1)),
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    PRIMARY KEY(document_id, id)
);

CREATE INDEX idx_sqlite_blocks_document_sort
ON blocks(document_id, sort_key)
WHERE deleted_at IS NULL;

CREATE TABLE block_attrs (
    document_id BLOB NOT NULL,
    block_id BLOB NOT NULL,
    attrs_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, block_id),
    FOREIGN KEY(document_id, block_id) REFERENCES blocks(document_id, id) ON DELETE CASCADE
);

CREATE TABLE block_payloads (
    block_id BLOB NOT NULL,
    document_id BLOB NOT NULL,
    kind_json TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    plain_text TEXT NOT NULL DEFAULT '',
    content_version INTEGER NOT NULL DEFAULT 1,
    byte_len INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, block_id),
    FOREIGN KEY(document_id, block_id) REFERENCES blocks(document_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_sqlite_payloads_document
ON block_payloads(document_id);

CREATE TABLE block_layout (
    document_id BLOB NOT NULL,
    block_id BLOB NOT NULL,
    layout_key_hash TEXT NOT NULL,
    estimated_height REAL NOT NULL,
    measured_height REAL,
    content_version INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, block_id, layout_key_hash),
    FOREIGN KEY(document_id, block_id) REFERENCES blocks(document_id, id) ON DELETE CASCADE
);

CREATE TABLE page_layout (
    document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    structure_version INTEGER NOT NULL,
    layout_key_hash TEXT NOT NULL,
    page_policy_version INTEGER NOT NULL,
    page_index INTEGER NOT NULL,
    height REAL NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, structure_version, layout_key_hash, page_policy_version, page_index)
);

CREATE TABLE document_index_snapshot (
    document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    visible_index_version INTEGER NOT NULL,
    structure_version INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    block_count INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, visible_index_version, structure_version)
);

CREATE TABLE edit_transactions (
    document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    transaction_id TEXT NOT NULL,
    transaction_json TEXT NOT NULL,
    structure_version INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, transaction_id)
);

CREATE TABLE persistence_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    task_kind TEXT NOT NULL,
    task_json TEXT NOT NULL,
    state TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE runtime_snapshots (
    document_id BLOB PRIMARY KEY NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    structure_version INTEGER NOT NULL,
    content_version INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
