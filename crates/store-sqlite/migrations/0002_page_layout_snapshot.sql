CREATE TABLE page_layout_v2 (
    document_id BLOB NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    visible_index_version INTEGER NOT NULL,
    structure_version INTEGER NOT NULL,
    layout_key_hash TEXT NOT NULL,
    page_policy_version INTEGER NOT NULL,
    page_index INTEGER NOT NULL,
    block_start_index INTEGER NOT NULL,
    block_count INTEGER NOT NULL,
    first_block_id BLOB,
    last_block_id BLOB,
    height REAL NOT NULL,
    measured_ratio REAL NOT NULL DEFAULT 0,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint REAL NOT NULL DEFAULT 0,
    dirty INTEGER NOT NULL DEFAULT 0 CHECK(dirty IN (0, 1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(
        document_id,
        visible_index_version,
        structure_version,
        layout_key_hash,
        page_policy_version,
        page_index
    )
);

INSERT INTO page_layout_v2 (
    document_id,
    visible_index_version,
    structure_version,
    layout_key_hash,
    page_policy_version,
    page_index,
    block_start_index,
    block_count,
    height,
    updated_at
)
SELECT
    document_id,
    0,
    structure_version,
    layout_key_hash,
    page_policy_version,
    page_index,
    0,
    0,
    height,
    updated_at
FROM page_layout;

DROP TABLE page_layout;
ALTER TABLE page_layout_v2 RENAME TO page_layout;

CREATE INDEX idx_sqlite_page_layout_lookup
ON page_layout(
    document_id,
    visible_index_version,
    structure_version,
    layout_key_hash,
    page_policy_version,
    page_index
);
