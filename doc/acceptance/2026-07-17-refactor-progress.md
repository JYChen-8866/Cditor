# 重构进展记录（2026-07-17）

> 分支：`codex/parley-text-layout`（在 2026-07-16 基线之上）
>
> 本文记录本轮完成项及其验证命令，是
> `2026-07-16-editor-refactor-baseline.md` 的增量更新。

## 1. 本轮完成项

### Gate P0 阻断项清零

| 项 | 状态 | 证据 |
|---|---|---|
| workspace strict Clippy | **通过** | `cargo clippy --workspace --all-targets -- -D warnings` 干净退出。core 25 项、runtime 15 项、app 29 项、editor 6 项、store-postgres 4 项、ding-board 10 项存量 lint 全部真实修复；仅 23 个 GUI render 函数的 `too_many_arguments` 以 `#[expect(..., reason = "P4-002 render context 聚合")]` 显式挂账（属 Phase 4/5 render context 重构范围）。大 enum（`UndoPayload::InlineSmall`、`CditorViewState::Ready`、`DbEditOperation::Table`）以 Box 收敛。 |
| P0-007 版本化 fixture | 完成 | `crates/core/src/fixtures/`（bidi/code/table 确定性生成器 + FNV-1a 语义 checksum；100k mixed 复用 demo_fixtures）。 |
| P0-008 frame benchmark 基线 | 完成 | `crates/runtime/benches/frame_baseline.rs` + `doc/acceptance/2026-07-17-frame-baseline-benchmark.md`（M1 Max full 模式全过）。 |
| P0-009 telemetry schema | 完成 | `crates/core/src/telemetry/`（input/layout/storage/sync 四域，类型级禁自由文本）+ `doc/architecture/telemetry-schema-v1.md`。 |
| P0-012 三类模板 | 完成 | `doc/templates/{adr-template,migration-checklist,manual-acceptance-template}.md`。 |

### Phase 1（除 P1-011 UI 呈现与 P1-013 实数据迁移外全部完成）

- ADR-006 裁决：UUIDv7 + base-256 fractional order key
  （`doc/architecture/adr/ADR-006-persistent-id-and-order-key.md`）。
- `crates/core/src/identity/`：PersistentId + 13 类 typed ID、RFC 9562
  单调生成器（回拨/溢出/双设备）、RuntimeHandle/IdArena、LegacyIdMap、
  OrderKey（between/entropy 消歧/rebalance）。
- `crates/core/src/schema/`：七域独立 SchemaVersion、ReadPolicy 四态、
  RawValue envelope（unknown 字节不变 + 新 minor 未知字段保留重写）、
  30 内置 kind 的 BlockRegistry（capabilities/migrator/unknown fallback）。
- `crates/core/tests/identity_tree_property.rs`：随机 tree/order 操作
  不变量（P1-012）。

### P6-015 机制层（大文本分段布局）

- `crates/text/src/segmented.rs`：O(n) 硬行分段、窗口化测量、自适应估高、
  局部失效、宽度 reflow、字节偏移滚动锚点；"分段总高 == 整块布局高度"
  一致性测试（含软换行）。
- `crates/text/benches/segmented_layout.rs`：10MiB/549 段 full 模式全指标
  在预算内（索引 p95 2.5ms、冷窗口 9.7ms、滚动步进 4.8ms、编辑重测 5.0ms、
  reflow 窗口 4.7ms）；对比整块 build p95 2.543s。
- 未完成：App `RichTextElement`/cache identity/高亮接线。

### Phase 4 增量（第二轮）

- **P4-007**：core 统一 `ChangeOrigin`（10 来源 + records_local_undo /
  breaks_typing_coalescing / marks_document_dirty 三个语义谓词）；App/SDK
  旧枚举删除，`change_origin_for_source` 精确映射（Ime/Plugin 独立）。
- **P4-014**：`transaction_codec`——EditTransaction 经 Operation 域 envelope
  序列化；未知 op 整体拒绝（不部分应用）、新 major 只读、旧 major 需迁移。
- **P4-013**：随机 edit -> undo all -> redo all property test（5 seed × 60 步
  + 200 步长会话，语义状态精确比对）。**该测试发现真实缺陷**：Enter split
  与 insert-paragraph 只快照文本，undo 无法移除新建块。
- **缺陷修复与后续收口**：split/insert-paragraph 最初通过结构快照修复 undo
  丢块，随后迁移为携带 forward/inverse operation 的 typed transaction；一次 Enter
  仍严格对应一个 undo step，undo 移除新块并恢复原块，redo 按位还原。旧的
  `StructurePasteUndoStep`/`StructureMoveUndoStep` 及其独立 undo/redo 栈现已删除，
  结构写入不再和 transaction undo 双轨运行。
- **P4-015**：42 个 command × readonly/无 focus/有 focus 三态的 query/execute
  一致性测试（禁用必须拒绝且 revision 不变；启用不得报前置错误）。
- **P4-008**：Runtime snapshot undo 按稳定 `SurfaceId` + 1 秒窗口合并连续输入；
  普通 Block、table cell、image caption、collection title 行为一致。selection、caret、
  focus/surface switch、CommandRouter、composition、paste、format、软换行、undo/redo
  均为边界；7 项 Runtime 定向测试和 1 项 App command 边界测试通过，Runtime 全量
  460 项测试通过。
- **P4-012**：text snapshot、structure move/paste step、external transaction 三条
  undo/redo 路径统一恢复 before/after selection 与语义滚动锚点；结构路径额外恢复
  跨 block 文本选区和 whole-block selection。锚点经当前 HeightIndex 重解，覆盖上方
  高度变化且每步只 restore 一次；12 项 undo UX 定向测试与 external transaction
  双向恢复测试通过。
- **P4-009**：IME、paste、gutter drag/structure move、table commit、AI apply
  都有独立 undo boundary。Enter、merge、空块/叶块删除、whole-block 批量删除和
  subtree move 均已改为带精确 inverse 的 typed transaction。批量删除使用单次完整
  preorder 子树范围；全选时同一事务内重置保留段落并删除其余块，非连续 selection
  在 mutation 前原子拒绝。旧 structure snapshot/move undo 栈已经移除。
- **P4-002**：keyboard、toolbar、slash、context menu、code language、AI 的
  command surface 已全部汇入 versioned CommandRouter，并统一执行 catalog/query gate。
  审计剩余 direct Runtime 点仅为 platform text/IME、mouse selection、gesture/resize/
  scroll 和 async load；这些保留在 P4-005/P4-006 与 Gate P4，不再错误挂账到 P4-002。
- **P4-005 完成 / P4-006 第三、四轮**：transaction applier 已消费全部 typed domain operation。
  Text 覆盖普通 Block、Code/HTML、table cell、image caption、collection title；Block
  payload/attrs、Collection schema/view/record/value、Comment thread/message/anchor、Asset
  attach/detach/update 均使用 copy-on-touch staging，逐项核对 before 值和 stable ID，
  中途失败时 payload/attrs/collection/comment/asset 状态整体不提交。Runtime 新增独立
  collection record/comment/asset owner，不把 record 写回 collection payload。
- applier 对每个 touched payload/dirty block 分别只推进一次 content/layout version，
  统一返回最终 document/structure/content/layout version 与 preorder dirty range，并同步
  live EditingSession 的 content version。首次执行用 transaction version precondition；
  undo/redo 依靠 operation before 值，避免 redo 重放首次 precondition 后永久 stale。
- keyboard/toolbar/SDK 共用的 inline mark/color 已成为首条端到端 typed transaction UI
  写路径：产出带 inverse 的 `TextEditOperation`，format/paste 自动选择独立 undo kind，
  undo/redo 重放同一 operation；不再在该路径写 snapshot undo。
- structured Markdown、AI Markdown 与 native rich block paste 全部分支现已产出
  payload-carrying `InsertBlocks`/`DeleteBlockRange` operation。跨块 selection 先把删除端
  扩展为完整 preorder 子树范围，再把语义终点之后被覆盖的节点作为 preserved tail
  重新插入，因此不会产生瞬时 orphan，且 undo 能精确恢复原 parent/depth/payload。
- 跨块 delete、merge、空块/叶块删除、whole-block 删除和 gutter subtree move 已通过
  同一 applier 提交；`rebuild_structure_index` 的生产调用只剩 transaction applier。
  table resize/reorder/merge/split/style、image resize 与 whiteboard commit 也已由 typed
  table/block operation 驱动。普通 Block Backspace 的最后一条 direct PieceTable 写路径已
  汇入 preapplied typed text transaction，并覆盖多码点 emoji grapheme 的 forward/inverse、
  replay 与 undo。Runtime 全量 504 项、App lib 382 项通过（另 1 项 ignored）。
- **P4-005 preapplied 原子契约收口**：所有可恢复错误在 mutation 前校验 owner payload、
  `SurfaceId`、content identity、grapheme range、live PieceTable/table cell 与 authoritative
  payload 一致性、layout metadata；失败不消费 transaction id、不改 undo/payload/layout。
  mutation 后的 transaction 构造与入队改为不可失败 commit，不再存在“正文已改、记录
  transaction 返回 Err”的半提交路径。空 replacement 现在是真正的零 mutation no-op。
  2 项失败/空操作测试逐字段验证 payload、live model、transaction id、undo event、layout
  和 pending transaction 均不变化。
- `SingleCharInputHotPath` 新增 mutation 前 forbidden-work/offset preflight；真实注入同步
  SQLite 禁令时，Runtime 测试证明 selected blocks、editing session、undo/redo stacks、
  typing group、payload/model、transaction id、layout 和 pending queue 全部零变化。
- 五类 Surface 等价矩阵在独立 Runtime 上仅用记录的 transaction 经 applier 重放，普通
  RichText、Code、table cell、image caption、collection title 的最终 payload、content
  version、layout version 与快路径逐项完全相同。由此 P4-005 的本地 fast path 原子性和
  operation 等价性证据闭环。
- **P4-006 版本审计修复**：preapplied transaction 记录层现在感知 mutation 前的
  layout version；若 Markdown shortcut 或 table 同步测高已在同一 commit epoch 推进过，
  不再重复增长。inline Markdown shortcut 的闭合字符也不再让 content version 增长两次。
  定向测试逐 transaction 断言 block/inline shortcut 与 table resize 的 content/layout
  version 均只增长一次。
- **P4-006 TextSurface identity 修复**：Collection title、Image caption 不再把
  content version 冒充 layout version，Table cell 不再固定 layout version 为 0，鼠标
  fallback hit-test 也不再构造 layout version 0；四者统一消费 owner Block 的
  `BlockLayoutMeta.layout_version`。App 2 项 identity 测试显式使用不同 content/layout
  数值，防止再次耦合。
- **P4-006 完成审计**：生产侧 content/layout version 写点只剩统一 applier 和已验证的
  block-local fast path；`AsyncVersionController` 对 document/structure/content/layout/
  font/scale/viewport/generation、width bucket、exact width、theme 全维度 gate，旧结果仅存
  historical hint，不覆盖当前 exact layout。payload window 的 generation owner 测试覆盖
  stale viewport result 不得清理或覆盖本地编辑/新请求。结合 dirty range、删除清理、
  EditingSession 同步和本轮四类 TextSurface identity 修复，P4-006 证据闭环。
- 文件拆分：`structure_insert.rs`（split/insert 路径）、
  `command_router_tests.rs`、`transaction_apply_domain.rs`、
  `transaction_apply_domain_validation.rs`、`format_transaction.rs`，维持 700 行上限。

## 2. 验证命令（本机 M1 Max / macOS 27.0 / rustc 1.95.0）

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `./scripts/dev/check_structure.sh` | 通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **通过（新）** |
| `cargo test --workspace` | **通过（本轮复验）**：runtime 510、app lib 387+1 ignored、core 241+4 ignored、text 53、ding-board 47、store-sqlite undo blob 6、store-postgres 23+55 ignored，其余 workspace unit/integration/doc tests 全部通过。 |
| `cargo bench -p cditor-runtime --bench frame_baseline -- --full` | 通过（报告见 acceptance） |
| `cargo bench -p cditor-text --bench segmented_layout -- --full` | 通过 |

## 3. Gate 判断更新

- Gate P0：**通过**。基线文档指出的全部缺口（fixture、benchmark 报告、
  telemetry schema、模板、strict Clippy）已闭环；自动化在干净环境可重复。
- Gate P1：部分通过。离线 ID 无冲突已验证；unknown round-trip 的存储层
  端到端与 legacy 迁移 checksum 依赖 Phase 7。
- Gate P2/P3/P6 判断不变（详见基线文档），但 P2 的 large-code 预算缺口
  已有 P6-015 机制层解法与基准证据。

## 4. 已知挂账

- P5-008 已推进到 Core/Runtime projection：新增稳定 kind tag、Columns payload、schema registry、
  PostgreSQL JSON roundtrip 和真实容器树 validator；`ColumnsLayoutModel` 固化稳定 Column ID、
  整数比例权重、最小宽度、group max-height、相邻 resize、x hit-test 和水平导航，Runtime 从
  DocumentIndex subtree height 生成 layout snapshot；`ColumnsChildHeightIndex` 支持列级 Fenwick
  更新和 group max delta，Runtime `resize_columns_boundary` 以单一 typed payload transaction
  提交权重并支持 undo/redo；subtree clipboard paste 会同步 remap Columns payload 内部 Column
  ID，并通过结构 validator 验证。2–12 列与 1000 次极端 resize 不变量测试通过。下一步是二维
  selection、GUI projection/drag 和外部格式 clipboard。

- P5-007 已完成：Indent/Outdent command capability query 的 soft-tab 与结构 reparent 都在 Runtime
  提供无副作用 preflight，CommandRouter 不再仅凭“存在 focused Block”错误启用 Tab/Shift-Tab。
  根层、首 sibling、不可接 children 的 sibling 和无缩进当前行均有行为测试；完整 complex Block
  Enter/delete/move keyboard matrix 已收口到 Core `BlockKeyboardPolicy`。Runtime 修复 HTML Enter
  错误 split、atomic/complex leaf 删除 no-op，以及 complex focus 伪造 text selection 导致 undo
  查找不存在 text model 的问题；typed delete undo 会恢复完整 payload。

- P5-003 已完成：selection-specific request 携带 document/structure/unified-selection/
  payload-generation identity，只列出当前操作依赖的缺失 payload；App persistence bridge
  以单飞任务加载并在主线程校验 stale，完整成功后自动重放 copy/cut/delete，失败、缺失记录
  或 stale 响应不会触发 mutation；whole-block copy/cut/delete 会包含折叠选中根的完整子树和
  delete 所需 surviving Block。Runtime 4 项 materialization 测试和 App 388 项测试通过。

- Parley 0.11 在少数 RTL + emoji ZWJ hit-test 路径会返回 UTF-8 scalar 内部 byte index；
  `cditor-text` 现在在 cursor 输入输出边界按 affinity 归一化 offset。失败 seed 已写入
  proptest regression corpus，53 项 text 测试和 workspace 全测通过。

- 23 个 `#[expect(clippy::too_many_arguments)]`：Phase 4 CommandRouter/
  render context 落地时移除。
- P1-011 App 只读模式 UI、P1-013 实数据迁移演练：随 Phase 7。
- 分段布局的 App 接线：随 Phase 6。
- P4-005 已完成：preapplied printable/IME 使用 mutation 前 preflight、mutation 后不可失败
  transaction commit、forbidden-work fail-injection 和五 Surface applier replay 等价矩阵；
  table/resize/reorder、image resize、whiteboard 与 gutter move 也已完成 typed operation
  迁移。
- P4-006 已完成：applier/fast path version 与 dirty range、payload generation、async layout
  identity、complex block typed payload 和 stale-result 分支已完成形式化审计与测试。
- P4-010：core `UndoStack` 已修复大 transaction 丢失问题：超过阈值的 transaction 以
  所有权移动保存为 pending `BlockRangeSnapshot`，undo/redo 栈移动不再 clone；新增
  `ExternalUndoBlobRef`、spill/hydrate 状态转换，只有 SQLite 写成功后才替换为 reference。
  SQLite migration `0004_undo_blobs.sql` + `SqliteDocumentStorage::{spill_next_undo_snapshot,
  hydrate_undo_snapshot}` 已落地 operation-envelope JSON、SHA-256 校验、文档隔离、按访问
  时间 prune 和显式 delete。Core 新增 `Pending -> Externalizing -> ExternalBlob` 状态机，
  spill job 以所有权移出 transaction，适配 GPUI 不跨 `await` 借用 Runtime；写入失败会
  原样 abort 回 pending snapshot。4 项 integration tests 覆盖 roundtrip、checksum
  corruption、write failure retry、cleanup。Runtime 当前 `external_undo_stack` 仍保留 typed transaction，尚未接入异步
  spill worker，因此“大 paste/undo 全链路不复制全文”仍未证明，P4-010 保持未勾选。
  Runtime 接入拆解（按此顺序推进）：
  - [x] R1：将 `external_undo_stack`/`external_redo_stack` 替换为 Runtime-owned
    `UndoStack` 适配层，保留 `RuntimeUndoEvent` 的焦点/selection/scroll 恢复元数据，
    并禁止 Runtime 直接 clone 大 transaction。
    - [x] R1a：Runtime 已删除独立 redo `Vec` 并统一使用 Core `UndoStack`；undo/redo
      通过 take/commit/rollback 移动 step，应用 inverse 时原位交换 `ops/inverse_ops`，不再
      clone 整笔 inverse；失败会恢复原 stack 与 event。
    - [x] R1b：100-step 容量裁剪和 transaction UX metadata 可变访问已迁入
      `UndoStack`，Core 所有权状态转换测试及 Runtime 504 项测试通过。
    - [x] R1c：`EditTransaction::ops/inverse_ops` 已改为 serde-compatible
      `Arc<Vec<EditOperation>>` 不可变共享载荷；transaction metadata clone 不复制 Block
      payload，JSON wire format 仍编码为普通数组。普通 typed transaction 和 2,000 Block
      paste 测试均以 `Arc::ptr_eq` 证明 undo 与 persistence queue 共享同一正向/逆向载荷；
      Runtime 504 项测试通过。
  - [x] R2：实现 `UndoPayload` 的 reference hydration：undo/redo 遇到
    `ExternalBlob` 时先发出可观察的 hydration 状态，后台加载并校验
    后再在主线程提交，不得跨 await 持有 Runtime 借用。
    - 证据：Runtime 暴露 undo/redo 顶层 `ExternalUndoBlobRef` 查询和 owned hydrate
      入口；App 键盘、CommandRouter、SDK 共用 `execute_history_action`，遇到 reference
      后通过 `StorageSession` 单飞加载，主线程校验 snapshot identity、hydrate 并自动重放
      原动作。SDK 发出 `HistoryHydrationStarted/Succeeded/Failed` 可观察事件；加载或重放
      失败时 history step/event 保持原位，可再次触发重试。
  - [x] R3：在 App persistence bridge 建立每文档单飞 spill worker；只处理最老的
    `BlockRangeSnapshot`，写盘成功后调用 `complete_externalization`，失败调用
    `abort_externalization` 并保留可撤销状态。
    - 证据：Runtime 暴露 owned spill job API，App 在 I/O 前释放 Runtime borrow；
      `StorageSession` 新增 backend-neutral undo blob 接口，SQLite 动态分发 roundtrip
      测试通过，非支持 backend 明确返回 backend error。worker 用 in-flight gate 保证
      单飞，成功提交 reference、失败恢复 transaction。
  - [ ] R4：接入 SQLite 生命周期清理：文档关闭、撤销栈淘汰、数据库压缩分别触发
    reference delete/prune；document_id、snapshot_id、checksum 三重校验必须覆盖。
    - [x] R4a：Core 在新编辑清空 redo、100-step 容量淘汰和显式 clear 时收集失联
      `ExternalUndoBlobRef`，去重后由 Runtime drain；App 单飞调用 backend-neutral delete，
      删除失败把 reference 原样恢复到重试队列。
    - [x] R4b：`StorageSession` 已接通 SQLite delete/prune，文档隔离和显式删除由 6 项
      undo blob integration tests 覆盖。
    - [ ] R4c：文档关闭 hook 与数据库 compact/maintenance 调度仍需接入。
  - [x] R5：增加大 paste 端到端证据：共享载荷 strong-count、spill/hydrate 延迟、写盘失败
    重试、undo/redo 对称性和新编辑清空 redo；完成后才勾选 P4-010。
    - 证据：SQLite integration test 使用 2,000 个 rich-text Block、约 4 MiB payload，
      `Arc::ptr_eq`/strong_count=2 证明 Runtime 不复制全文；spill 和 hydrate 各自 10 秒
      上限内完成，encoded length/checksum/完整 transaction equality 均验证。另有写失败
      retry、checksum corruption、delete/prune 和 clear/redo 淘汰测试。
