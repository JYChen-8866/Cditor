-- Cditor PostgreSQL initial schema.
-- This schema is designed for 10w-block rich-text documents, async payload windows,
-- cloud sync, server-side search, layout cache, and crash/retry recovery.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS schema_migrations_meta (
    version BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    checksum TEXT,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    root_path TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_opened_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_workspaces_last_opened_at
ON workspaces(last_opened_at);

CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    icon_json JSONB,
    cover_json JSONB,
    metadata_json JSONB,
    structure_version BIGINT NOT NULL DEFAULT 1,
    content_version BIGINT NOT NULL DEFAULT 1,
    layout_version BIGINT NOT NULL DEFAULT 0,
    schema_version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_documents_workspace_id
ON documents(workspace_id)
WHERE deleted_at IS NULL;

CREATE TABLE document_tree (
    document_id UUID PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    parent_id UUID,
    prev_id UUID,
    next_id UUID,
    sort_key TEXT NOT NULL DEFAULT '',
    depth INTEGER NOT NULL DEFAULT 0,
    collapsed BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_document_tree_workspace_parent_sort
ON document_tree(workspace_id, parent_id, sort_key);

CREATE TABLE blocks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    parent_id UUID,
    prev_id UUID,
    next_id UUID,
    sort_key TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL,
    flags INTEGER NOT NULL DEFAULT 0,
    content_version BIGINT NOT NULL DEFAULT 1,
    structure_version BIGINT NOT NULL DEFAULT 1,
    attrs_version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_blocks_document_sort
ON blocks(document_id, sort_key)
WHERE deleted_at IS NULL;

CREATE INDEX idx_blocks_document_parent_sort
ON blocks(document_id, parent_id, sort_key)
WHERE deleted_at IS NULL;

CREATE INDEX idx_blocks_document_structure_version
ON blocks(document_id, structure_version);

CREATE INDEX idx_blocks_prev_id
ON blocks(prev_id);

CREATE INDEX idx_blocks_next_id
ON blocks(next_id);

CREATE TABLE block_attrs (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    attrs_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    attrs_version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE block_payloads (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    payload_format TEXT NOT NULL,
    payload_json JSONB,
    payload_bytes BYTEA,
    plain_text TEXT NOT NULL DEFAULT '',
    content_hash TEXT,
    content_version BIGINT NOT NULL DEFAULT 1,
    byte_len BIGINT NOT NULL DEFAULT 0,
    inline_run_count INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_block_payloads_document_id
ON block_payloads(document_id);

CREATE INDEX idx_block_payloads_content_version
ON block_payloads(block_id, content_version);

CREATE INDEX idx_block_payloads_text_len
ON block_payloads(byte_len);

CREATE TABLE block_code_meta (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    language TEXT,
    line_count INTEGER NOT NULL DEFAULT 0,
    syntax_version BIGINT NOT NULL DEFAULT 0,
    fold_state_json JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE block_tables (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    row_count INTEGER NOT NULL DEFAULT 0,
    column_count INTEGER NOT NULL DEFAULT 0,
    header_rows INTEGER NOT NULL DEFAULT 0,
    header_cols INTEGER NOT NULL DEFAULT 0,
    table_version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE block_table_rows (
    block_id UUID NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    row_index INTEGER NOT NULL,
    row_id UUID NOT NULL DEFAULT gen_random_uuid(),
    height DOUBLE PRECISION,
    row_version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(block_id, row_index)
);

CREATE UNIQUE INDEX idx_block_table_rows_row_id
ON block_table_rows(block_id, row_id);

CREATE TABLE block_table_cells (
    block_id UUID NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    row_id UUID NOT NULL,
    column_index INTEGER NOT NULL,
    payload_json JSONB NOT NULL,
    plain_text TEXT NOT NULL DEFAULT '',
    content_version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(block_id, row_id, column_index)
);

CREATE TABLE assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    object_key TEXT NOT NULL,
    public_url TEXT,
    media_type TEXT,
    hash TEXT,
    size_bytes BIGINT,
    width INTEGER,
    height INTEGER,
    duration_ms INTEGER,
    metadata_json JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_assets_workspace_id
ON assets(workspace_id)
WHERE deleted_at IS NULL;

CREATE INDEX idx_assets_hash
ON assets(hash);

CREATE TABLE block_assets (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    asset_id UUID NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    role TEXT NOT NULL DEFAULT 'main',
    stable_width DOUBLE PRECISION,
    stable_estimated_height DOUBLE PRECISION,
    stable_min_height DOUBLE PRECISION,
    stable_max_height DOUBLE PRECISION,
    stable_confidence INTEGER,
    aspect_ratio DOUBLE PRECISION,
    caption_payload_json JSONB
);

CREATE TABLE block_layout (
    block_id UUID NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    layout_key_hash TEXT NOT NULL,
    width_bucket INTEGER NOT NULL,
    exact_width DOUBLE PRECISION,
    content_version BIGINT NOT NULL,
    attrs_version BIGINT NOT NULL DEFAULT 0,
    style_version BIGINT NOT NULL DEFAULT 0,
    font_version BIGINT NOT NULL DEFAULT 0,
    theme_version BIGINT NOT NULL DEFAULT 0,
    scale_factor DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    measured_height DOUBLE PRECISION,
    estimated_height DOUBLE PRECISION NOT NULL,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint DOUBLE PRECISION NOT NULL DEFAULT 0,
    line_count INTEGER,
    layout_cost INTEGER NOT NULL DEFAULT 0,
    measured_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (block_id, layout_key_hash)
);

CREATE INDEX idx_block_layout_document_id
ON block_layout(document_id);

CREATE INDEX idx_block_layout_block_measured_at
ON block_layout(block_id, measured_at);

CREATE TABLE page_layout (
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    visible_index_version BIGINT NOT NULL DEFAULT 0,
    structure_version BIGINT NOT NULL,
    layout_key_hash TEXT NOT NULL,
    page_policy_version BIGINT NOT NULL,
    page_index INTEGER NOT NULL,
    block_start_index INTEGER NOT NULL,
    block_count INTEGER NOT NULL,
    first_block_id UUID,
    last_block_id UUID,
    height DOUBLE PRECISION NOT NULL,
    measured_ratio DOUBLE PRECISION NOT NULL DEFAULT 0,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint DOUBLE PRECISION NOT NULL DEFAULT 0,
    dirty BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        document_id,
        visible_index_version,
        structure_version,
        layout_key_hash,
        page_policy_version,
        page_index
    )
);

CREATE TABLE document_index_snapshot (
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    visible_index_version BIGINT NOT NULL DEFAULT 0,
    structure_version BIGINT NOT NULL,
    snapshot_format TEXT NOT NULL,
    snapshot_bytes BYTEA NOT NULL,
    block_count INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(document_id, visible_index_version, structure_version)
);

CREATE TABLE edit_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    transaction_kind TEXT NOT NULL,
    ops_json JSONB NOT NULL,
    inverse_ops_json JSONB,
    affected_blocks_json JSONB NOT NULL,
    before_selection_json JSONB,
    after_selection_json JSONB,
    before_anchor_json JSONB,
    after_anchor_json JSONB,
    structure_version_before BIGINT,
    structure_version_after BIGINT,
    content_version_after BIGINT,
    client_id UUID,
    device_id UUID,
    sequence BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    persisted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_edit_transactions_document_created
ON edit_transactions(document_id, created_at);

CREATE TABLE undo_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    transaction_id UUID NOT NULL REFERENCES edit_transactions(id) ON DELETE CASCADE,
    payload_kind TEXT NOT NULL,
    block_count INTEGER NOT NULL DEFAULT 0,
    byte_len BIGINT NOT NULL DEFAULT 0,
    snapshot_json JSONB,
    snapshot_bytes BYTEA,
    external_path TEXT,
    checksum TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ
);

CREATE TABLE persistence_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    task_kind TEXT NOT NULL,
    task_json JSONB NOT NULL,
    affected_blocks_json JSONB NOT NULL,
    state TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    next_retry_at TIMESTAMPTZ
);

CREATE INDEX idx_persistence_queue_state_retry
ON persistence_queue(state, next_retry_at, created_at);

CREATE TABLE runtime_snapshots (
    document_id UUID PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    structure_version BIGINT NOT NULL,
    content_version BIGINT NOT NULL,
    focused_block_id UUID,
    selection_json JSONB,
    scroll_anchor_json JSONB,
    render_window_json JSONB,
    dirty_blocks_json JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE collections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    schema_version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE collection_properties (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    property_type TEXT NOT NULL,
    config_json JSONB NOT NULL,
    sort_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE collection_views (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    view_type TEXT NOT NULL,
    name TEXT NOT NULL,
    config_json JSONB NOT NULL,
    sort_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE collection_rows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    sort_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE collection_cells (
    row_id UUID NOT NULL REFERENCES collection_rows(id) ON DELETE CASCADE,
    property_id UUID NOT NULL REFERENCES collection_properties(id) ON DELETE CASCADE,
    value_json JSONB NOT NULL,
    plain_text TEXT NOT NULL DEFAULT '',
    content_version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(row_id, property_id)
);

CREATE TABLE database_block_bindings (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    view_id UUID REFERENCES collection_views(id) ON DELETE SET NULL,
    title TEXT
);

CREATE TABLE sync_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID REFERENCES workspaces(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    local_transaction_id UUID NOT NULL REFERENCES edit_transactions(id) ON DELETE CASCADE,
    operation_kind TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    affected_blocks_json JSONB NOT NULL,
    base_structure_version BIGINT,
    base_content_version BIGINT,
    client_id UUID NOT NULL,
    device_id UUID NOT NULL,
    sequence BIGINT NOT NULL,
    state TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    server_ack_at TIMESTAMPTZ,
    server_revision TEXT
);

CREATE INDEX idx_sync_outbox_state_sequence
ON sync_outbox(state, sequence);

CREATE INDEX idx_sync_outbox_document_state
ON sync_outbox(document_id, state, sequence);

CREATE TABLE sync_state (
    document_id UUID PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    client_id UUID NOT NULL,
    device_id UUID NOT NULL,
    last_local_sequence BIGINT NOT NULL DEFAULT 0,
    last_uploaded_sequence BIGINT NOT NULL DEFAULT 0,
    last_server_revision TEXT,
    last_pulled_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE remote_tombstones (
    entity_id UUID PRIMARY KEY,
    entity_kind TEXT NOT NULL,
    document_id UUID,
    deleted_by_client_id UUID,
    deleted_by_device_id UUID,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    server_revision TEXT
);

CREATE TABLE block_search (
    block_id UUID PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    plain_text TEXT NOT NULL DEFAULT '',
    search_vector TSVECTOR,
    content_version BIGINT NOT NULL,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_block_search_document_id
ON block_search(document_id);

CREATE INDEX idx_block_search_vector
ON block_search USING GIN(search_vector);
