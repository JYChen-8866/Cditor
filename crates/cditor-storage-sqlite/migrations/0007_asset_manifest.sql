CREATE TABLE assets (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    local_source TEXT NOT NULL,
    content_hash TEXT,
    state TEXT NOT NULL CHECK(state IN ('local_pending', 'uploading', 'ready', 'failed', 'deleted')),
    canonical_asset_id BLOB,
    upload_session_id TEXT,
    uploaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK(uploaded_bytes >= 0),
    remote_object_key TEXT,
    public_url TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

CREATE INDEX idx_assets_workspace_state
ON assets(workspace_id, state, updated_at);

CREATE INDEX idx_assets_workspace_hash
ON assets(workspace_id, content_hash)
WHERE content_hash IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE block_assets (
    document_id BLOB NOT NULL,
    block_id BLOB NOT NULL,
    asset_id BLOB NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    role TEXT NOT NULL DEFAULT 'main',
    created_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, block_id, asset_id, role),
    FOREIGN KEY(document_id, block_id)
        REFERENCES blocks(document_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_block_assets_asset ON block_assets(asset_id);
