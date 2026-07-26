# Cditor SQLite / PostgreSQL 双后端设计

> 状态：Selectable mode implemented
>
> 当前实现：`cditor-storage`、`cditor-storage-sqlite`、
> `cditor-storage-postgres`、`cditor-session`、`apps/cditor-desktop`

## 1. 模式定义

当前每个编辑器实例只选择一个持久化后端：Memory、SQLite 或 PostgreSQL。SQLite 与
PostgreSQL 不做朴素双写，避免产生两个无法裁决的 revision 真相。

```text
Memory:      Runtime truth, process exit discards data
SQLite:      SQLite persistent truth + Runtime live truth
PostgreSQL:  PostgreSQL persistent truth + Runtime live truth
```

Local-first 多端同步需要独立的 outbox、server revision、ack/pull 和冲突协议，属于协同范围，
当前明确排除。

## 2. 边界

```text
cditor-storage                 StorageProvider/DocumentStorage/DTO/error
       ^                    ^
       |                    |
cditor-storage-sqlite   cditor-storage-postgres
       ^                    ^
       +-------- Desktop ---+
                    |
                 Session
                    |
                 Runtime
```

- Storage contract crate 不依赖 Tokio、Runtime、Session、GPUI 或 SQLx。
- adapter 自己拥有 pool/connection、migration、row/codec 和 timeout。
- Desktop 选择 concrete provider，但不读取 adapter row/repository 内部类型。
- Session 负责任务、cold start、save、payload window、recovery 和取消策略。
- Runtime 只接收 storage-neutral DTO/result，不依赖 Import/Export 或具体数据库。
- GPUI Editor 只通过 `EditorSessionHandle` 请求 I/O，不持有连接或 StorageSession。

## 3. 共享 contract

两个 adapter 必须实现同一 `DocumentStorage` contract，覆盖：

- metadata/index cold start；
- bounded payload window；
- 原子 transaction/save batch；
- revision 和 checksum 校验；
- layout/page cache；
- undo blob 和 emergency recovery；
- migration preflight、cancel、backup/restore；
- unknown schema/bytes 的拒绝或只读策略。

共享 contract suite 位于 `cditor-test-support`，SQLite 和 PostgreSQL adapter 都必须调用
`run_document_storage_contract`。

## 4. SQLite

- WAL 模式和单 writer 策略；
- busy timeout 有界；
- schema migration 前 preflight 和备份；
- 中断或阶段失败自动恢复；
- test-only connection/row API 只能在 dev dependency 或显式 test-support feature；
- production public API 不暴露 `SqliteRow/Pool/Connection/Writer`。

SQLite 适合本地文档和离线单机使用。数据库工作不得在 GPUI 输入线程同步执行。

## 5. PostgreSQL

- provider 可从 URL 或宿主 `PgPool` 构造；
- migration、repository、row codec 和 SQLx 类型留在 adapter；
- Desktop 只组合 `PostgresStorageProvider`；
- 集成测试通过独立测试库和 ignored test 运行；
- 日志和 dry-run 输出不得泄漏数据库 URL/密码。

PostgreSQL 适合服务端权威、团队空间和运维集中场景，但当前 Desktop 直连模式不等同于未来
协同服务协议。

## 6. SDK 选择

`cditor-sdk::Cditor` 只接受 backend-neutral provider：

```rust
Cditor::new()
    .with_document_id(document_id)
    .with_storage_provider(provider)
```

Desktop 扩展提供便捷方法：

```rust
Cditor::new().with_document_id(id).with_sqlite_path(path);
Cditor::new().with_document_id(id).with_postgres_url(url);
```

这些方法属于 `cditor_desktop::CditorStorageExt`，不能回流到 SDK。

## 7. 性能与一致性

- 输入先提交 Runtime transaction，持久化异步执行；
- dirty payload 在确认落盘前保持 pin；
- cold start 只加载结构和首个 bounded payload window；
- viewport 变化触发合并、可取消的 window request；
- layout cache 是可重建派生数据，正文和 transaction 不是；
- stale revision/save outcome 不覆盖更新状态；
- 单次 save/undo/recovery 保持原子边界。

## 8. 验收

```sh
cargo test -p cditor-storage-sqlite --all-targets
cargo test -p cditor-storage-postgres --all-targets
cargo test -p cditor-session --all-targets
scripts/dev/test_run_editor_scripts.sh
scripts/dev/check_structure.sh
```

PostgreSQL ignored integration test 需要 `CDITOR_TEST_DATABASE_URL`。默认 workspace gate 不应依赖
外部数据库可用性。

重构前的耦合分析归档于
`doc/archive/architecture/sqlite-postgres-dual-backend-design-pre-refactor.md`。
