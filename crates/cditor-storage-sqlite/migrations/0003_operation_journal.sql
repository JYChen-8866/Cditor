-- P7-003：operation journal / sync outbox / checkpoint / crash marker。
-- journal 是本地耐久性的真相：materialized 行 + journal 条目 + outbox 条目
-- 必须在同一个 SQLite 事务内提交（P7-004）。

CREATE TABLE operation_journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id BLOB NOT NULL,
    transaction_id INTEGER NOT NULL,
    schema_major INTEGER NOT NULL,
    schema_minor INTEGER NOT NULL,
    -- Operation 域 VersionedEnvelope 的原始 JSON（未知内容字节保留）。
    envelope_json TEXT NOT NULL,
    -- ChangeOrigin wire tag（user/ime/remote/...）。
    origin TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_journal_document ON operation_journal(document_id, id);

CREATE TABLE sync_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    journal_id INTEGER NOT NULL REFERENCES operation_journal(id) ON DELETE CASCADE,
    document_id BLOB NOT NULL,
    -- pending -> inflight -> acked | rejected；rejected 保留 last_error。
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK(state IN ('pending', 'inflight', 'acked', 'rejected')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_outbox_document_state ON sync_outbox(document_id, state, id);

CREATE TABLE journal_checkpoints (
    document_id BLOB PRIMARY KEY NOT NULL,
    -- materialized 状态已包含到（含）该 journal id。
    journal_id INTEGER NOT NULL,
    materialized_checksum INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- 单行 crash marker：启动置 dirty，干净退出置 clean。
CREATE TABLE crash_marker (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    started_at INTEGER NOT NULL,
    clean_shutdown INTEGER NOT NULL DEFAULT 0 CHECK(clean_shutdown IN (0, 1))
);
