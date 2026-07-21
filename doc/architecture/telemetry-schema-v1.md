# Telemetry Schema v1（P0-009）

> 真相来源：`crates/core/src/telemetry/`（类型定义 + 单元测试）。本文只做导览，
> 字段以代码为准。

## 1. 无内容原则

- schema 从类型上禁止 `String`/路径字段：所有字段只能是数值、布尔、枚举或
  数值 ID/哈希。测试 `test_support::assert_content_free` 断言序列化后的 JSON
  中所有 string 值都是标识符形态的枚举 tag。
- 不记录绝对时间：envelope 用会话内毫秒偏移 `session_offset_ms` 与单调
  `sequence`。
- trace 关联仅允许 `transaction_id`、`document_hash`、`task_generation`
  三个数值维度（总设计 27.1）。

## 2. Envelope

`TelemetryRecord { schema_version, sequence, session_offset_ms, trace, event }`，
`TELEMETRY_SCHEMA_VERSION = 1`。`TelemetryEvent` 以 `domain` tag 分四个域：
`input` / `layout` / `storage` / `sync`；每个域内以 `kind` tag 区分事件。

## 3. 各域事件

| 域 | 事件 | 用途 |
|---|---|---|
| input | `action` | 输入动作（插入/删除/导航/粘贴等）到事务提交的延迟 |
| input | `ime_preview_latency` | IME preview 更新延迟（预算 p95 < 16ms） |
| input | `geometry_query` | 几何查询来源采样（Gate P2 fallback-rate=0 的度量口径） |
| input | `stale_callback_rejected` | 平台回调被 session identity 拒绝及维度 |
| layout | `build` | 布局构建路径（cache_hit/reflow/full_build + 分类原因）与耗时 |
| layout | `cache_snapshot` | 缓存 entries/bytes 双预算、命中率与压力档位 |
| layout | `stale_result_rejected` | 异步结果按九个身份维度被拒绝（`From<SnapshotIdentityMismatch>`） |
| layout | `frame_budget` | 帧耗时相对预算采样（长帧诊断入口） |
| storage | `save_status_changed` | P7-006 五态保存状态机迁移 |
| storage | `transaction_durable` | 本地事务落盘耗时（预算 p95 < 50ms） |
| storage | `journal_replay` / `checkpoint` | 启动恢复结果与 checkpoint 截断 |
| storage | `error` | P7-007 错误分类（disk full/busy/permission/corruption…） |
| sync | `push_batch` / `pull_batch` | 批次规模、尝试次数、结果、往返/应用耗时 |
| sync | `rejection` | P8-006 拒绝分类 |
| sync | `retry_scheduled` / `outbox_depth` | 退避与 outbox 积压采样 |

## 4. 演进规则

- 新增字段/变体：minor 演进，不升版本；reader 必须容忍未知 `kind`。
- 删除、改名或语义变化：提升 `TELEMETRY_SCHEMA_VERSION`，旧 reader 按
  版本分支解析。
- 新增域或事件时必须同步补 round-trip + content-free 单元测试。

## 5. 与现有实现的关系

- App 侧 `TextGeometryTelemetry`（`crates/app/src/gui/text/diagnostics.rs`）
  的累计计数是 `input.geometry_query` 的现行来源；后续接入时按查询产生
  事件或周期性汇总，二者口径一致（snapshot / sync_fallback_build /
  unavailable）。
- storage/sync 域当前尚无生产发射点，属于 Phase 7/8 的接入任务；schema
  先行固化以约束后续实现不携带内容。
