# Cditor SQLite / PostgreSQL 双后端设计方案

> 状态：Selectable 模式实施中，LocalFirst 模式保留为后续阶段
>
> 日期：2026-07-14
>
> 目标：在不破坏 10 万 Block 大文档架构的前提下，让同一套 Cditor SDK 可以选择 SQLite 或 PostgreSQL，并为后续 SQLite 本地持久化 + PostgreSQL 云同步预留正确边界。

## 0. 结论

可以支持，而且 SQLite 很适合单机、本地文档和离线场景。但这不是只给 `CditorBackend` 增加一个 enum variant 的小改动。

当前 PostgreSQL 已经穿透到 SDK cold start、GUI persistence、payload window loader、render 调度和 cache policy。正确做法是先建立后端无关的 `DocumentStorage` 边界，再实现 SQLite adapter。

建议分成两个产品能力：

1. **Selectable 模式**：每个编辑器实例选择 SQLite 或 PostgreSQL，二者使用相同 runtime 和 SDK。
2. **LocalFirst 模式**：SQLite 负责本地可靠落盘和 outbox，PostgreSQL 负责云端权威数据与多端同步。

第一阶段只实现 Selectable 模式。不要把两个数据库放进同一次保存流程做朴素双写；SQLite 成功而 PostgreSQL 失败时，会产生无法可靠判定的双真相。

改动规模判断：

| 范围 | 规模 | 说明 |
| --- | --- | --- |
| 只增加 SQLite store，不接 GUI | 中等 | 新 crate、migration、store contract tests |
| SDK 可选择并完整读写 SQLite/PostgreSQL | 中到大型 | 需要重构 cold start、autosave、payload loader 和 diagnostics |
| SQLite + PostgreSQL 自动同步 | 大型 | 还需要 outbox、ack、pull、冲突处理和全局稳定 ID |

SQLite 本身不会成为主要性能问题。对于单机读取，它通常比远端 PostgreSQL 延迟更低。真正的风险是单写者竞争、过长事务、逐 Block 查询、全量 payload hydrate、WAL 不受控增长，以及在 UI 线程执行数据库工作。

## 1. 与总体架构的关系

本方案遵守 `large-document-rich-text-architecture.md` 中的以下边界：

- `DocumentRuntime` 是编辑时的内存真相。
- UI 只消费 runtime projection，不直接访问数据库。
- 结构索引与重 payload 分离。
- 冷启动不读取 10 万 Block 的全部 payload。
- payload 按 viewport/window 批量加载。
- 编辑先更新内存，再异步持久化。
- dirty payload 在确认落盘前保持 pin。
- layout cache、FTS 等派生数据可以重建，正文不可丢失。
- PostgreSQL 继续作为云端、团队、权限、审计和服务端搜索的权威存储。

新增 SQLite 后，存储角色定义如下：

```text
Memory mode:
  DocumentRuntime 是唯一真相，退出即丢失。

SQLite standalone mode:
  SQLite 是单机文档的持久化真相。
  DocumentRuntime 是当前编辑会话的内存真相。

PostgreSQL mode:
  PostgreSQL 是当前持久化真相。
  DocumentRuntime 仍先应用编辑，再后台保存。

LocalFirst mode（后续阶段）:
  SQLite 保存本地正文、pending transaction 和 outbox。
  PostgreSQL 保存服务端 revision、团队空间和跨设备权威状态。
  DocumentRuntime 不直接对任一数据库做同步调用。
```

## 2. 当前实现评估

### 2.1 已有可复用能力

- `cditor-core` 已定义 Block、payload、attrs、transaction 和 document index 类型。
- `cditor-storage` 已包含 layout cache、optimistic persistence、recovery 等后端无关逻辑。
- `cditor-storage-postgres` 已有完整 schema、类型转换和各类 store。
- runtime 已实现 document index cold start、初始 payload window、payload cache 和虚拟滚动。
- SDK 已有 `CditorBuilder`、统一错误、save status、close guard 和 diagnostics。
- PostgreSQL 保存已经是 debounce + background task，不在键盘输入路径同步写数据库。

### 2.2 当前具体耦合

以下位置仍然直接知道 PostgreSQL：

| 位置 | 当前耦合 | 需要改成 |
| --- | --- | --- |
| `crates/app/src/api/options.rs` | `CditorBackend::Postgres*` 和 `PgPool` | 后端配置 enum + backend factory |
| `crates/app/src/api/cold_start.rs` | 直接构造三个 Postgres store | `Arc<dyn DocumentStorage>` |
| `crates/app/src/api/cditor.rs` | `block_on_postgres`、Postgres timeout/target | 通用 storage task 和 target |
| `gui/persistence/postgres_saver.rs` | batch、state、outcome 都以 Postgres 命名 | 通用 persistence coordinator |
| `gui/app/persistence_bridge.rs` | 直接创建 `PostgresPayloadStore` | 通过 storage session load window |
| `gui/app/render.rs` | 从 `PgPool` 判断能否加载 payload | 查询 storage capabilities/session |
| `gui/app/lifecycle.rs` | 持有 `PostgresPersistenceState` | 持有 `EditorPersistenceState` |
| `runtime/content/payload_cache.rs` | `postgres_default()` | `persistent_backend_default()` |

更关键的是，`crates/store/src/traits.rs` 当前只重新导出了简化的 `DocumentIndexStore`，还没有能够覆盖 cold start、payload window 和原子保存的公共 trait。

### 2.3 现有文档需要澄清的地方

`database-implementation-plan.md` 的产品结论是 PostgreSQL，但部分 schema 示例使用了 SQLite 风格的 `TEXT / INTEGER / BLOB`。双后端落地后应明确：

- 共享的是逻辑模型、序列化格式和 store contract。
- PostgreSQL 与 SQLite 使用各自独立 migration。
- 不尝试维护一份同时兼容两种 SQL 方言的 schema 文件。
- PostgreSQL 的 `JSONB / UUID / TIMESTAMPTZ / TSVECTOR` 与 SQLite 的 `TEXT / BLOB / INTEGER / FTS5` 分别实现。

## 3. 支持模式

### 3.1 Selectable 模式，第一阶段

一个 `CditorComponent` 在其生命周期内只连接一个持久化后端：

```text
CditorBuilder
  -> Memory
  -> SQLite file
  -> SQLite injected backend
  -> PostgreSQL URL
  -> PostgreSQL pool
```

后端在 build 时确定，打开文档后不支持原地切换。需要迁移数据时，通过显式 import/copy API 创建新 session，不能替换活跃 runtime 下的 store。

### 3.2 LocalFirst 模式，后续阶段

LocalFirst 不是两个 store 的简单组合调用，而是一个独立的同步后端：

```text
DocumentRuntime
  -> SQLite transaction: content + edit transaction + outbox
  -> local save acknowledged
  -> background sync worker
  -> PostgreSQL API/store
  -> server revision / ack
  -> SQLite sync_state update
```

远端拉取也必须先进入 SQLite transaction，再由 versioned change set 更新 runtime。这样 crash 后 SQLite 中仍有完整的本地状态、未发送操作和服务端 ack 位置。

### 3.3 第一阶段非目标

- 不实现 SQLite 与 PostgreSQL 实时双向同步。
- 不实现多进程同时编辑同一个 SQLite 文件。
- 不实现 SQLCipher；先通过文件权限保护本地数据库。
- 不保证 PostgreSQL FTS 和 SQLite FTS5 的 score 数值相同，只保证结果协议一致。
- 不让 SDK 在一个已打开组件上热切换数据库。

## 4. 目标依赖边界

```text
cditor-app
  -> cditor-runtime
  -> cditor-storage
  -> backend factory only:
       cditor-storage-postgres
       cditor-storage-sqlite

cditor-runtime
  -> cditor-storage traits / DTO
  -> cditor-core

cditor-storage-postgres
  -> cditor-storage
  -> cditor-core
  -> sqlx postgres

cditor-storage-sqlite
  -> cditor-storage
  -> cditor-core
  -> sqlx sqlite
```

必须禁止：

```text
GUI View -> PgPool / SqlitePool
render() -> SQL query
runtime -> PostgreSQL concrete store
SQLite store -> GPUI
PostgreSQL codec 与 SQLite codec 各自复制一套 core 序列化规则
```

## 5. 模块设计

### 5.1 `cditor-storage` 公共层

建议新增：

```text
crates/store/src/
  backend.rs
  error.rs
  document_store.rs
  payload_store.rs
  persistence_store.rs
  storage_session.rs
  dto/
    mod.rs
    cold_start.rs
    save_batch.rs
    diagnostics.rs
  codec/
    mod.rs
    attrs.rs
    payload.rs
    transaction.rs
```

职责：

- 定义 object-safe async store contract。
- 定义 cold start、payload batch、save batch 和 outcome DTO。
- 提供统一 `StorageError` 和错误分级。
- 承载 PostgreSQL/SQLite 共用的 core 类型序列化。
- 不依赖 GPUI、sqlx、PgPool 或 SqlitePool。

### 5.2 `cditor-storage-postgres`

保留当前 store 实现，但增加一个聚合 adapter：

```text
PostgresDocumentStorage
  -> PostgresDocumentStore
  -> PostgresPayloadStore
  -> PostgresLayoutCacheStore
  -> PostgresTransactionStore
```

原有公共 store 可以继续保留；应用层只能依赖聚合后的公共 contract。

### 5.3 新增 `cditor-storage-sqlite`

```text
crates/store-sqlite/
  Cargo.toml
  migrations/
    0001_initial.sql
  src/
    lib.rs
    config.rs
    connection.rs
    error.rs
    migrations.rs
    storage.rs
    stores/
      document.rs
      payload.rs
      layout.rs
      transaction.rs
      search.rs
      recovery.rs
    tests/
      contract.rs
      migration.rs
      concurrency.rs
```

所有文件保持单一职责；store 实现接近 700 行时继续按 metadata/index/snapshot 或 read/write 拆分。

### 5.4 App/GUI 通用协调层

建议把：

```text
postgres_saver.rs
PostgresPersistenceState
PostgresPersistenceTarget
flush_postgres_persistence
load_postgres_payload_window
```

改为：

```text
persistence/coordinator.rs
EditorPersistenceState
StorageSession
flush_persistence
load_payload_window
```

`StorageSession` 内部持有 `Arc<dyn DocumentStorage>` 和当前 `DocumentId`。GUI 只判断 session 是否具备 `payload_window`、`autosave` 等 capability，不判断具体数据库类型。

## 6. 公共 Store Contract

第一版不需要把每个 PostgreSQL 小 store 都做成 trait。应用真正需要的是三个高层原语：冷启动、payload window 和原子保存。

建议接口：

```rust
#[async_trait::async_trait]
pub trait DocumentStorage: Send + Sync {
    fn backend_kind(&self) -> StorageBackendKind;
    fn capabilities(&self) -> StorageCapabilities;

    async fn initialize(&self) -> Result<(), StorageError>;

    async fn load_document(
        &self,
        request: LoadDocumentRequest,
    ) -> Result<StoredDocumentColdStart, StorageError>;

    async fn load_payloads(
        &self,
        request: LoadPayloadsRequest,
    ) -> Result<LoadedPayloadBatch, StorageError>;

    async fn commit(
        &self,
        batch: StorageSaveBatch,
    ) -> Result<StorageSaveOutcome, StorageError>;

    async fn flush(&self) -> Result<(), StorageError>;

    async fn diagnostics(&self) -> Result<StorageDiagnostics, StorageError>;
}
```

Contract 约束：

- `load_document` 返回 storage DTO，不返回 `DocumentRuntime`，保持 storage 不依赖 runtime。
- `commit` 必须保证正文、结构、attrs、transaction 和必要 outbox 的后端内原子性。
- `StorageSaveOutcome` 携带实际保存的 structure/content version。
- 旧版本保存成功不能把更新版本标记为 clean。
- `load_payloads` 必须 batch；缺失 ID 明确返回，不能静默生成空 payload。
- layout/search 失败可降级，document/payload/transaction 失败不可伪装成功。
- `flush` 等待已入队的可靠写操作，不要求等待可重建的低优先级 layout/FTS 工作。

能力模型建议：

```rust
pub struct StorageCapabilities {
    pub persistent: bool,
    pub payload_window: bool,
    pub full_text_search: bool,
    pub cloud_sync: bool,
    pub multi_process_read: bool,
    pub server_authoritative: bool,
}
```

## 7. SQLite 连接与并发模型

### 7.1 驱动选择

第一版建议继续使用 `sqlx 0.8` 的 SQLite feature，理由：

- 与 PostgreSQL 使用同一 async/error/migration 工具链。
- 现有项目已经依赖 SQLx 和 Tokio。
- SQLx SQLite connection worker 不要求在 GPUI 主线程执行阻塞 sqlite3 调用。
- contract test 可以复用相同 async 测试结构。

不建议第一版同时引入 `rusqlite`，否则还需要维护两种连接、事务和错误模型。只有在 benchmark 证明 SQLx SQLite 成为瓶颈时，再评估专用 rusqlite worker。

### 7.2 单写者、多读者

SQLite 即使开启 WAL 也只有一个 writer。建议：

```text
StoragePersistenceCoordinator
  -> 单一有界 write queue
  -> dedicated writer connection
  -> BEGIN IMMEDIATE
  -> batch commit

Payload loaders / search
  -> 2-4 read connections
  -> WAL snapshot reads
```

不要让 autosave、layout cache、FTS 和 recovery worker 各自争抢写锁。优先级：

```text
正文/结构 transaction
  > recovery/outbox
  > payload metadata
  > layout cache
  > FTS repair/maintenance
```

### 7.3 默认 PRAGMA

由 `SqliteConnectOptions` 和每连接初始化 hook 设置：

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
PRAGMA wal_autocheckpoint = 1000;
```

说明：

- `FULL` 是本地正文默认 durability，降低掉电后已报告保存成功的数据丢失概率。
- 可提供 `SqliteDurability::Balanced` 映射到 `NORMAL`，但必须由宿主显式选择。
- `cache_size`、`mmap_size` 和 page size 先通过 benchmark 决定，不写死激进值。
- `journal_mode` 必须检查实际返回值；无法进入 WAL 时返回明确错误或使用受测试的 fallback。
- writer 使用 `BEGIN IMMEDIATE`，尽早暴露锁冲突，避免事务写到中途才失败。

### 7.4 文件和进程边界

- build 时规范化路径并创建父目录。
- 数据库文件默认使用 `.cditor.db` 后缀。
- 一个进程内相同 canonical path 复用同一 backend/session registry。
- 第一阶段不支持两个 Cditor 进程同时编辑同一文件。
- 检测到持续 `SQLITE_BUSY` 时进入 `StorageError::Busy`，不能无限重试卡住关闭流程。
- 备份使用 SQLite backup API 或 `VACUUM INTO`；数据库打开时不能只复制主 `.db` 文件而忽略 WAL。

## 8. SQLite Schema 策略

### 8.1 逻辑表保持一致

第一阶段必须覆盖当前 GUI 真正使用的最小可靠集合：

- `schema_migrations_meta`
- `workspaces`
- `documents`
- `blocks`
- `block_attrs`
- `block_payloads`
- `block_layout`
- `page_layout`
- `document_index_snapshot`
- `edit_transactions`
- `persistence_queue`
- `runtime_snapshots`

若第一阶段声明 FTS、asset、sync capability，则对应表必须同时实现；不能只建空表并返回 `Unsupported`。

### 8.2 类型映射

| 逻辑类型 | PostgreSQL | SQLite |
| --- | --- | --- |
| 全局 ID | `UUID` | 16-byte `BLOB`，由 Rust 生成/校验 |
| JSON | `JSONB` | UTF-8 `TEXT`，写入前由 serde 校验 |
| 时间 | `TIMESTAMPTZ` | UTC microseconds `INTEGER` |
| bool | `BOOLEAN` | `INTEGER CHECK(value IN (0, 1))` |
| binary | `BYTEA` | `BLOB` |
| u64 version | checked `BIGINT` | checked signed `INTEGER` |
| FTS | `tsvector + GIN` | FTS5 virtual table |

不能把超出 `i64` 的 Rust version 直接 cast；两个后端都必须返回 `StorageError::VersionOutOfRange`。

### 8.3 ID 决策

Selectable 模式可以暂时沿用 runtime 的 `u64 DocumentId/BlockId` 并做确定性 UUID 映射，保证与现有 PostgreSQL 数据兼容。

LocalFirst/多端同步前必须完成全局稳定 UUID/ULID ID 迁移。不同设备独立生成 `u64` 会碰撞，不能作为同步协议最终 ID。

SQLite 中建议存 16-byte UUID BLOB，而不是十进制 `u64`：

- 与 PostgreSQL UUID 语义一致。
- 后续复制/同步不需要重新分配 ID。
- BLOB 索引比 UUID 文本更紧凑。

### 8.4 FTS5

SQLite 搜索通过单独 FTS5 virtual table 实现，不把 FTS 当正文真相：

- 正文 commit 先成功。
- 同 transaction 或 versioned repair task 更新 FTS。
- 搜索结果必须携带 `content_version`。
- 过期 FTS task 不得覆盖新文本。
- FTS 损坏或缺失时允许重建。

中文分词需要单独验收。`unicode61` 可以做基础 Unicode tokenization，但不能替代完整中文分词。可在 bundled SQLite 支持稳定后评估 FTS5 trigram，或沿用外部搜索服务。

### 8.5 Query 规则

- Block index 按 `document_id, sort_key` 流式读取。
- payload 查询按 ID batch，SQLite 每批建议最多 500，避免变量数量兼容问题。
- 保存 payload 使用 prepared statement + transaction，不逐行 commit。
- index snapshot 使用 versioned BLOB，miss/stale 时从 blocks 重建。
- 大图片、附件、视频和 whiteboard 大 blob 仍放文件/对象存储，SQLite 只存 metadata 和引用。

## 9. SDK 设计

### 9.1 推荐公共配置

```rust
pub enum CditorStorageBackend {
    Memory,
    Sqlite(SqliteOptions),
    Postgres(PostgresOptions),
    Custom(Arc<dyn DocumentStorage>),
    // Phase 2:
    LocalFirst(LocalFirstOptions),
}

pub struct SqliteOptions {
    pub path: PathBuf,
    pub create_if_missing: bool,
    pub durability: SqliteDurability,
    pub busy_timeout: Duration,
    pub read_connections: u8,
}
```

`CditorBackend` 可以逐步重命名为 `CditorStorageBackend`；保留 type alias 或兼容 variant，避免立即破坏已有宿主。

### 9.2 Builder 接口

```rust
let sqlite_editor = CditorBuilder::new()
    .with_document_id(42)
    .with_sqlite_path("./workspace.cditor.db")
    .with_autosave(2)
    .build(cx)?;

let postgres_editor = CditorBuilder::new()
    .with_document_id(42)
    .with_postgres_url(database_url)
    .with_autosave(2)
    .build(cx)?;
```

完整接口建议：

```rust
with_storage_backend(CditorStorageBackend)
with_sqlite_path(path)
with_sqlite_options(options)
with_postgres_url(url)          // 兼容保留
with_postgres_pool(pool)        // 兼容保留
with_custom_storage(storage)
```

规则：

- backend 是 enum 单值，builder 后一次配置替换前一次配置，不可能同时留下两个 active backend。
- SQLite/PostgreSQL 都要求显式 `document_id`，除非后续增加独立的 create/open document lifecycle。
- URL、文件路径和数据库错误日志必须脱敏。
- `build()` 在 migration/config 错误时返回 `CditorError::Storage` 或 `InvalidInput`。

### 9.3 Handle 与事件补充

为了可靠关闭 SQLite 文件和支持后续同步，建议补齐此前 deferred 的 async 生命周期能力：

```rust
handle.save(cx)
handle.flush(cx)
handle.close(cx)
```

具体签名需符合 GPUI task 模型，但语义必须是：

- `save` 触发当前 dirty version 的持久化。
- `flush` 等待可靠写队列到达调用时的 barrier。
- `close` 先检查 close guard，再 flush、checkpoint/释放 session。
- timeout/失败不能发出 `SaveSucceeded`。

事件建议增加：

```rust
StorageStateChanged { backend, state }
SyncStateChanged { pending, last_server_revision } // LocalFirst 阶段
```

`DocumentInfo` / diagnostics 增加 backend kind，但不暴露连接串或绝对路径。

## 10. 运行流程

### 10.1 通用 cold start

```text
Builder config
  -> backend factory
  -> initialize + migrations
  -> load document metadata/index snapshot
  -> snapshot miss 时 load block index
  -> load layout cache
  -> construct DocumentRuntime
  -> plan initial viewport
  -> batch load initial payload window
  -> attach StorageSession to editor
  -> emit Ready
```

`CditorRuntimeLoadResult` 不再返回 `Option<PgPool>`，而是返回：

```rust
pub struct CditorRuntimeLoadResult {
    pub runtime: DocumentRuntime,
    pub report: DocumentRuntimeColdStartReport,
    pub storage: Option<StorageSession>,
}
```

### 10.2 通用 autosave

```text
Edit applied to runtime
  -> dirty/version state updated
  -> debounce
  -> capture immutable StorageSaveBatch + exact revision
  -> background commit
  -> outcome matches in-flight version?
       yes: advance persisted version
       no: retain newer dirty state
  -> emit save event
```

SQLite 和 PostgreSQL 必须经过同一个状态机，不能各自实现一套 Clean/Dirty/Saving 判断。

### 10.3 Payload window

```text
render/window planner finds missing payload IDs
  -> shared scheduler coalesces request
  -> storage.load_payloads(batch)
  -> timeout/cancellation/generation check
  -> apply records to runtime cache
  -> stale response discarded
```

现有 `POSTGRES_VIEWPORT_LOAD_*` 常量重命名为 backend-neutral 名称。SQLite 可以使用更短内部 timeout，但公共行为保持一致。

## 11. 性能分析与预算

### 11.1 预期表现

SQLite 对以下场景通常有优势：

- 本机 cold start 无网络 RTT。
- 128 个左右 payload 的窗口读取。
- 单用户短事务 autosave。
- 离线搜索和最近文档列表。

PostgreSQL 对以下场景更合适：

- 多用户、多进程并发写。
- 服务端权限、审计和共享 workspace。
- 复杂查询、服务端 FTS 和运维备份。
- 大规模云同步与跨设备权威 revision。

### 11.2 SQLite 主要风险

| 风险 | 表现 | 设计措施 |
| --- | --- | --- |
| 单 writer | `SQLITE_BUSY`、autosave 延迟 | 单写队列、短事务、busy timeout |
| 长 transaction | payload read 被拖慢、WAL 增长 | save batch 限额，超大 paste 分事务+恢复点 |
| N+1 | 10 万次 statement | index 流式读、payload 500 ID/chunk |
| 同步 DB 调用 | 输入或滚动卡顿 | 所有 IO 在 storage task，UI 只收结果 |
| WAL 无界增长 | 磁盘占用 | idle checkpoint、关闭 checkpoint、监控 WAL bytes |
| 全量 hydrate | 内存和 JSON decode 爆炸 | 只载结构 + viewport payload |
| FTS 写放大 | 输入保存变慢 | versioned background update/batch |
| VACUUM 停顿 | 长时间锁库 | 不在活跃编辑时自动 full VACUUM |

### 11.3 建议性能门槛

绝对时间受磁盘和 CI 环境影响，正式门槛应在固定参考机记录，同时 CI 使用趋势/比例回归。建议初始目标：

| 场景 | 目标 |
| --- | --- |
| UI 输入路径同步数据库调用 | 0 次 |
| 10 万 Block cold start | 不加载全量 payload |
| 初始 payload window | 不超过配置窗口 + 必要 pin |
| payload batch query count | 每 chunk 1 次，不按 Block N+1 |
| 小 autosave | 单个 SQLite transaction |
| 保存并继续编辑 | 旧 ack 不清除新 dirty version |
| 快速滚动 | 旧 generation payload 结果丢弃 |
| writer busy | 有界等待并返回可恢复错误 |
| close | flush barrier 可验证，无静默丢 dirty |

固定参考机 benchmark 再建立数值基线，例如 index load、128 payload load、1/20/500 dirty block commit 的 p50/p95；在第一版实现前不承诺缺乏测量依据的毫秒数字。

## 12. LocalFirst 同步设计边界

只有 Selectable 模式稳定后才进入本阶段。

### 12.1 本地原子事务

一次本地编辑持久化必须在同一 SQLite transaction 内完成：

```text
edit_transactions
blocks / attrs / payloads
sync_outbox
sync_state.local_sequence
COMMIT
```

只有 transaction commit 后才能对 UI 报告 LocalSaved。RemoteSynced 是另一状态，不能混为 SaveSucceeded。

### 12.2 上传与 ack

```text
read pending outbox by sequence
  -> batch upload
  -> PostgreSQL validates base revision
  -> server stores operations and returns revision
  -> SQLite marks ack + advances sync_state
```

必须区分：

- `dirty`: 尚未写入 SQLite。
- `local_saved`: 已可靠写 SQLite，但可能未上传。
- `syncing`: 正在上传。
- `synced`: 服务端已 ack。
- `conflict`: 服务端拒绝 base revision，需要恢复/合并。

### 12.3 禁止朴素双写

以下流程禁止：

```text
save SQLite
save PostgreSQL
两个都成功才返回
```

原因：进程可能在两次写之间退出，重试也可能重复应用 transaction。LocalFirst 必须依靠稳定 transaction ID、幂等 server apply、sequence 和 ack。

## 13. Migration、备份与恢复

### 13.1 Migration

- 两个 backend 各自维护 migration 目录和 checksum。
- migration 前检查 schema version，不自动打开更高版本数据库。
- SQLite DDL 能 transaction 时必须 transaction；需要重建表时使用 create-copy-validate-swap 流程。
- migration 失败保留原文件，错误中给出可执行的恢复建议。
- 大 migration 提供进度/取消边界，不能在 GPUI 主线程阻塞。

### 13.2 备份

- SQLite 使用 online backup API 或 `VACUUM INTO`。
- 备份开始前建立 flush barrier。
- 备份结果记录 schema version 和 checksum。
- PostgreSQL 仍使用服务端备份/PITR，不通过 SDK 复制数据库目录。

### 13.3 Recovery

- SQLite transaction commit 是正文恢复边界。
- `persistence_queue` 恢复未完成的可重试工作。
- `runtime_snapshot` 只恢复 selection/scroll/dirty context，不覆盖正文。
- layout/FTS 损坏时重建，不阻止正文只读打开。
- `PRAGMA integrity_check` 不应每次启动全量执行，可在显式诊断或异常恢复时运行。

## 14. 错误与可观测性

统一错误建议：

```rust
pub enum StorageError {
    InvalidConfiguration(String),
    Migration { backend: StorageBackendKind, message: String },
    NotFound { entity: &'static str, id: String },
    CorruptData { message: String },
    Serialization(String),
    VersionOutOfRange { value: u64 },
    Busy { waited: Duration },
    Timeout { operation: &'static str, timeout: Duration },
    Conflict { expected: u64, actual: u64 },
    Io(String),
    Backend { backend: StorageBackendKind, message: String },
}
```

日志和 diagnostics 至少包含：

- backend kind。
- cold start 各阶段耗时与 query count。
- index/payload/layout cache 命中数量。
- pending/saving/failed operation 数量。
- SQLite write queue depth、busy retry、WAL bytes、last checkpoint。
- PostgreSQL pool wait/query timeout。
- 当前 persisted/runtime revision。

禁止记录完整 PostgreSQL URL、密码、SQLite 用户目录绝对路径或正文 payload。

## 15. 测试方案

### 15.1 Shared contract tests

同一套测试函数分别运行在 SQLite 和 PostgreSQL adapter 上：

- metadata round trip。
- 结构 index round trip 和稳定顺序。
- 10 万 Block index 加载不读取 payload。
- payload batch round trip、missing IDs、版本校验。
- RichText/Code/Table/Image payload codec parity。
- attrs round trip。
- snapshot hit/stale fallback。
- layout exact/historical/stale behavior。
- transaction + structure + payload 原子 commit。
- rollback 后不存在半写入。
- 保存 v5 期间编辑 v6，v5 ack 后仍 dirty。
- close/flush barrier。

SQLite contract tests 使用 tempfile 数据库，默认不 ignored；PostgreSQL 同套测试使用 Docker test database，可按当前策略 ignored 或在 CI service 中运行。

### 15.2 SQLite 专项测试

- 新文件 migration、重复 migration、checksum mismatch。
- WAL/foreign_keys/synchronous 等 PRAGMA 生效。
- 两个 reader 与 writer 并发时读取一致。
- writer busy timeout 有界。
- process/task 中断后已 commit 数据存在，未 commit 数据不存在。
- WAL checkpoint 后数据完整。
- 文件路径不存在时按配置创建或报错。
- read-only 文件返回明确错误。
- 500+ ID payload 自动 chunk 且结果完整。
- FTS rebuild 与 stale task 保护。
- backup 文件可独立打开。

### 15.3 SDK/GUI 集成测试

- `.with_sqlite_path()` 可 build 为 Ready。
- 缺少 document ID 的行为与 PostgreSQL 一致。
- 编辑、autosave、drop/reopen 后内容恢复。
- SQLite 与 PostgreSQL 的 command/selection/save event 语义一致。
- readonly 不写库；切回 writable 后 dirty 自动保存。
- 快速滚动经过公共 payload loader，无 backend-specific render 分支。
- `CditorHandle::flush/close` 成功和失败路径。
- dropped component 的 Handle 仍返回 `ComponentDropped`。

### 15.4 Performance tests

- 10 万 Block SQLite seed/build benchmark。
- cold start query/decode/runtime-build 分段计时。
- 128/500 payload window latency 和 query count。
- 1/20/500 dirty block save throughput。
- 连续输入 + autosave + 滚动并发帧时间。
- WAL size/checkpoint 行为。
- PostgreSQL 重构前后基线对比，确保抽象层没有明显回归。

## 16. 实施阶段

### Phase 0：基线和 contract

- [ ] 记录当前 PostgreSQL cold start、payload load、save 测试与 benchmark 基线。
- [x] 在 `cditor-storage` 定义统一 DTO、错误和 `DocumentStorage` trait。
- [ ] 把 Postgres types 中与数据库无关的 codec 移到 `cditor-storage::codec`。
- [ ] 建立可由不同 backend factory 复用的 contract test harness。

验收：PostgreSQL 现有行为不变，workspace tests 通过。

### Phase 1：PostgreSQL 先适配公共边界

- [x] 实现 `PostgresDocumentStorage` 聚合 adapter。
- [x] cold start 返回 `StorageSession`，不返回 `PgPool`。
- [x] persistence state、batch、outcome 和 payload loader 全部去 Postgres 命名。
- [x] GUI/render 不再 import `cditor-storage-postgres` 或 `sqlx::PgPool`。
- [x] runtime cache policy 改为 persistent backend 通用策略。
- [x] aggregate adapter 使用单个 PostgreSQL transaction 原子提交 structure、index snapshot、page layout snapshot、attrs、payload 和 edit transaction。

验收：只启用 Postgres 时所有现有测试和行为保持一致；`rg` 检查 GUI 内无 PgPool/Postgres store 直接依赖。

### Phase 2：SQLite store

- [x] 新建 `cditor-storage-sqlite` crate。
- [x] 实现 config、连接、PRAGMA、migration 和 schema v1。
- [x] 实现 document/index/snapshot store，损坏或 stale snapshot 回退到 blocks。
- [x] 实现 payload/attrs store。
- [x] 实现 versioned block layout exact/historical fallback。
- [x] 接入后端中立的 versioned page layout snapshot；SQLite/PostgreSQL 都按 visible index、structure、layout key 和 page policy 精确读写，并校验连续覆盖及首尾可见 Block ID，损坏或 stale 时回退重建。
- [x] 实现 transaction 和原子 `commit(batch)`。
- [x] 同一 canonical path 的实例共享进程内 writer gate，并提供有界 busy timeout。
- [x] 实现可等待 flush barrier 和 SQLite WAL checkpoint。
- [ ] 补齐 writer queue depth、WAL bytes、last checkpoint diagnostics。
- [ ] 运行完整 shared contract tests。

验收：tempfile SQLite 可完成 create -> edit -> save -> reopen，10 万 Block 不全量 hydrate。

### Phase 3：SDK 与 GUI

- [x] 增加 `SqliteOptions` 和 builder API。
- [x] 增加 SQLite cold start/loading/error 状态。
- [x] main/example 提供 SQLite 文件启动入口。
- [x] 实现 Handle save/flush generation barrier、失败事务恢复和 retry。
- [ ] 实现 Handle close/open/switch document 完整生命周期。
- [x] diagnostics 暴露 backend kind 和 pending save 数量。
- [ ] 增加 backend-neutral storage state event。
- [x] 更新组件集成文档。

验收：同一 GUI binary 通过 SDK 参数选择 SQLite 或 PostgreSQL，编辑功能和事件契约一致。

### Phase 4：性能与可靠性

- [ ] 10 万 Block benchmark 与 query count 验收。
- [ ] autosave/scroll 并发测试。
- [ ] WAL checkpoint、backup、recovery 测试。
- [ ] PostgreSQL 重构回归对比。
- [ ] 补充跨平台 Windows/macOS/Linux 文件锁和路径测试。

### Phase 5：LocalFirst，可选后续

- [ ] 完成全局稳定 ID 迁移。
- [ ] SQLite transaction 同步写 outbox。
- [ ] PostgreSQL 幂等 apply 和 revision/ack。
- [ ] pull、tombstone、conflict copy 和 recovery UI。
- [ ] 区分 LocalSaved/RemoteSynced 状态与 close policy。

## 17. 预计改动范围

Selectable 双后端预计涉及：

- 新增 1 个 workspace crate，约 12-20 个源文件和独立 migrations。
- 重构 `cditor-storage` 公共 contract/codec。
- 改动 app API、cold start、GUI persistence、render payload load、lifecycle 和 diagnostics。
- 适配 PostgreSQL 聚合 backend，但尽量不重写其底层 SQL store。
- 新增 shared contract、SQLite integration、SDK integration 和 performance tests。

保守判断是 **中到大型重构**，不是高风险推倒重写。按上述阶段推进时，每一阶段都能保持 PostgreSQL 可运行并独立验收。

LocalFirst 同步是另一项大型功能，不应包含在第一版 SQLite selectable backend 的交付承诺中。

## 18. 推荐默认决策

| 决策 | 推荐值 |
| --- | --- |
| SQLite Rust driver | `sqlx 0.8` SQLite |
| journal | WAL |
| durability | `FULL` 默认，显式支持 `Balanced/NORMAL` |
| writer | 单队列、单连接、短 transaction |
| readers | 2-4，按平台 benchmark 调整 |
| IDs | SQLite BLOB UUID；暂时兼容 runtime u64 mapping |
| JSON | serde 验证后的 TEXT |
| FTS | FTS5，能力独立声明 |
| assets | 文件/对象存储，DB 只保存 metadata |
| SDK | backend enum + builder convenience methods |
| 双库 | 第一阶段二选一，后续 outbox LocalFirst |
| migration | 后端独立 SQL，共享逻辑 schema/version policy |

## 19. 完成定义

只有同时满足以下条件，才能称为“SQLite 与 PostgreSQL 双后端已支持”：

- SDK 能显式选择 SQLite 或 PostgreSQL。
- GUI/runtime 不直接依赖具体 pool/store。
- 两个后端通过相同 storage contract tests。
- 两个后端 cold start 都不加载全量 payload。
- 编辑保存具有版本保护和后端内原子性。
- SQLite writer 不运行在 GPUI UI hot path。
- SQLite reopen、migration、busy、rollback、WAL/recovery 有测试。
- PostgreSQL 原有测试和性能没有明显回归。
- 文档明确 Selectable 与 LocalFirst 的差异，没有声称朴素双写等于同步。

## 20. 对当前问题的直接回答

**改动大吗？**

可选择 SQLite/PostgreSQL 属于中到大型改造，主要工作不是 SQLite SQL，而是清理当前 app/GUI 对 PostgreSQL 的具体依赖。通过先让 PostgreSQL 适配公共 trait，再实现 SQLite，可以控制风险，不需要重写 runtime/editor。

**会有性能问题吗？**

正常设计下不会。SQLite 对本地单用户场景大概率更快。需要重点控制单 writer、事务长度、batch 查询、WAL、FTS 和后台调度。10 万 Block 架构仍然必须坚持“结构全量轻加载，payload 窗口化”。

**能否同时支持？**

同一 binary 和 SDK 可以同时提供两种 backend 选择。若“同时”指同一文档本地 SQLite + 云端 PostgreSQL，则必须按 LocalFirst/outbox/ack 方案实现，不能简单双写。
