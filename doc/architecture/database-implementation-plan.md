# Cditor 数据库完整实现方案与任务清单

> 目标：重新设计并完整落地 `Cditor` 的本地数据库层，使其服务于 10w block 级大文档富文本编辑器，而不是只做简单 CRUD 保存。数据库层必须匹配当前项目的 `DocumentRuntime / DocumentIndex / VisibleDocumentIndex / PayloadWindow / EditTransaction / LayoutCache / OptimisticPersistence` 架构。

## 0. 设计结论

当前数据库不能设计成“一个 document 表 + 一个 blocks 表 + 正文 JSON”。

完整方案应该是一个面向大文档编辑运行时的本地存储系统：

```txt
PostgreSQL
├── workspace / document metadata
├── document structure index
├── block attrs
├── block payload window
├── edit transaction log
├── undo large payload snapshot
├── layout cache
├── page layout cache
├── asset / media registry
├── full-text search index
├── persistence queue
└── crash recovery snapshot
```

关键原则：

- `DocumentRuntime` 是编辑时的内存真相。
- PostgreSQL 是云端持久化真相。
- UI 不是任何数据真相。
- 结构索引与 payload 必须分离。
- 启动不能加载 10w block 的全部正文。
- 编辑操作必须以 transaction 原子保存。
- layout cache 可以失效降级，但正文不能错。
- 保存失败时不能丢内存内容，dirty block 必须 pin。

---

## 1. 当前项目状态

### 1.1 已有内容

项目中已有：

```txt
src/storage/
├── cache_recovery.rs
├── height_write_debounce.rs
├── layout_cache.rs
├── mod.rs
├── optimistic_persistence.rs
└── traits.rs
```

已有能力：

- `layout_cache.rs`
  - `BLOCK_LAYOUT_TABLE_SQL`
  - `PAGE_LAYOUT_TABLE_SQL`
  - `InMemoryLayoutCacheStore`
- `optimistic_persistence.rs`
  - `PersistenceState`
  - `BlockPersistenceState`
  - `OptimisticPersistenceManager`
- `traits.rs`
  - 简化版 `DocumentIndexStore`

### 1.2 缺失内容

当前缺失：

- PostgreSQL / sqlx 依赖。
- PostgreSQL connection pool 管理。
- migration 系统。
- 完整 schema。
- `DocumentStore`。
- `BlockPayloadStore`。
- `LayoutCacheStore` PostgreSQL 实现。
- `TransactionStore`。
- `AssetStore`。
- `FtsStore`。
- `PersistenceQueue`。
- 后台 persistence worker。
- crash recovery。
- runtime cold start 接入。
- payload window 按需加载。
- 事务保存失败恢复。

---

## 2. 设计目标

### 2.1 必须支持

- [ ] 10w block 文档冷启动。
- [ ] 启动只加载结构索引，不加载全量 payload。
- [ ] render window 附近 payload 按需加载。
- [ ] 单字符输入低延迟，不被 DB 写阻塞。
- [ ] split / merge / insert / delete / move block 原子持久化。
- [ ] paste 1w+ blocks 可恢复、可 undo，不撑爆内存。
- [ ] layout cache 持久化，冷启动可复用历史高度。
- [ ] PostgreSQL / 网络写失败时内存内容不丢。
- [ ] app crash 后可恢复最近已提交事务和未完成保存队列。
- [ ] 全文搜索不依赖 UI entity。
- [ ] 图片、文件、whiteboard 等资源可追踪、可清理、可恢复。

### 2.2 性能目标

| 场景 | 目标 |
|---|---|
| 10w blocks 加载 document index | 不加载全部 payload |
| 打开文档首屏 | 优先显示 placeholder + 历史高度 |
| 单字符输入 | 不同步等待 PostgreSQL |
| block split/merge | 局部结构更新，DB 后台事务保存 |
| 滚动到新窗口 | 按 block id 批量加载 payload |
| 关闭文档 | flush dirty queue 或提示 |
| layout height 写入 | debounce / batch |
| Search index 更新 | 后台增量 |

### 2.3 非目标

- 第一阶段不实现实时多人协同协议。
- 第一阶段不实现 CRDT。
- 不把 PostgreSQL 当实时编辑锁。
- 不让 UI entity 参与存储。
- 第一阶段不实现云同步，但 schema、id、transaction log、outbox 必须为后续云服务预留。

---

## 3. 技术选型

### 3.0 Store 接口口径

本方案与总架构保持一致：对 `DocumentRuntime / PayloadWindow / editor` 暴露的是异步/命令式存储接口；PostgreSQL 访问通过 `sqlx::PgPool` 和后台任务执行，绝不能让 UI hot path 直接等待远程数据库。

```txt
Runtime / Editor
  -> async DocumentStore / PersistenceCommand
    -> Persistence / Sync task
      -> sqlx PgPool
        -> PostgreSQL
```

对于桌面客户端，即使连接云端 PostgreSQL，编辑也必须先落到内存 runtime 和 pending queue，然后后台保存/同步。

### 3.1 PostgreSQL 访问库

推荐使用：

```toml
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "uuid", "chrono", "json", "migrate"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

原因：

- 最终目标是云服务，多端同步和团队协作。
- PostgreSQL 支持强事务、JSONB、GIN 索引、行级锁、全文检索、分区和后续权限模型。
- `sqlx` 适合服务端/后台异步数据库访问。
- 客户端本地离线缓存可以后置为可选模块，但不是主数据库方案。

### 3.2 PostgreSQL 连接与事务要求

必须使用连接池和短事务：

```rust
PgPoolOptions::new()
    .max_connections(...)
    .acquire_timeout(...)
```

写入事务要求：

```sql
BEGIN;
-- write edit_transactions
-- write blocks / payloads / attrs
-- write outbox / search task
COMMIT;
```

结构编辑需要乐观并发控制：

```sql
WHERE document_id = $1 AND structure_version = $expected
```

冲突时不能覆盖服务端新版本，必须返回 conflict / retry / recovery。

### 3.3 本地 Docker PostgreSQL 开发环境

开发阶段使用 Docker 启动 PostgreSQL，不要求开发者安装系统级 PostgreSQL。

已提供：

```txt
docker-compose.yml
.env.example
migrations/0001_initial.sql
```

启动命令：

```sh
docker compose up -d
```

开发库：

```txt
postgres://cditor:cditor@localhost:5432/cditor_dev
```

测试库：

```txt
postgres://cditor:cditor@localhost:5433/cditor_test
```

验收方式：

```sh
docker compose ps
# postgres 和 postgres_test 都应为 healthy
```

迁移验收在 DB-Phase 2 完成后执行：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@localhost:5432/cditor_dev cargo test postgres_migration -- --ignored
```

---

## 4. 模块设计

目标目录：

```txt
src/storage/
├── mod.rs
├── traits.rs
├── optimistic_persistence.rs
├── layout_cache.rs
├── height_write_debounce.rs
├── cache_recovery.rs
└── postgres/
    ├── mod.rs
    ├── error.rs
    ├── pool.rs
    ├── migrations.rs
    ├── schema.rs
    ├── types.rs
    ├── document_store.rs
    ├── block_store.rs
    ├── payload_store.rs
    ├── layout_store.rs
    ├── transaction_store.rs
    ├── asset_store.rs
    ├── search_store.rs
    ├── sync_outbox.rs
    ├── snapshot_store.rs
    └── tests.rs
```

### 4.1 模块职责

| 模块 | 职责 |
|---|---|
| `error.rs` | 统一存储错误类型 |
| `pool.rs` | PostgreSQL 连接池、事务入口 |
| `migrations.rs` | schema migration 执行 |
| `schema.rs` | v1 全量 schema SQL |
| `types.rs` | DB row 类型与 runtime 类型转换 |
| `document_store.rs` | document metadata / tree |
| `block_store.rs` | block structure index |
| `payload_store.rs` | block payload window |
| `layout_store.rs` | block/page layout cache |
| `transaction_store.rs` | edit transaction log |
| `asset_store.rs` | asset/media registry |
| `search_store.rs` | `tsvector` / search index 更新与查询 |
| `sync_outbox.rs` | 云同步 outbox、ack、retry |
| `snapshot_store.rs` | index snapshot / recovery snapshot |

---

## 5. 数据分层

### 5.1 Workspace 层

保存 workspace、文档树入口、最近打开记录。

### 5.2 Document Metadata 层

保存文档标题、icon、cover、版本、删除状态。

### 5.3 Structure Index 层

保存轻量结构：

- block 顺序
- parent / prev / next
- depth
- kind
- flags
- versions

用于快速构建：

- `DocumentIndex`
- `VisibleDocumentIndex`

### 5.4 Payload 层

保存重内容：

- rich text spans
- code text
- table payload
- image/file payload
- whiteboard payload
- embed/html payload

用于按需加载 `PayloadWindow`。

### 5.5 Layout Cache 层

保存高度缓存：

- block height
- page height
- confidence
- layout key
- max error hint

用于冷启动和滚动稳定。

### 5.6 Transaction 层

保存用户编辑事务：

- ops
- inverse ops
- selection before/after
- affected blocks
- timestamp

用于：

- undo/redo
- crash recovery
- replay/debug

### 5.7 Asset 层

保存资源引用和本地缓存索引：

- image
- file
- whiteboard
- embed thumbnail

### 5.8 FTS 层

保存 block 粒度全文搜索索引。

---

## 6. Schema v1 设计

PostgreSQL 主 schema 使用 `UUID` 作为所有核心实体 id。当前 Rust 代码里的数字 `BlockId` / `DocumentId` 只是临时 demo 类型，进入真实存储层时必须通过转换层迁移到 UUID。

为后续云服务预留：

- 所有核心实体 id 必须全局稳定，后续可切到 UUID/ULID。
- transaction 必须可序列化并上传。
- 删除使用 tombstone/soft delete，不做立即物理删除。
- 本地写入必须进入 outbox，支持网络恢复后上传。
- 服务端 ack 后更新 sync state。

### 6.1 `schema_migrations`

生产实现优先使用 `sqlx::migrate!`，该表用于记录自定义 schema 元数据或兼容手写 migration。

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    checksum TEXT,
    applied_at INTEGER NOT NULL
);
```

### 6.2 `workspaces`

```sql
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root_path TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_opened_at INTEGER
);
```

索引：

```sql
CREATE INDEX idx_workspaces_last_opened_at
ON workspaces(last_opened_at);
```

### 6.3 `documents`

```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT,
    title TEXT NOT NULL DEFAULT '',
    icon_json TEXT,
    cover_json TEXT,
    metadata_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    structure_version INTEGER NOT NULL DEFAULT 1,
    content_version INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id)
);
```

索引：

```sql
CREATE INDEX idx_documents_workspace_id
ON documents(workspace_id, deleted_at);

CREATE INDEX idx_documents_parent_document_id
ON documents(parent_document_id, deleted_at);
```

### 6.4 `document_tree`

用于 workspace 文档树排序。block 内部结构不放这里。`document_tree` 是文档树 parent/prev/next/sort_key 的唯一真相；`documents` 禁止再保存 `parent_document_id`，避免双真相。

```sql
CREATE TABLE document_tree (
    document_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    parent_id TEXT,
    prev_id TEXT,
    next_id TEXT,
    sort_key TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 0,
    collapsed INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(document_id) REFERENCES documents(id),
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id)
);
```

索引：

```sql
CREATE INDEX idx_document_tree_workspace_parent_sort
ON document_tree(workspace_id, parent_id, sort_key);
```

### 6.5 `blocks`

结构索引真相。

```sql
CREATE TABLE blocks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    parent_id TEXT,
    prev_id TEXT,
    next_id TEXT,
    sort_key TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 0,
    kind INTEGER NOT NULL,
    flags INTEGER NOT NULL DEFAULT 0,
    content_version INTEGER NOT NULL DEFAULT 1,
    structure_version INTEGER NOT NULL DEFAULT 1,
    attrs_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

索引：

```sql
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
```

设计说明：

- `sort_key` 用于稳定排序和局部插入。
- `prev_id` / `next_id` 用于局部链表修复和一致性校验。
- `depth` 是冗余字段，用于快速构建 `DocumentIndex`。
- `flags` 用于 folded / locked / has_children 等快速判断。
- `deleted_at` 支持软删除和 crash recovery。

### 6.6 `block_attrs`

```sql
CREATE TABLE block_attrs (
    block_id TEXT PRIMARY KEY,
    attrs_json TEXT NOT NULL,
    attrs_version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(block_id) REFERENCES blocks(id)
);
```

### 6.7 `block_payloads`

payload 重内容表。

```sql
CREATE TABLE block_payloads (
    block_id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    payload_format TEXT NOT NULL,
    payload_json TEXT,
    plain_text TEXT NOT NULL DEFAULT '',
    content_hash TEXT,
    content_version INTEGER NOT NULL DEFAULT 1,
    byte_len INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(block_id) REFERENCES blocks(id),
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

索引：

```sql
CREATE INDEX idx_block_payloads_document_id
ON block_payloads(document_id);

CREATE INDEX idx_block_payloads_content_version
ON block_payloads(block_id, content_version);
```

设计说明：

- `plain_text` 用于 FTS、搜索预览、轻量复制。
- `payload_json` 保存具体 payload。
- 大 payload 后续可以迁移到外部 blob 表或文件。

### 6.8 `block_code_meta`

```sql
CREATE TABLE block_code_meta (
    block_id TEXT PRIMARY KEY,
    language TEXT,
    line_count INTEGER NOT NULL DEFAULT 0,
    syntax_version INTEGER NOT NULL DEFAULT 0,
    fold_state_json TEXT,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(block_id) REFERENCES blocks(id)
);
```

### 6.9 `block_tables`

表格 block 分两层：

- 小表可以完整保存在 `block_payloads.payload_json`。
- 大表 / native table 必须支持内部虚拟化，不能强制加载整表 JSON。

`block_tables` 保存表格元信息，具体大表内容由 `block_table_rows` / `block_table_cells` 按 row/cell window 加载。

```sql
CREATE TABLE block_tables (
    block_id TEXT PRIMARY KEY,
    row_count INTEGER NOT NULL DEFAULT 0,
    column_count INTEGER NOT NULL DEFAULT 0,
    header_rows INTEGER NOT NULL DEFAULT 0,
    header_cols INTEGER NOT NULL DEFAULT 0,
    table_version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(block_id) REFERENCES blocks(id)
);

CREATE TABLE block_table_rows (
    block_id TEXT NOT NULL,
    row_index INTEGER NOT NULL,
    row_id TEXT NOT NULL,
    height REAL,
    row_version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(block_id, row_index),
    FOREIGN KEY(block_id) REFERENCES blocks(id)
);

CREATE UNIQUE INDEX idx_block_table_rows_row_id
ON block_table_rows(block_id, row_id);

CREATE TABLE block_table_cells (
    block_id TEXT NOT NULL,
    row_id TEXT NOT NULL,
    column_index INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    plain_text TEXT NOT NULL DEFAULT '',
    content_version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(block_id, row_id, column_index),
    FOREIGN KEY(block_id) REFERENCES blocks(id)
);
```

### 6.10 `block_assets`

block 到资源的引用。

```sql
CREATE TABLE block_assets (
    id TEXT PRIMARY KEY,
    block_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    asset_kind TEXT NOT NULL,
    source TEXT NOT NULL,
    local_path TEXT,
    media_type TEXT,
    name TEXT,
    size_bytes INTEGER,
    checksum TEXT,
    width REAL,
    height REAL,
    stable_estimated_height REAL,
    stable_min_height REAL,
    stable_max_height REAL,
    stable_confidence INTEGER,
    metadata_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY(block_id) REFERENCES blocks(id),
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

索引：

```sql
CREATE INDEX idx_block_assets_block_id
ON block_assets(block_id)
WHERE deleted_at IS NULL;

CREATE INDEX idx_block_assets_document_id
ON block_assets(document_id)
WHERE deleted_at IS NULL;

CREATE INDEX idx_block_assets_checksum
ON block_assets(checksum);
```

### 6.11 `asset_blobs`

可选本地 blob/对象缓存注册表。第一版不强制把二进制存 PostgreSQL，推荐存对象存储，DB 存索引。

```sql
CREATE TABLE asset_blobs (
    checksum TEXT PRIMARY KEY,
    storage_kind TEXT NOT NULL,
    local_path TEXT,
    media_type TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    ref_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at INTEGER,
    created_at INTEGER NOT NULL,
    deleted_at INTEGER
);
```

### 6.12 `block_layout`

沿用并扩展当前 `layout_cache.rs` 的设计。

```sql
CREATE TABLE block_layout (
    block_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    layout_key_hash TEXT NOT NULL,
    width_bucket INTEGER NOT NULL,
    exact_width REAL,
    content_version INTEGER NOT NULL,
    attrs_version INTEGER NOT NULL DEFAULT 0,
    style_version INTEGER NOT NULL DEFAULT 0,
    font_version INTEGER NOT NULL DEFAULT 0,
    theme_version INTEGER NOT NULL DEFAULT 0,
    scale_factor REAL NOT NULL DEFAULT 1.0,
    measured_height REAL,
    estimated_height REAL NOT NULL,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint REAL NOT NULL DEFAULT 0,
    line_count INTEGER,
    layout_cost INTEGER NOT NULL DEFAULT 0,
    measured_at INTEGER,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (block_id, layout_key_hash),
    FOREIGN KEY(block_id) REFERENCES blocks(id),
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

索引：

```sql
CREATE INDEX idx_block_layout_document_id
ON block_layout(document_id);

CREATE INDEX idx_block_layout_block_measured_at
ON block_layout(block_id, measured_at);
```

### 6.13 `page_layout`

```sql
CREATE TABLE page_layout (
    document_id TEXT NOT NULL,
    visible_index_version INTEGER NOT NULL DEFAULT 0,
    structure_version INTEGER NOT NULL,
    layout_key_hash TEXT NOT NULL,
    page_policy_version INTEGER NOT NULL,
    page_index INTEGER NOT NULL,
    block_start_index INTEGER NOT NULL,
    block_count INTEGER NOT NULL,
    first_block_id TEXT,
    last_block_id TEXT,
    height REAL NOT NULL,
    measured_ratio REAL NOT NULL DEFAULT 0,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint REAL NOT NULL DEFAULT 0,
    dirty INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (
        document_id,
        visible_index_version,
        structure_version,
        layout_key_hash,
        page_policy_version,
        page_index
    ),
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

### 6.14 `document_index_snapshot`

用于超大文档冷启动加速。不是唯一真相，失效后可从 `blocks` 重建。

```sql
CREATE TABLE document_index_snapshot (
    document_id TEXT NOT NULL,
    structure_version INTEGER NOT NULL,
    visible_index_version INTEGER NOT NULL DEFAULT 0,
    snapshot_format TEXT NOT NULL,
    snapshot_blob BLOB NOT NULL,
    block_count INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, structure_version, visible_index_version),
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

### 6.15 `edit_transactions`

```sql
CREATE TABLE edit_transactions (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    transaction_kind TEXT NOT NULL,
    ops_json TEXT NOT NULL,
    inverse_ops_json TEXT,
    affected_blocks_json TEXT NOT NULL,
    before_selection_json TEXT,
    after_selection_json TEXT,
    before_anchor_json TEXT,
    after_anchor_json TEXT,
    structure_version_before INTEGER,
    structure_version_after INTEGER,
    content_version_after INTEGER,
    created_at INTEGER NOT NULL,
    persisted_at INTEGER NOT NULL,
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

索引：

```sql
CREATE INDEX idx_edit_transactions_document_created
ON edit_transactions(document_id, created_at);
```

### 6.16 `undo_snapshots`

用于大 paste / 大 delete / 大 move 的外部 undo payload。

```sql
CREATE TABLE undo_snapshots (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    payload_kind TEXT NOT NULL,
    block_count INTEGER NOT NULL DEFAULT 0,
    byte_len INTEGER NOT NULL DEFAULT 0,
    snapshot_json TEXT,
    snapshot_blob BLOB,
    external_path TEXT,
    checksum TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    FOREIGN KEY(document_id) REFERENCES documents(id),
    FOREIGN KEY(transaction_id) REFERENCES edit_transactions(id)
);
```

### 6.17 `persistence_queue`

保存未完成或失败的持久化任务，支持 crash 后恢复。

```sql
CREATE TABLE persistence_queue (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    task_kind TEXT NOT NULL,
    task_json TEXT NOT NULL,
    affected_blocks_json TEXT NOT NULL,
    state TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    next_retry_at INTEGER,
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

索引：

```sql
CREATE INDEX idx_persistence_queue_state_retry
ON persistence_queue(state, next_retry_at, created_at);
```

### 6.18 `runtime_snapshots`

可选，用于 crash recovery 和快速恢复编辑上下文。

```sql
CREATE TABLE runtime_snapshots (
    document_id TEXT PRIMARY KEY,
    structure_version INTEGER NOT NULL,
    content_version INTEGER NOT NULL,
    focused_block_id TEXT,
    selection_json TEXT,
    scroll_anchor_json TEXT,
    render_window_json TEXT,
    dirty_blocks_json TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

### 6.19 `sync_outbox`

后续云服务需要 local-first 同步。客户端每个已应用本地事务都应该进入 outbox，等待上传和服务端 ack。第一阶段可以只建表不接远程。

```sql
CREATE TABLE sync_outbox (
    id TEXT PRIMARY KEY,
    workspace_id TEXT,
    document_id TEXT NOT NULL,
    local_transaction_id TEXT NOT NULL,
    operation_kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    affected_blocks_json TEXT NOT NULL,
    base_structure_version INTEGER,
    base_content_version INTEGER,
    client_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    state TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    server_ack_at INTEGER,
    server_revision TEXT,
    FOREIGN KEY(document_id) REFERENCES documents(id),
    FOREIGN KEY(local_transaction_id) REFERENCES edit_transactions(id)
);

CREATE INDEX idx_sync_outbox_state_sequence
ON sync_outbox(state, sequence);

CREATE INDEX idx_sync_outbox_document_state
ON sync_outbox(document_id, state, sequence);
```

### 6.20 `sync_state`

```sql
CREATE TABLE sync_state (
    document_id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    last_local_sequence INTEGER NOT NULL DEFAULT 0,
    last_uploaded_sequence INTEGER NOT NULL DEFAULT 0,
    last_server_revision TEXT,
    last_pulled_at INTEGER,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

### 6.21 `remote_tombstones`

用于云同步删除传播和冲突恢复。第一阶段本地删除仍以各业务表 `deleted_at` 为主，`remote_tombstones` 为跨端同步索引。

```sql
CREATE TABLE remote_tombstones (
    entity_id TEXT PRIMARY KEY,
    entity_kind TEXT NOT NULL,
    document_id TEXT,
    deleted_by_client_id TEXT,
    deleted_by_device_id TEXT,
    deleted_at INTEGER NOT NULL,
    server_revision TEXT
);
```

### 6.22 `block_search`

使用 PostgreSQL `tsvector` + GIN。中文搜索后续可接 `pg_jieba`、Meilisearch 或 OpenSearch。

```sql
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
```

更新策略：

- payload 保存成功后更新 FTS。
- 删除 block 后从 FTS 删除。
- 大批量 paste 后批量更新。
- FTS 写失败不影响正文保存，但要进入 repair queue。
- FTS update 必须携带 `content_version`；旧版本任务不能覆盖新版本索引。

---

## 7. Rust Trait 设计

### 7.1 `DocumentStore`

```rust
pub trait DocumentStore {
    async fn load_document_metadata(
        &self,
        document_id: DocumentId,
    ) -> Result<DocumentMetadataRow, StorageError>;

    async fn save_document_metadata(
        &self,
        metadata: &DocumentMetadataRow,
    ) -> Result<(), StorageError>;

    async fn load_document_index(
        &self,
        document_id: DocumentId,
    ) -> Result<LoadedDocumentIndex, StorageError>;

    async fn save_structure_changes(
        &self,
        changes: &StructureChangeSet,
    ) -> Result<(), StorageError>;
}
```

### 7.2 `PayloadStore`

```rust
pub trait PayloadStore {
    async fn load_block_payloads(
        &self,
        block_ids: &[BlockId],
    ) -> Result<Vec<BlockPayloadRecord>, StorageError>;

    async fn save_block_payloads(
        &self,
        payloads: &[BlockPayloadRecord],
    ) -> Result<(), StorageError>;
}
```

### 7.3 `LayoutCacheStore`

```rust
pub trait LayoutCacheStore {
    async fn load_block_height(
        &self,
        block_id: BlockId,
        key: LayoutCacheKey,
    ) -> Result<CachedHeight, StorageError>;

    async fn save_block_layout(
        &self,
        row: BlockLayoutRow,
    ) -> Result<(), StorageError>;

    async fn load_page_height(
        &self,
        request: PageLayoutCacheRequest,
    ) -> Result<CachedPageHeight, StorageError>;

    async fn save_page_layout(
        &self,
        row: PageLayoutRow,
    ) -> Result<(), StorageError>;
}
```

### 7.4 `TransactionStore`

```rust
pub trait TransactionStore {
    async fn save_edit_transaction(
        &self,
        transaction: &EditTransaction,
    ) -> Result<(), StorageError>;

    async fn load_recent_transactions(
        &self,
        document_id: DocumentId,
        limit: usize,
    ) -> Result<Vec<StoredEditTransaction>, StorageError>;

    async fn save_undo_snapshot(
        &self,
        snapshot: &UndoSnapshotRow,
    ) -> Result<(), StorageError>;
}
```

### 7.5 `FtsStore`

```rust
pub trait FtsStore {
    async fn upsert_block_text(
        &self,
        document_id: DocumentId,
        block_id: BlockId,
        kind: &RichBlockKind,
        plain_text: &str,
    ) -> Result<(), StorageError>;

    async fn delete_block_text(
        &self,
        block_id: BlockId,
    ) -> Result<(), StorageError>;

    async fn search(
        &self,
        document_id: DocumentId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FtsSearchResult>, StorageError>;
}
```

---

## 8. Runtime 接入流程

### 8.1 冷启动流程

```mermaid
flowchart TD
    A[Open Document] --> B[Acquire PostgreSQL Connection]
    B --> C[Run Migrations]
    C --> D[Load Document Metadata]
    D --> E[Try Load DocumentIndexSnapshot]
    E -->|Hit and Valid| F[Build DocumentIndex]
    E -->|Missing or Stale| G[Load blocks table]
    G --> F
    F --> H[Load layout cache]
    H --> I[Build PageLayoutIndex]
    I --> J[Create DocumentRuntime]
    J --> K[Plan initial render window]
    K --> L[Load PayloadWindow for visible blocks]
```

验收：

- 不加载所有 payload。
- snapshot 失效时能从 `blocks` 重建。
- layout cache 缺失时使用估算高度。

### 8.2 PayloadWindow 加载流程

```mermaid
flowchart TD
    A[WindowPlanner emits block range] --> B[Convert to block ids]
    B --> C[PayloadWindow checks missing ids]
    C --> D[Create load request generation]
    D --> E[DB worker batch load payloads]
    E --> F[Return payloads]
    F --> G{Generation still current?}
    G -->|Yes| H[Apply to PayloadWindow]
    G -->|No| I[Drop stale result]
```

验收：

- 快速滚动时旧请求不会覆盖新窗口。
- 当前编辑 block payload 始终 pin。
- selection endpoint block payload 可优先加载。

### 8.3 编辑保存流程

```mermaid
flowchart TD
    A[User Edit] --> B[DocumentRuntime apply EditTransaction]
    B --> C[Update memory model]
    C --> D[Mark dirty blocks]
    D --> E[Enqueue persistence task]
    E --> F[DB worker]
    F --> G[PostgreSQL BEGIN]
    G --> H[Write edit_transactions]
    H --> I[Write blocks / payloads / attrs]
    I --> J[Write undo snapshots if needed]
    J --> K[Enqueue search index update task]
    K --> L[COMMIT]
    L --> M[Mark clean]
    M --> N[Background FTS worker applies versioned update]
```

失败策略：

- PostgreSQL transaction 失败则 rollback。
- `OptimisticPersistenceManager` 标记 `SaveFailed`。
- affected blocks 加 pin。
- task 进入 retry queue。

---

## 9. Persistence Worker 设计

### 9.1 为什么需要 worker

单字符输入不能同步等待 PostgreSQL。

因此需要：

```txt
DocumentRuntime -> PersistenceCommand -> Async Persistence Task -> PostgreSQL
```

### 9.2 command 类型

```rust
pub enum PersistenceCommand {
    SaveTransaction(EditTransaction),
    SaveBlockLayouts(Vec<BlockLayoutRow>),
    SavePageLayouts(Vec<PageLayoutRow>),
    LoadPayloadWindow(PayloadWindowRequest),
    UpdateFts(Vec<FtsUpdate>),
    SaveRuntimeSnapshot(RuntimeSnapshot),
    ShutdownFlush,
}
```

### 9.3 worker 要求

- 写入任务可 batch。
- layout cache 写入可 debounce。
- transaction 保存优先级高于 layout cache。
- shutdown 时 flush dirty transaction。
- 失败任务进入内存队列；当 PostgreSQL / 网络恢复可写时再镜像到 `persistence_queue` 或 sync outbox。
- 如果 PostgreSQL 完全不可达，dirty block 仍保留在内存并 pin，close guard 必须阻止静默关闭或提示保存到本地 recovery file。

---

## 10. Migration 系统

### 10.1 规则

- 每个 migration 有 version、name、SQL、checksum。
- migration 在 PostgreSQL transaction 中执行。
- migration 成功后写 `schema_migrations`。
- 已执行版本不重复执行。
- checksum 不一致时报错。

### 10.2 v1 migration 内容

v1 创建：

- `schema_migrations`
- `workspaces`
- `documents`
- `document_tree`
- `blocks`
- `block_attrs`
- `block_payloads`
- `block_code_meta`
- `block_tables`
- `block_assets`
- `asset_blobs`
- `block_layout`
- `page_layout`
- `document_index_snapshot`
- `edit_transactions`
- `undo_snapshots`
- `persistence_queue`
- `runtime_snapshots`
- `sync_outbox`
- `sync_state`
- `remote_tombstones`
- `block_fts`
- all indexes

---

## 11. Crash Recovery

### 11.1 启动恢复流程

```mermaid
flowchart TD
    A[Open DB] --> B[Run migrations]
    B --> C[Load pending persistence_queue]
    C --> D[Retry safe tasks]
    D --> E[Load runtime_snapshot]
    E --> F[Load document index]
    F --> G[Validate structure_version]
    G --> H[Recover dirty blocks]
```

### 11.2 恢复规则

- 已提交 PostgreSQL transaction 的数据可信。
- `persistence_queue.state = failed/pending` 的任务需要重试或提示用户。
- `runtime_snapshots` 只能恢复 focus / selection / scroll / dirty pin 等运行时上下文，不作为正文真相。
- `runtime_snapshots` 禁止保存正文 payload，禁止覆盖 `blocks` / `block_payloads`。
- transaction log 可用于 debug/replay，不强制每次启动 replay。

---

## 12. FTS 设计

### 12.1 更新时机

- 正文 transaction 只 enqueue FTS task，不直接依赖 FTS 成功。
- payload 保存成功后后台更新。
- block 删除成功后删除。
- batch paste 后批量更新。
- FTS 更新失败不影响正文 transaction，但记录 repair task。
- FTS update 必须比较 `content_version`，过期任务直接丢弃。

### 12.2 搜索结果

搜索返回：

```rust
pub struct FtsSearchResult {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub score: f64,
    pub snippet: String,
}
```

搜索跳转流程：

```txt
Search result block_id -> DocumentIndex index_of -> VirtualScrollTarget -> scroll_to_target
```

不需要 UI entity。

---

## 13. Asset / Media 设计

### 13.1 保存策略

- DB 保存 metadata。
- 二进制文件默认保存到 workspace asset directory。
- DB 记录 checksum / local_path / ref_count。
- 图片 decode / thumbnail 缓存在单独目录。

### 13.2 Stable Layout Box

图片等资源加载前必须有稳定布局盒：

- `block_assets.stable_estimated_height`
- `block_assets.stable_min_height`
- `block_assets.stable_max_height`
- `block_assets.stable_confidence`
- `block_layout.estimated_height`

资源加载完成后：

- 更新 asset metadata。
- 更新 block layout。
- 通过 height correction 修正滚动。

---

## 14. Database / Collection Block 设计

总架构支持 Notion-like Database block。数据库方案必须提供 collection 级存储，而不是把 database block 简化成普通 payload JSON。

### 14.1 `collections`

```sql
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id)
);
```

### 14.2 `collection_properties`

```sql
CREATE TABLE collection_properties (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    name TEXT NOT NULL,
    property_type TEXT NOT NULL,
    config_json TEXT NOT NULL,
    sort_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY(collection_id) REFERENCES collections(id)
);
```

### 14.3 `collection_views`

```sql
CREATE TABLE collection_views (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    view_type TEXT NOT NULL,
    name TEXT NOT NULL,
    config_json TEXT NOT NULL,
    sort_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY(collection_id) REFERENCES collections(id)
);
```

### 14.4 `collection_rows` / `collection_cells`

```sql
CREATE TABLE collection_rows (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    sort_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY(collection_id) REFERENCES collections(id)
);

CREATE TABLE collection_cells (
    row_id TEXT NOT NULL,
    property_id TEXT NOT NULL,
    value_json TEXT NOT NULL,
    plain_text TEXT NOT NULL DEFAULT '',
    content_version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(row_id, property_id),
    FOREIGN KEY(row_id) REFERENCES collection_rows(id),
    FOREIGN KEY(property_id) REFERENCES collection_properties(id)
);
```

### 14.5 `database_block_bindings`

```sql
CREATE TABLE database_block_bindings (
    block_id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    view_id TEXT,
    title TEXT,
    FOREIGN KEY(block_id) REFERENCES blocks(id),
    FOREIGN KEY(collection_id) REFERENCES collections(id),
    FOREIGN KEY(view_id) REFERENCES collection_views(id)
);
```

---

## 15. Decoration / Annotation 持久化策略

- 默认 decoration 不进 DB，例如 search highlight、spellcheck、临时 overlay。
- 用户创建的持久化 comment / annotation / suggestion 进入 annotation 表。
- `PaintOnly` decoration 不改变 layout cache key。
- `LayoutAffecting` decoration 必须提升 style/layout version，使相关 block layout cache 失效。

第一版 annotation 表可后置，但实现 decoration 时不能把运行时 overlay 误当正文 payload。

---

## 16. 错误处理

### 14.1 `StorageError`

建议错误类型：

```rust
pub enum StorageError {
    Postgres(String),
    Migration(String),
    SchemaVersionMismatch { expected: u32, found: u32 },
    NotFound { entity: &'static str, id: String },
    CorruptData { message: String },
    Serialization(String),
    Io(String),
    Busy,
    RetryExhausted { task_id: String },
}
```

### 14.2 错误分级

| 错误 | 处理 |
|---|---|
| payload 加载失败 | block 显示 error placeholder |
| layout cache 失败 | 使用估算高度 |
| FTS 写失败 | 加 repair task |
| transaction 写失败 | dirty block pin + retry |
| migration 失败 | 启动失败，提示用户 |
| schema checksum mismatch | 启动失败，要求修复 |

---

## 17. 云服务演进设计

### 17.1 总体策略

采用 local-first 架构：

```txt
Client DocumentRuntime
  -> pending transaction / sync_outbox
  -> Async persistence task
  -> Cloud API or direct service layer
  -> PostgreSQL via sqlx
```

PostgreSQL 是云端权威存储，负责跨设备同步、备份、分享、权限和团队能力。客户端如需离线能力，可后置实现本地 cache/outbox，但不再作为主数据库方案。

### 17.2 客户端访问 PostgreSQL 的边界

- GPUI 主线程不能直接 await 远程 DB。
- 所有 PostgreSQL 访问走 async store / persistence task。
- `DocumentRuntime` 先更新内存 truth，再后台保存。
- 网络失败时保留 pending transaction，并 pin dirty blocks。
- 如需离线编辑，后续增加本地 cache/outbox，但它不是本文主数据库。

### 17.3 服务端建议

后续新增独立 workspace / crate：

```txt
crates/cditor-server
  src/api
  src/auth
  src/sync
  src/storage/postgres
  src/search
```

服务端数据库建议 PostgreSQL，使用 `sqlx`：

- `workspaces`
- `documents`
- `blocks`
- `block_payloads`
- `edit_operations`
- `document_revisions`
- `users`
- `workspace_members`
- `permissions`
- `asset_objects`

### 17.4 同步协议预留

第一阶段不实现 CRDT，但 transaction log 必须带上：

- `client_id`
- `device_id`
- local `sequence`
- base version
- affected blocks
- serialized edit ops

后续可以演进：

```txt
Last-write-wins block payload sync
  -> operation transform
    -> CRDT / Yjs-like model if需要多人实时协同
```

### 17.5 冲突策略第一版

第一版云同步不做实时协同，采用保守策略：

- 同一 block 不同设备并发编辑时，服务端保留冲突副本。
- 客户端显示 conflict block 或 recovery view。
- 结构冲突优先保留两边 block，避免数据丢失。
- 服务端永远不因为冲突丢弃客户端 payload。

---

## 18. 性能预算

| 操作 | 预算 |
|---|---|
| open connection + migration check | < 50ms 常规情况 |
| load 10w block index | 可后台化，首屏优先 |
| load payload window | batch 查询，避免 N+1 |
| save single char transaction | 不阻塞 UI |
| save layout cache | debounce/batch |
| FTS update | background |
| snapshot write | idle priority |

---

## 19. 测试计划

### 16.1 migration tests

- 新 DB 创建所有表。
- 重复 migration 幂等。
- checksum mismatch 报错。
- migration 失败 rollback。

### 16.2 document store tests

- 保存 / 加载 document metadata。
- 保存 / 加载 10w block index。
- soft delete block 不出现在 index。
- structure_version 不一致时 snapshot 失效。

### 16.3 payload store tests

- 按 block ids 批量加载 payload。
- 缺失 payload 返回错误或 placeholder 策略。
- RichText / Code / Table / Image payload round trip。

### 16.4 layout store tests

- exact key 命中返回 exact。
- font/theme/width 变化降级。
- page layout structure_version 变化标 dirty。
- stale measured result 不覆盖新版本。

### 16.5 transaction store tests

- InsertText transaction round trip。
- SplitBlock transaction round trip。
- Paste large blocks 使用 undo snapshot。
- PostgreSQL transaction rollback 不产生半写入。

### 16.6 persistence tests

- 保存失败进入 queue。
- retry 成功后清除 failed state。
- close guard 发现 dirty/saving/failed blocks。

### 16.7 FTS tests

- block text upsert。
- block delete 后搜索不到。
- CJK / emoji 基本搜索可用。

### 16.8 integration tests

- 从 DB 打开 demo document。
- 编辑后保存再重新打开。
- 10w blocks 冷启动不全量加载 payload。
- crash recovery 恢复 pending queue。

---

## 20. 实施任务清单

> 规则：每完成一个任务，必须跑对应验证，并把 `[ ]` 改成 `[x]`。不要跳过失败任务继续做后续依赖任务。

### DB-Phase 0：本地 PostgreSQL 开发环境

- [x] DB-000A 新增 `docker-compose.yml`，包含 `postgres` 与 `postgres_test`。
- [x] DB-000B 新增 `.env.example`，提供开发库和测试库连接串。
- [x] DB-000C `.gitignore` 忽略本地 `.env`。
- [x] DB-000D 新增 `migrations/0001_initial.sql` 初始 schema 草案。

验收：

```sh
docker compose up -d
docker compose ps
# postgres / postgres_test 应为 healthy
```

### DB-Phase 1：依赖与模块骨架

- [x] DB-001 添加 `sqlx` / `tokio` / `serde` / `serde_json` 依赖。
- [x] DB-002 新建 `src/storage/postgres` 模块目录。
- [x] DB-003 新增 `postgres/mod.rs` 并在 `storage/mod.rs` 导出。
- [x] DB-004 实现 `StorageError`。
- [x] DB-005 实现 PostgreSQL `PgPool` opener。
- [x] DB-006 实现连接池配置与健康检查。
- [x] DB-007 添加 connection smoke test。

验收：

```sh
cargo test storage::postgres
cargo check
```

### DB-Phase 2：Migration 系统

- [x] DB-008 使用 `sqlx::migrate!` 接入 `migrations/`。
- [x] DB-009 实现 migration runner。
- [x] DB-010 实现 migration metadata / version 常量。
- [x] DB-011 添加不连库的 migration source smoke test。
- [x] DB-012 添加需要 Docker PostgreSQL 的 ignored migration integration test。
- [x] DB-013 实现 repeated migration 幂等测试。

验收：

```sh
cargo test migration
```

### DB-Phase 3：Schema v1

- [x] DB-014 编写完整 schema v1 SQL。
- [x] DB-015 创建 workspace/document/document_tree 表。
- [x] DB-016 创建 blocks/block_attrs/block_payloads 表。
- [x] DB-017 创建 code/table/asset 表。
- [x] DB-018 创建 block_layout/page_layout 表。
- [x] DB-019 创建 snapshot/transaction/queue 表。
- [x] DB-020 创建 block_search `tsvector` 表与 GIN 索引。
- [x] DB-021 添加所有必要 index。
- [x] DB-022 添加 schema smoke test：所有表存在。
- [x] DB-022A 创建 collection/database block 相关表。
- [x] DB-022B 创建 sync_outbox / sync_state / remote_tombstones 云同步预留表。

验收：

```sh
cargo test schema_v1
```

### DB-Phase 4：类型转换层

- [x] DB-023 定义 DB row 类型。
- [x] DB-024 实现 `DocumentId` / `BlockId` 与 DB UUID 转换。
- [x] DB-025 实现 `RichBlockKind` kind tag 转换。
- [x] DB-026 实现 `BlockAttrs` JSON 序列化。
- [x] DB-027 实现 `BlockPayload` JSON 序列化。
- [x] DB-028 实现 `EditTransaction` JSON 序列化。
- [x] DB-029 添加 payload round-trip tests。

验收：

```sh
cargo test postgres_types
```

### DB-Phase 5：Document / Block Store

- [x] DB-030 实现 `PostgresDocumentStore::save_document_metadata`。
- [x] DB-031 实现 `PostgresDocumentStore::load_document_metadata`。
- [x] DB-032 实现保存 block structure index。
- [x] DB-033 实现加载 block structure index。
- [x] DB-034 实现 soft delete block。
- [x] DB-035 实现 structure_version 更新。
- [x] DB-036 实现 `DocumentIndexStore` for PostgreSQL。
- [x] DB-037 添加 10w blocks index load test。

验收：

```sh
cargo test document_store
```

### DB-Phase 6：Payload Store

- [x] DB-038 实现 `load_block_payloads(block_ids)`。
- [x] DB-039 实现 `save_block_payloads(payloads)`。
- [x] DB-040 实现 batch query，避免 N+1。
- [x] DB-041 实现 missing payload 策略。
- [x] DB-042 实现 RichText / Code / Table / Image round trip。
- [x] DB-043 实现 plain_text 同步。

验收：

```sh
cargo test payload_store
```

### DB-Phase 7：Layout Cache PostgreSQL Store

- [x] DB-044 实现 `PostgresLayoutCacheStore`。
- [x] DB-045 实现 block layout save/load。
- [x] DB-046 实现 page layout save/load。
- [x] DB-047 实现 exact match / historical fallback。
- [x] DB-048 实现 stale content_version 保护。
- [x] DB-049 接入 `height_write_debounce`。

验收：

```sh
cargo test layout_store
```

### DB-Phase 8：Transaction Store

- [x] DB-050 实现 `save_edit_transaction`。
- [x] DB-051 实现 `load_recent_transactions`。
- [x] DB-052 实现 `undo_snapshots` 保存。
- [x] DB-053 实现 large paste snapshot 策略。
- [x] DB-054 实现 PostgreSQL transaction 原子写入。
- [x] DB-055 添加 rollback 测试。

验收：

```sh
cargo test transaction_store
```

### DB-Phase 9：Persistence Queue / Worker

- [x] DB-056 实现 `persistence_queue` CRUD。
- [x] DB-057 实现 failed / retry state transition。
- [x] DB-058 实现 worker command enum。
- [x] DB-059 实现 worker thread loop。
- [x] DB-060 实现 shutdown flush。
- [x] DB-061 接入 `OptimisticPersistenceManager`。
- [x] DB-062 添加保存失败恢复测试。

验收：

```sh
cargo test persistence_queue
```

### DB-Phase 10：Runtime Cold Start 接入

- [x] DB-063 实现 `DocumentRuntime::from_store`。
- [x] DB-064 启动加载 document metadata。
- [x] DB-065 启动加载 `DocumentIndex`。
- [x] DB-066 启动加载 layout cache。
- [x] DB-067 首屏加载 payload window。
- [x] DB-068 snapshot 命中时使用 `document_index_snapshot`。
- [x] DB-069 snapshot stale 时从 blocks 重建。

验收：

```sh
cargo test runtime_from_store
```

### DB-Phase 11：PayloadWindow 异步加载

- [x] DB-070 实现 payload window load request。
- [x] DB-071 实现 generation 过期丢弃。
- [x] DB-072 实现 focused block priority load。
- [x] DB-073 实现 selection endpoint priority load。
- [x] DB-074 实现 loading / error placeholder。

验收：

```sh
cargo test payload_window_store
```

### DB-Phase 12：FTS

- [x] DB-075 实现 FTS upsert。
- [x] DB-076 实现 FTS delete。
- [x] DB-077 实现 FTS search。
- [x] DB-078 transaction 保存后 enqueue FTS 更新任务。
- [x] DB-079 FTS 失败进入 repair task。
- [x] DB-079A FTS 过期 content_version 更新必须丢弃。

验收：

```sh
cargo test fts_store
```

### DB-Phase 13：Asset Store

- [x] DB-080 实现 `block_assets` CRUD。
- [x] DB-081 实现 `asset_blobs` registry。
- [x] DB-082 实现 checksum 去重。
- [x] DB-083 实现 ref_count。
- [x] DB-084 实现 stable layout box 保存。
- [x] DB-085 实现 asset cleanup candidate query。

验收：

```sh
cargo test asset_store
```

### DB-Phase 14：Crash Recovery

- [x] DB-086 实现 `runtime_snapshots` save/load。
- [x] DB-087 启动加载 pending queue。
- [x] DB-088 启动 retry safe tasks。
- [x] DB-089 dirty block 恢复 pin。
- [x] DB-090 corrupt snapshot fallback。

验收：

```sh
cargo test crash_recovery
```

### DB-Phase 15：云同步预留

- [x] DB-091 实现 sync_outbox 本地写入。
- [x] DB-092 每个本地 EditTransaction 生成 outbox 记录。
- [x] DB-093 实现 sync_state sequence 更新。
- [x] DB-094 实现 remote_tombstones 写入。
- [x] DB-095 添加 outbox crash recovery 测试。

验收：

```sh
cargo test sync_outbox
```

### DB-Phase 16：端到端集成

- [x] DB-096 demo document 保存到 PostgreSQL。
- [x] DB-097 从 PostgreSQL 打开 demo document。
- [x] DB-098 编辑后保存并重新打开。
- [x] DB-099 10w blocks 打开不加载全量 payload。
- [x] DB-100 layout cache 跨重启复用。
- [x] DB-101 paste large document 后 transaction/undo snapshot 正常。
- [x] DB-102 PostgreSQL search 结果可跳转 block。
- [x] DB-103 保存失败后 close guard 生效。

验收：

```sh
cargo test postgres_integration
cargo check
```

---

## 20. 推荐执行顺序

严格按顺序执行：

```txt
DB-Phase 1  依赖与模块骨架
DB-Phase 2  Migration 系统
DB-Phase 3  Schema v1
DB-Phase 4  类型转换层
DB-Phase 5  Document / Block Store
DB-Phase 6  Payload Store
DB-Phase 7  Layout Cache Store
DB-Phase 8  Transaction Store
DB-Phase 9  Persistence Queue / Worker
DB-Phase 10 Runtime Cold Start
DB-Phase 11 PayloadWindow 异步加载
DB-Phase 12 FTS
DB-Phase 13 Asset Store
DB-Phase 14 Crash Recovery
DB-Phase 15 端到端集成
```

不要先做 GUI 接入。数据库层必须先能被测试独立验证。

---

## 21. 与富文本编辑迁移的依赖关系

数据库实现会影响后续富文本编辑迁移：

| 编辑能力 | DB 依赖 |
|---|---|
| Enter split block | transaction store + block structure save |
| Backspace merge block | transaction store + block delete/update |
| paste markdown blocks | payload store + transaction store + undo snapshot |
| document-level selection copy | block payload window load |
| table edit | payload store + table metadata |
| image paste | asset store + payload store |
| undo large paste | undo_snapshots |
| crash recovery | persistence_queue + runtime_snapshots |

因此数据库层不能设计成后补模块，必须和编辑事务一起推进。

---

## 22. 文档维护规则

- 每完成一个 DB task，必须把对应 `[ ]` 改成 `[x]`。
- 如果实现中发现 schema 需要调整，必须先更新本文件。
- 如果跳过某个任务，必须在任务下说明原因。
- 所有 DB task 必须有测试或明确无法测试的说明。
- 不允许为了通过测试删除设计要求。
