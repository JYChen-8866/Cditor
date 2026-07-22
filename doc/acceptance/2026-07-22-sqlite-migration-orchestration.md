# SQLite Migration Orchestration 验收记录

> 日期：2026-07-22
>
> 对应总清单：P1-013、Gate P1 legacy checksum、P7-013
>
> 测试入口：`cargo test -p cditor-storage-sqlite --test migration_orchestration`

## 1. 实现边界

`cditor-storage-sqlite::SqliteMigrationManager` 包装 SQLx migrator。SQLx 继续负责单个
migration 的事务与 `_sqlx_migrations` 账本；manager 负责跨 migration 的数据安全流程：

```text
preflight
  -> consistent backup
  -> isolated dry-run
  -> integrity / FK / checksum validation
  -> formal migration
  -> validation
  -> success (retain backup) | atomic rollback
```

`SqliteDocumentStorage::open` 在创建业务连接前运行该流程。新库没有历史数据，直接创建
当前 schema；已是当前版本的库只校验 migration ledger，不制造无意义备份。

## 2. Preflight 与备份

Preflight 拒绝以下状态：

- 已存在但没有 SQLx ledger 的非受管数据库；
- ledger 中失败、未知或 checksum 不匹配的 migration；
- 可用空间不足 `3 * (db + WAL + SHM) + 16 MiB`；
- 源库 `integrity_check` 非 `ok` 或存在外键违规。

备份使用 SQLite `VACUUM main INTO ?`，因此包含 WAL 中已提交状态且不是数据库文件的
朴素复制。备份文件和父目录执行 `fsync`。成功迁移后保留备份路径于
`SqliteMigrationReport`；显式 rollback 会先验证备份，再通过同目录临时文件 + rename
原子替换源库，并清理旧 WAL/SHM sidecar。

## 3. Dry-run、Checksum 与 Unknown Preservation

正式修改前，manager 从一致性备份复制隔离 dry-run 数据库，在其上逐 migration 执行。
迁移前、dry-run 后和正式迁移后均记录：

| 校验 | 覆盖 |
|---|---|
| `semantic_sha256` | workspace/document/block/attrs/payload/transaction/runtime snapshot/journal 权威行 |
| `unknown_raw_sha256` | attrs、kind、payload、transaction、snapshot、operation envelope 原始 JSON |
| `asset_refs_sha256` | asset 与 block-asset 引用（表存在时） |
| SQLite validation | `PRAGMA integrity_check` 与 `foreign_key_check` |

新增但为空的权威表与“该域尚无数据”视为相同语义，避免 schema 扩展本身改变逻辑
checksum。JSON 不做 parse/re-serialize，测试 fixture 使用非常规空白、字段顺序和转义，
因此能证明 raw bytes 保留而不只是解析结果等价。

## 4. Progress、Cancel 与 Resume

进度阶段为 Preflight、Backup、DryRun、Applying、Validating、Completed、RollingBack。
DryRun 与 Applying 均按 migration version 报告 completed/total。取消只在版本事务边界
生效，不会中断一条 DDL 的原子提交。

SQLx ledger 是持久 resume cursor：进程在 migration N 事务提交前退出时 SQLite 回滚该
事务；提交后退出时 ledger 已记录 N，下次 preflight 只计划 N+1 之后的版本。当前 migration
规模无需表内 cursor；未来单个超大 backfill 必须另建持久 batch cursor。

## 5. 自动化证据

`migration_orchestration.rs` 从真正只执行 `0001_initial.sql` 的数据库开始，写入 legacy
document、旧 page layout 和 unknown plugin kind/payload/attrs，然后覆盖：

1. `open_runs_backup_dry_run_validation_and_preserves_unknown_bytes`：v1 -> v4，三阶段 checksum
   一致，backup 存在，unknown 三个字段逐字节相同；显式 rollback 后 schema 回到 v1。
2. `progress_reports_each_safe_resume_boundary`：dry-run 与正式阶段都按 2、3、4 顺序报告。
3. `cancellation_before_backup_leaves_source_untouched`：preflight 边界取消不产生备份或写入。
4. `cancellation_between_formal_migrations_automatically_restores_backup`：正式 migration 2 已提交
   后取消，触发自动 rollback，schema、语义内容与 unknown raw bytes 全部恢复到 v1。

验证结果：4 passed；`cditor-storage-sqlite` 全包 34 项测试通过；包级 strict Clippy 通过。
最终 workspace 门禁结果随总重构清单持续记录。
