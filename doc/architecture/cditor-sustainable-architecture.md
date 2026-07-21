# Cditor 可持续架构宪章

> 本文是 Cditor workspace 的**顶层架构宪章**，定义分层、契约、数据流、扩展点与不变量。
> 它统领而非取代既有文档：
> [工程结构](project-structure.md) 记录目录边界与源码规则，
> [总体设计](cditor-mature-notion-editor-master-design.md) 记录产品级设计，
> [双后端设计](sqlite-postgres-dual-backend-design.md) 记录存储实现。
> 当本文与其它文档冲突时，以本文的**分层与契约约束**为准。

---

## 1. 定位与非目标

**定位**：为 Cditor 未来 5 年的演进提供一套稳定的结构性约束，使得新增能力（协同、插件、同步、搜索、自动化）以**增量子系统**的方式接入，而不需要改写核心。

**非目标**：
- 不推翻现有分层。现状的 `core → editor/text → runtime → store → app` 方向是正确的，本文是把它**固化为契约**并补齐扩展点。
- 不提前创建空 crate。扩展能力先以**契约与扩展点**登记，待真实需求出现再落地实现（见 §7）。
- 不描述具体算法。热路径、虚拟滚动、布局估算等在各自领域文档中维护。

**读者**：任何要新增功能、拆分模块、或评审 PR 结构合理性的人。

---

## 2. 架构哲学

五条原则，按优先级排序。冲突时，序号小的优先。

1. **领域内核稳定优先**（Stable Core）
   领域模型（文档、Block、富文本、Selection、Transaction、Layout 元数据）是整个系统里变化最慢的部分。所有其它层可以重写，`core` 的语义必须保持向后兼容。

2. **依赖单向下沉**（Acyclic Downward Dependencies）
   依赖只能指向更稳定的层。UI 依赖运行时，运行时依赖领域，领域不依赖任何人。禁止任何反向或环形依赖。

3. **能力经协商而非硬编码**（Capabilities over Conditionals）
   上层不得用 `if postgres / if sqlite` 这类后端判断分支决定行为。能力差异通过**契约上声明的 capability** 协商。这是 Cditor 已经在 `StorageCapabilities` 上验证过的模式，本文将其上升为全局法则。

4. **扩展以新增子系统实现**（Extend by Addition）
   新能力优先表现为新 crate / 新模块 + 在既有契约上实现一个 trait，而不是修改核心分发逻辑。核心对扩展**开放**，对修改**封闭**。

5. **可观测性与生产逻辑分离**（Observability is Orthogonal）
   trace、诊断、回放、性能度量不得内联进热路径的业务分支。它们通过独立的遥测契约旁路接入。

---

## 3. 分层现状（实测，2026-07）

下表是对代码的实测，不是理想图。行数含测试。

| 层 | crate | 生产行数(约) | 职责 | 稳定性 |
| --- | --- | --- | --- | --- |
| 领域内核 | `core` | 17K | 文档/Block/富文本/Selection/EditTransaction/Layout 元数据/schema/telemetry | **最稳** |
| 文本引擎 | `text` | 6K | Parley shaping、Bidi、geometry、paint plan（Parley 唯一消费者） | 稳 |
| 编辑算法 | `editor` | 6K | 无 UI 框架的窗口规划、虚拟滚动、anchor、hit-test | 稳 |
| 运行时 | `runtime` | 27K(+12K测试) | 活文档真相、事务应用、payload window、投影、调度、内容管线 | **热点** |
| 存储契约 | `store` | 2.2K | `DocumentStorage` trait + 缓存/去抖/恢复/乐观持久化 | 中 |
| 存储实现 | `store-postgres` / `store-sqlite` | 10K / 3.3K | 具体后端 | 中 |
| AI | `ai` | 0.8K | provider / 配置 / 流式协议 | 稳 |
| 应用外壳 | `app` | 38K | GPUI、平台输入、overlay、渲染、后端装配、API glue | **热点** |
| 白板 | `ding-board` | 12K | 独立白板产品（本文不覆盖） | 独立 |

分层方向正确，依赖无环。真正的结构张力集中在 `runtime` 与 `app` 的**边界**上——不是它们"太大"，而是它们**耦合太宽**。§5 用数据说明。

### 3.1 runtime 内部（实测）

`runtime` 的 27K 生产行里，78% 在 `document_runtime/`，其余按子系统分得很干净：

```text
runtime/src/
  document_runtime/   活文档真相；53 个文件全是 impl DocumentRuntime 的方法分组
    tests/            12K 行测试（占 document_runtime 的 40%）
    transaction_apply*.rs   事务原子应用（staging → 校验 → 提交）
    selection*.rs / text_*.rs / structure_*.rs / table/   领域动词分组
    projection.rs     读路径：DocumentRuntime → EditorViewProjection
  editing/            热路径（逐键输入）、EditingSession、CompositionController
  scheduling/         布局调度、主线程预算、worker pool、异步版本控制
  content/            payload window/cache、paste import、media cache、query index、安全
  projection/         无状态投影计算（list/view）
  acceptance/         大文档验收场景（10MB code block / 100k blocks / 50k rows）
```

**这套子系统划分是健康的，不需要拆 crate。** editing / scheduling / content / projection 各自边界清楚，公共 API 在 `lib.rs` 分区 re-export。runtime 的问题不在这里。

---

## 4. 数据流（实测）

Cditor 是一个**单向数据流**系统。理解这个环，就理解了整个架构。

```text
        ┌─────────────────────── 冷启动 ───────────────────────┐
        │                                                       │
   store 后端                                              DocumentRuntime
  (Postgres/SQLite)  ── LoadedDocument ──▶  cold_start  ──▶  (活文档真相)
        ▲                                                       │
        │                                                       │ 读路径
   StorageSaveBatch                                             ▼
        │                                              projection_for_window()
   持久化(去抖/乐观)                                            │
        │                                                       ▼
        │                                            EditorViewProjection
        │                                          (只读快照：可见 block/几何/光标)
        │                                                       │
        │                                                       ▼
        └──── apply_transaction ◀── 命令 ──── app (GPUI 渲染 + 平台输入)
                    ▲                                            │
             写路径 │                                            │ 用户输入
                    └────────────────────────────────────────────┘
```

**写路径（命令下行）**：用户输入 → app 翻译为对 runtime 的方法调用 → runtime 内部构造 `EditTransaction` → `apply_transaction` 走 **staging 模型**（所有变更先落 staging 副本，整树 preorder 校验通过才提交回 runtime；任一 op 违反前置条件则整个事务拒绝、状态零改动）→ 推进 `structure_version` / `content_version` / `revision` → 返回 `AppliedTransaction`（含受影响块的 dirty range）。

**读路径（投影上行）**：app 调 `projection_for_window()` → runtime 依据当前滚动窗口计算 `EditorViewProjection`（一个**只读快照**：可见 block、几何、光标位置）→ app 据此绘制 GPUI 树。app **不直接读 runtime 的内部状态**，只消费投影。

**持久化路径（旁路）**：事务应用后，变更经 `store` 的去抖（`height_write_debounce`）与乐观持久化（`optimistic_persistence`）异步落盘，不阻塞输入热路径。

### 4.1 三个关键设计优点

1. **事务的原子性 + 前置校验**（`transaction_apply.rs`）是这套架构最扎实的地方。staging → 校验 → 提交的模式意味着非法编辑无法污染文档状态，也是未来接入**协同**（远端 op 走同一入口）和**回放**（journal replay 走同一入口）的天然锚点。文件头注释明确写了 remote / migration / undo / redo 都收敛到这一个入口——这是极好的前瞻设计。

2. **能力协商契约**（`StorageCapabilities`）。`store` 的 trait 上声明 `payload_window / full_text_search / cloud_sync / server_authoritative`，上层据此协商而非硬编码后端判断。SQLite 与 Postgres 的差异（如全文搜索、服务器权威）通过 capability 表达。**这是全代码库最好的可扩展性范例，应推广到所有横向能力（见 §6）。**

3. **投影作为唯一读路径**。app 通过 `EditorViewProjection` 这一只读快照消费 runtime，而非直接读内部字段。这条边界如果保持纯粹，就是 UI 与运行时解耦的关键。

---

## 5. 核心张力（实测诊断，非推测）

前面说 runtime 和 app 的问题"不在于大，在于边界耦合太宽"。这里用数字证明。

### 5.1 张力一：runtime 是共享巨型可变状态

`DocumentRuntime`（`document_runtime/state.rs`）是**一个约 50 字段的结构体**：

```rust
pub struct DocumentRuntime {
    pub index: DocumentIndex,              pub visible_index: VisibleDocumentIndex,
    pub height_index: BlockHeightIndex,    pub page_layout: PageLayoutIndex,
    pub scroll: VirtualScrollState,        pub editing: Option<EditingSession>,
    pub payload_window: PayloadWindow,
    block_attrs, collection_records, comment_threads, assets, table_runtimes,
    text_models, selected_block_ids, document_selection, ai_session,
    undo_stacks, redo_stacks, external_undo_stack, typing_undo_group, ...
    // 约 50 个字段
}
```

53 个 `document_runtime/*.rs` 文件全部是 `impl DocumentRuntime` 的方法分组。**模块化发生在语法层（方法拆文件），但没发生在状态层**——同一份可变状态被广泛共享：

| 字段 | 被多少个文件直接读写 |
| --- | --- |
| `self.index` | 28 |
| `self.payload_window` | 28 |
| `self.document_selection` | 24 |
| `self.editing` | 16 |

**这不是"该拆 crate"的信号，恰恰相反。** 这些字段本就是紧密耦合的活文档真相——把它们拆到不同 crate 会制造跨 crate 的可变借用地狱。真正的改进方向是**在结构体内部划分内聚子结构**（如把 undo 相关的 5 个字段收进 `UndoState`、typing 相关的 3 个收进 `TypingState`），减少任意方法能触碰任意字段的表面积。这是**渐进的内部收敛**，不是分层重构。

### 5.2 张力二：app↔runtime 是一个 222 方法的宽命令表面

这是全架构**最重要的一个数字**：

- `DocumentRuntime` 暴露 **222 个公共方法**。
- `app` 直接调用了其中 **173 个**（`focus_block_at_offset`、`toggle_inline_mark_on_selection`、`caret_offset_for_block`、`replace_text_from_platform`…）。

也就是说，app 与 runtime 之间**没有一个窄接口**。app 直接操纵 runtime 的上百个编辑动词。读路径是干净的（走 `EditorViewProjection` 投影），但**写路径是散射的**——每加一个编辑功能，app 就多知道一个 runtime 方法，耦合宽度单调增长。

后果：
- 无法在不改 app 的情况下替换 runtime 的编辑实现。
- 协同/自动化/脚本这类"非人类输入源"想复用编辑逻辑，得各自重新拼这 173 个调用。
- 命令的权限、来源（`ChangeOrigin`）、可撤销性散落在调用点，而非集中表达。

**这是前一份 5 年提案没看到的真正问题。** 它担心"runtime 太重、app 太胖"要拆 crate，但真正的债是**写路径缺少一个命令抽象**。

### 5.3 结论：正确的方向是"收窄边界"，不是"拆分中枢"

runtime 的字段共享和 app 的宽调用面，都指向同一个解法：**在 app 与 runtime 之间引入一个显式的命令层**，而不是把 runtime 拆成更多 crate。见 §6。

---

## 6. 目标架构：收窄边界 + 能力契约

目标不是重画分层图（分层已经对了），而是给现有边界**装上契约**，让上百个方法调用收敛成少数几个稳定接口。

### 6.1 命令层（Command Seam）——解决 5.2

在 app 与 runtime 之间引入一个**命令**概念。所有对文档的修改意图统一表达为一个 `Command` 值，经单一入口进入 runtime：

```text
              现状（散射）                          目标（收窄）
   app ──173 个方法──▶ DocumentRuntime      app ──▶ Command ──▶ runtime.dispatch(cmd)
                                                                    │
   协同 ──自己重拼调用──▶ ...                协同 ──▶ Command ──────┤
   自动化 ──自己重拼调用──▶ ...              脚本 ──▶ Command ──────┘
                                            （所有输入源共用一个入口）
```

要点：
- `Command` 是一个**可序列化、带来源（`ChangeOrigin`）、带权限（`TransactionPermissionSet`）的值**。runtime 已有的 `apply_transaction` staging 模型正是它的落点——命令翻译成 `EditTransaction` 后走既有的原子应用入口。
- 读路径**不变**，仍是 `EditorViewProjection`。命令层只收窄**写路径**。
- app 的职责收缩为：`输入事件 → Command`（意图翻译）+ `EditorViewProjection → GPUI 树`（渲染）。它不再需要知道 173 个 runtime 方法，只需要知道 `Command` 的构造。
- 这一层是协同、自动化、脚本、宏录制的**共同复用点**：它们都产出 `Command`，无需各自重拼编辑逻辑。

> 落地方式：**先归类，再收窄**。不要一步到位定义"完美命令枚举"。先把 app 现在调用的 173 个方法按语义归组（文本编辑 / 结构编辑 / 选区 / 格式 / 焦点 / 表格 / AI…），每组先收敛成一个命令子集，逐组迁移。runtime 现有的 pub 方法在迁移期保留，迁移完成后降为 `pub(crate)`。

### 6.2 内部状态收敛——解决 5.1

在 `DocumentRuntime` 内部，把当前平铺的约 50 个字段按内聚度收进子结构（纯机械重构，不改行为，不改 crate 边界）：

```text
DocumentRuntime
  ├── DocumentModel   { index, visible_index, block_attrs, collection_records, ... }
  ├── LayoutState     { height_index, page_layout, scroll, pending_measured_heights }
  ├── UndoState       { undo_stacks, redo_stacks, external_undo_stack, typing_*, undo_events }
  ├── SelectionState  { document_selection, focused_*, selected_block_ids, visual_caret }
  ├── EditingState    { editing, hot_path, text_models, payload_window }
  └── AiState         { ai_session, next_ai_request_id }
```

好处：把"任意方法能触碰任意字段"的表面积，降为"方法声明它操作哪个子状态"。借用检查器会帮你发现哪些方法其实跨了本不该跨的状态边界——这是免费的耦合审计。

### 6.3 能力契约推广——把 `StorageCapabilities` 变成通用模式

`store` 的能力协商是全代码库最好的扩展范式。把同一模式用于所有**横向能力**：任何"可选的、后端/环境相关的"能力，都通过一个 capability 声明 + 一个 trait 接入，上层协商而非硬编码。

```text
StorageCapabilities   { payload_window, full_text_search, cloud_sync, server_authoritative }  ← 已存在
CollaborationCapability { presence, remote_cursors, conflict_resolution }                      ← 未来
SearchCapability        { full_text, semantic, cross_document }                                 ← 未来
```

规则：**上层禁止 `if postgres` / `if 有协同` 式的分支**，一律查 capability。

### 6.4 可观测性旁路

trace / 诊断 / 回放已有 `app/gui/diagnostics/` 和 `core/telemetry` 的基础。约束固化为：观测数据通过**事件/hook 旁路**产出，热路径只发射结构化事件，消费在别处。命令层（6.1）天然是遥测的好锚点——每个 `Command` 就是一条可记录、可回放的事件。

---

## 7. 扩展点登记（不提前建 crate）

未来能力**先登记为契约意图，不预先创建空 crate**。凭空设计的边界几乎必然是错的——协同会反向重塑 core 的事务模型，插件 API 必须由真实插件用例倒逼。每项在真实需求出现时才落地，落地时必须复用既有的锚点（命令层 / staging 入口 / 能力契约）。

| 未来能力 | 接入锚点（已存在的机制） | 落地信号 | 预期形态 |
| --- | --- | --- | --- |
| 协同编辑 | `apply_transaction` staging 入口 + `ChangeOrigin::Remote` | 有真实多人场景 | 新 crate `collaboration`，产出 `Command`/远端事务走同一入口 |
| 插件系统 | 命令层（6.1）+ 投影只读快照 | 有 ≥2 个真实插件用例 | `plugin-api`(trait) + `plugin-host`；插件只能发 `Command`、读投影 |
| 云同步 | `StorageCapabilities.cloud_sync` + sync_outbox（Postgres 已有） | 需跨设备 | 在能力契约下新增实现，非新分层 |
| 全文/语义搜索 | `StorageCapabilities.full_text_search` + runtime `query_index` | 需跨文档搜索 | `SearchCapability` trait，Postgres FTS 已是雏形 |
| 自动化/脚本 | 命令层（6.1） | 需宏/批量编辑 | 脚本引擎产出 `Command` 序列，复用命令入口 |
| 遥测/回放 | 命令层 + `core/telemetry` | 需性能回放 | 记录 `Command` 事件流，重放走同一入口 |

**关键洞察：上表所有能力都收敛到同两个锚点——命令层（写）和投影（读）。** 这就是为什么 §6.1 的命令层是 5 年演进里回报最高的一笔投资：它一次性为协同、插件、自动化、遥测铺好了共同入口。反之，如果现在就去拆 runtime crate，这些能力反而没有统一接入点。

---

## 8. 架构不变量（评审 checklist）

PR 评审时对照，违反需在 PR 说明里显式论证：

1. **依赖无环、单向下沉**。`core` 不依赖任何本地 crate；`runtime` 不依赖 `store-postgres`/SQLx/GPUI/Parley；除 `text` 外不得直接依赖 Parley。（`scripts/dev/check_structure.sh` 已强制部分）
2. **写路径经命令层**（目标态）。app 新增编辑功能应产出 `Command`，不新增对 runtime 具体方法的直接调用。迁移期内至少不扩大 173 这个数字。
3. **读路径经投影**。app 不得新增对 `DocumentRuntime` 内部状态的直接读取，一律走 `EditorViewProjection`。
4. **能力经协商**。禁止 `if postgres`/`if sqlite`/后端类型判断式分支；差异走 capability。
5. **文档修改的原子性**。所有文档状态变更走 `apply_transaction` 的 staging 模型；不得在其外直接改 `index`/payload。
6. **非白板文件 ≤ 700 行**（既有规则）。
7. **可观测性不入热路径**。诊断/trace 通过事件旁路，不加业务分支。
8. **新能力先登记后建 crate**（§7）。不创建无实现的占位 crate。

---

## 9. 落地路线图（按回报排序，非时间承诺）

每步独立可交付、可回滚，不要求一次做完。

**近期（结构收敛，低风险高回报）**
1. `store-sqlite` 的 `cditor-runtime` 从 `[dependencies]` 移到 `[dev-dependencies]`（只被 `tests/journal_recovery.rs` 用）。一行修复，纠正分层。
2. 收敛 `store`：把 `layout_cache`/`height_write_debounce`/`cache_recovery`/`optimistic_persistence` 与 `DocumentStorage` 契约分离，让 `store` 的 trait 层更纯（可保持同 crate，仅模块归组）。
3. `DocumentRuntime` 内部状态按 §6.2 收进子结构。纯机械重构，借用检查器护航。

**中期（命令层，架构关键投资）**
4. 定义 `Command` 类型与 `runtime.dispatch(Command)` 单一写入口，翻译到既有 `apply_transaction`。
5. 按语义分组，把 app 的 173 个直接调用逐组迁移到 `Command`。每迁移一组，把对应 runtime pub 方法降为 `pub(crate)`。
6. 命令层接入遥测：每个 `Command` 产生一条可记录事件。

**长期（平台化，按真实需求触发）**
7. 需求出现时，按 §7 落地协同 / 插件 / 同步 / 搜索，全部复用命令层 + 投影 + 能力契约。

---

## 附：一句话架构

> Cditor 是一个**单向数据流**的富文本编辑器：`core` 定义稳定领域，`runtime` 持有活文档真相并以**原子事务**修改它，app 通过**命令下行、投影上行**驱动它。5 年演进的核心不是拆分中枢，而是**给 app↔runtime 那条 222 方法的宽边界装上命令契约**——这条命令 seam 是协同、插件、自动化、遥测的共同入口。
