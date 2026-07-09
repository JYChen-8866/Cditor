# Table Runtime Rework Analysis

## 背景

当前 V2 的表格仍然会在回车、切换 block、点击其他位置后消失。前面已经补过多处防御逻辑，例如 `RichBlockKind::Table` 自动补 `BlockPayload::Table`、表格 block 没有 cell focus 时禁止普通文本输入、表格 cell 的按键优先级提前到 slash menu 之前。

这些补丁可以覆盖一部分路径，但没有改变根因：V2 现在把表格当成普通 block payload 的一个分支，而旧版 `/Users/jychen/Desktop/Cditor/src` 把表格当成 block 内部的独立运行时。两者的状态模型不一样，所以 V2 很容易在 focus、projection、payload loading、保存回写之间出现一次短暂不一致，然后 UI 就把表格渲染成空 payload 或普通文本。

本文先梳理旧版和 V2 的实现思路，再给出按旧版方式重做 V2 表格的落地方案。

## 旧版实现

旧版主要看 `/Users/jychen/Desktop/Cditor/src/editor2`。

### Block 拥有表格运行时

`/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs` 里，`CditorBlock` 同时持有持久化记录和表格运行时：

- `record: BlockRecord`
- `table_runtime: Option<TableRuntime>`

构造 block 时，如果 `record.kind == BlockKind::Table`，会用 `TableRuntime::build(record.table.as_ref())` 创建运行时。也就是说，旧版不是每次 render 都临时从 payload 推导表格状态，而是 block entity 自己保存一份表格 runtime。

关键路径：

- `CditorBlock::new` 创建 `table_runtime`
- `ensure_table_cell_entities` 为每个 cell 创建稳定的 `CditorTableCell` entity
- `sync_kind_dependent_caches` 在 kind 变化时重建或清空 `table_runtime`
- `focus_table_cell` 只更新 `table_runtime.focused_cell`
- `replace_focused_table_cell_range` 先改 runtime，再通过 `update_table_cell_record` 写回 `record.table`

这个模型有一个重要特征：表格的编辑状态不会依赖普通 block 文本模型。表格 cell 是表格 runtime 的一部分，不是 block title 或 rich text payload 的一段文本。

### 表格渲染是稳定实体树

`/Users/jychen/Desktop/Cditor/src/editor2/component/table/mod.rs` 的 `render_table_block` 直接读取 `block.table_runtime.as_ref()`。

渲染流程是：

1. block 读自己的 `table_runtime`
2. runtime 里有 row/cell 结构
3. 每个 cell 对应一个稳定的 `CditorTableCell` entity
4. 每次 render 只同步 `TableCellViewState`
5. cell 自己负责文本、光标、选中态、输入态

这和 V2 的差别很大。旧版表格不是 projection 里一个临时 `TablePayload`，而是一棵有 identity 的 UI/entity 结构。只要 block 还是 table，cell entity 就不会因为 block focus 或普通输入刷新而消失。

### Slash 插入保证 kind/table 成对出现

`/Users/jychen/Desktop/Cditor/src/editor2/runtime/indexed_document.rs` 的 slash 插入走 `apply_slash_menu_item`，核心是 `slash_replacement_record`：

- 设置 `replacement.kind = kind`
- 设置 `replacement.table = default_table_for_slash_insert(kind)`
- 当 kind 是 `BlockKind::Table` 时，`default_table_for_slash_insert` 创建默认 2 行 2 列 `TableData`
- 清空 children，重新计算高度

旧版的强约束是：`BlockKind::Table` 必须同时拥有 `record.table = Some(TableData)`。不会出现 table kind 但 payload 是 paragraph/rich text 的中间态。

### 表格输入优先于普通编辑器命令

旧版按键处理里，表格 cell focus 的路径优先级很高。只要 `focused_table_cell` 存在，`escape/backspace/delete/space/enter/字符输入` 都先交给表格 cell，处理完直接返回，不再走 slash menu、block enter、普通文本输入。

这避免了一个关键问题：用户在表格 cell 里按回车时，不应该触发 block split；用户在表格 block 上输入时，不应该把整个 table payload 改成普通文本 payload。

## V2 当前实现

V2 相关文件主要在：

- `crates/core/src/rich_text/table.rs`
- `crates/core/src/rich_text/payload.rs`
- `crates/engine/src/document_runtime/*`
- `crates/app/src/gui/block/table.rs`
- `crates/app/src/gui/input/mouse.rs`
- `crates/app/src/gui/app/input/keyboard.rs`

### 表格只是 BlockPayload 的一个 variant

核心数据结构是：

- `RichBlockKind::Table`
- `BlockPayload::Table(TablePayload)`
- `TablePayload { rows, header_rows, header_cols }`

理论上 `RichBlockKind::Table` 应该配 `BlockPayload::Table`，但 V2 的很多通用路径都可以只处理 kind 或只处理 payload：

- block kind 转换
- plain text split
- markdown paste
- undo/redo
- payload window loading
- projection
- composition preview
- generic text edit

只要有一条路径把 table block 的 payload 改成了 rich text/code/html/plain text，表格 UI 就没有可靠 runtime 可以兜底。

### Focus 是全局状态

V2 的表格焦点存在 `DocumentRuntime.focused_table_cell: Option<FocusedTableCell>`。这比旧版集中，但也更脆弱：

- block focus 和 table cell focus 是两套状态
- GUI 点击通过 table cell layout cache 反推 offset
- projection 每次读取当前 runtime 状态生成 `ViewBlockSnapshot`
- table render 根据 snapshot 决定 active cell

旧版是 cell entity 自己拥有稳定状态；V2 是一次 render 快照里带一个全局 focus 标记。

### 表格渲染没有独立 entity runtime

`crates/app/src/gui/block/table.rs` 现在用 `TableCellTextElement` 直接渲染 cell 文本，layout 信息回写到 `CditorV2View.table_cell_layouts`。

这能画出来，但没有旧版那种稳定 cell entity：

- 没有 per-cell entity identity
- 没有 per-cell revision 驱动
- 没有单独的 cell input component 生命周期
- cell layout cache 是附属缓存，不是表格状态源

因此，只要 projection 临时给了空 payload、placeholder payload 或 kind/payload 不匹配，表格就会在 UI 上消失。

## 根本差异

| 维度 | 旧版 `/Users/jychen/Desktop/Cditor/src` | V2 当前实现 |
| --- | --- | --- |
| 状态源 | `CditorBlock.record.table` + `table_runtime` | `BlockPayload::Table` + 全局 `focused_table_cell` |
| UI identity | row/cell 都是稳定 entity | render 时从 payload 临时画 cell |
| kind/payload 约束 | `BlockKind::Table` 和 `record.table` 成对创建 | 需要各个路径手动 normalize |
| 输入优先级 | cell focus 时先处理表格输入并返回 | 已补优先级，但仍依赖全局状态 |
| 表格文本模型 | cell 独立文本范围 | 容易被普通 block text model 路径影响 |
| 切换 block | block 内 table runtime 保留 | projection/payload/focus 任一处不同步都可能丢 |
| 保存回写 | `update_table_cell_record` 写回 table record | payload window 和 loaded payload snapshot 回写 |

## 为什么 V2 还会消失

从日志看，表格最初是正常的：

```text
rows=2 cols=2 focused=true payload_loaded=true
```

后面突然变成：

```text
rows=0 cols=0 focused=false payload_loaded=true
```

这说明问题不只是 GUI 没画出来，而是 runtime/projection 看到的 table payload 已经变成空表格，或者 table block 被一条路径重写成了空 payload。当前补丁虽然增加了 normalize，但它仍是“事后修复”：

1. slash 插入或转换时能创建默认 2x2。
2. 某些加载路径能修复空表格。
3. 某些普通输入路径能避免覆盖 table。

但 V2 没有旧版那种 block 内部 table runtime，所以当发生以下情况时仍然危险：

- 切换 focus 触发 projection，payload window 给了空 table 或 loading payload。
- 普通 block 编辑路径在没有 cell focus 时处理了 table block。
- 保存回写把某次空 table snapshot 当成真实数据写入。
- undo/redo 或 composition preview 生成了 kind/payload 不匹配的记录。
- GUI click/keyboard 先 blur table cell，再让 block text path 接管当前 table block。

旧版不会这么容易丢，是因为 table runtime 是 block entity 的一等成员。即使渲染刷新、focus 改变、菜单关闭，block 里仍有 `table_runtime` 和 `record.table`。

## 目标实现

V2 应该照旧版思路重做表格，而不是继续在通用路径上补判断。

### 1. 建立表格运行时模块

在 engine 里建立独立模块，例如：

```text
crates/engine/src/document_runtime/table/
  mod.rs
  runtime.rs
  focus.rs
  edit.rs
  projection.rs
  defaults.rs
```

核心类型：

```rust
pub struct TableRuntime {
    pub rows: Vec<TableRowRuntime>,
    pub focused_cell: Option<TableCellPosition>,
    pub revision: u64,
}

pub struct TableRowRuntime {
    pub cells: Vec<TableCellRuntime>,
}

pub struct TableCellRuntime {
    pub text: String,
    pub revision: u64,
}

pub struct TableCellPosition {
    pub row: usize,
    pub col: usize,
}
```

这套 runtime 从 `BlockPayload::Table` 构建，但编辑时优先修改 runtime，再同步回 payload。不要让普通 block text model 直接修改 table payload。

### 2. 明确 table block invariant

在 runtime 层定义唯一入口：

```rust
fn ensure_table_block(block_id: BlockId) -> &mut TableRuntime
```

规则：

- `RichBlockKind::Table` 必须拥有非空 `BlockPayload::Table`
- table payload 为空时修复为默认 2x2
- 非 table kind 不能持有 table runtime
- 从其他 kind 转为 table 时，原 block 文本只进入第一个 cell
- 从 table 转为其他 kind 时，按明确策略导出 plain text

这个 invariant 应该集中在一个模块里执行，不要散在 `structure_edit.rs`、`text_payload.rs`、`projection.rs`、`undo_redo.rs`、`payload_window.rs`。

### 3. Projection 输出表格视图状态，而不是裸 payload

当前 `ViewBlockSnapshot` 主要带 block payload。表格应该改为输出专门的 view state：

```rust
pub struct TableViewState {
    pub rows: Vec<TableRowViewState>,
    pub focused_cell: Option<TableCellPosition>,
    pub focused_offset: Option<usize>,
    pub revision: u64,
}

pub struct TableCellViewState {
    pub text: String,
    pub revision: u64,
    pub active: bool,
}
```

GUI 不应该自己猜 table 是否有效；如果 block 是 table，projection 就必须给出可渲染的 table view state。

### 4. GUI 使用稳定 row/cell component

照旧版 `CditorTableCell` 的思路，在 app 层建立：

```text
crates/app/src/gui/block/table/
  mod.rs
  row.rs
  cell.rs
  input.rs
  layout.rs
```

每个 cell 应该有稳定 identity：

- `block_id`
- `row`
- `col`
- `revision`

cell component 负责：

- 鼠标点击定位 offset
- caret 绘制
- IME bounds
- 文本选择
- backspace/delete/enter/普通字符
- layout cache

表格 block renderer 只负责行列结构和边框，不直接承担所有输入细节。

### 5. 表格输入必须有最高优先级

键盘入口应保持这种顺序：

1. 如果有 focused table cell，先处理 table cell 输入。
2. 如果 table block focused 但没有 cell focus，只允许方向键、escape、点击进入 cell 等安全行为。
3. slash menu/code toolbar/普通 block enter 在表格 cell 输入之后。

也就是说，回车在表格 cell 内永远不能走 block split。

### 6. Slash 插入必须原子化

slash menu 插入 table 时，不能先改 kind 再等后续 normalize。应该一个事务同时写入：

- `kind = Table`
- `payload = default 2x2 TablePayload`
- `table_runtime = TableRuntime::from_payload(payload)`
- focus 第一格或 block，取决于产品规则
- 清掉当前 slash query 文本
- 同步 undo snapshot

这要照旧版 `slash_replacement_record + default_table_for_slash_insert` 的思路做。

### 7. 保存只保存真实 table 数据

保存层要避免把临时 projection、placeholder、空 table snapshot 写回数据库。

建议规则：

- 只有 runtime dirty 的 payload 才保存。
- table runtime 保存时从 runtime 生成 payload。
- 空 table 不是合法保存状态，除非用户显式删除所有行列。
- 加载到 `kind=Table` 但 payload 空或非 table 时，修复为默认 2x2，并标记需要保存修复结果。

## 任务列表

### Phase 1: 收口 table invariant

- [ ] 新建 `crates/engine/src/document_runtime/table/`。
- [ ] 移动默认 2x2 构造、payload normalize、table kind 转换逻辑到 table 模块。
- [ ] 删除散落在多个文件里的重复 `default_table_payload` 判断。
- [ ] 给 `kind=Table + 非 table payload`、`kind=Table + 空 table payload`、`table -> paragraph`、`paragraph -> table` 写集中单测。

### Phase 2: 引入 TableRuntime

- [ ] 在 `DocumentRuntime` 中增加 `table_runtimes: HashMap<BlockId, TableRuntime>`。
- [ ] payload window 加载 table block 时创建 runtime。
- [ ] block kind 转为 table 时创建 runtime。
- [ ] block kind 离开 table 时移除 runtime。
- [ ] 表格 cell 编辑只改 runtime，再同步 payload。
- [ ] 删除 table block 对普通 `PieceTableTextModel` 的依赖。

### Phase 3: Projection 改成 TableViewState

- [ ] `ViewBlockSnapshot` 为 table 增加专门的 `TableViewState`。
- [ ] projection table 分支必须从 `TableRuntime` 读取。
- [ ] projection 不再从不可信 payload 临时推导表格结构。
- [ ] 加测试：切换 block、回车、点击外部后 projection 仍然输出 2x2 table。

### Phase 4: GUI 拆出 table component

- [ ] 把 `crates/app/src/gui/block/table.rs` 拆成目录模块。
- [ ] 建立 `TableCellComponent` 或等价的稳定 cell state。
- [ ] cell component 接管点击定位、caret、selection、IME bounds。
- [ ] table renderer 只负责结构布局和装饰。
- [ ] 保留当前主题和边框样式。

### Phase 5: 输入事件按旧版顺序重排

- [ ] keyboard 入口第一优先级处理 focused table cell。
- [ ] table cell 的 enter/backspace/delete/space/char 全部在 table 模块内处理。
- [ ] block split、slash menu、toolbar 不得在 table cell focus 时抢事件。
- [ ] 鼠标点击 cell 后应保持 table block 和 cell focus 同步。

### Phase 6: 持久化和修复

- [ ] 保存路径只保存 runtime 产生的合法 table payload。
- [ ] 加载旧数据时修复空 table 或错误 payload。
- [ ] 对 PostgreSQL payload roundtrip 写测试。
- [ ] 确认 undo/redo 不会把 table 变成普通文本 payload。

### Phase 7: 清理防御补丁

- [ ] 删除重复 normalize。
- [ ] 删除为了兜底而分散在 text edit/composition/projection 的 table 特判。
- [ ] 保留必要的 assert 或 debug trace，确保 invariant 破坏时能立刻定位。

## 后续实施方案

后续不要一次性大改 GUI。正确顺序是先把 engine 里的表格状态源修稳，再让 projection 和 GUI 逐步从这个状态源读取。每一步都要能独立提交、独立测试。

### Step 0: 先冻结当前问题

目标是把“表格从 2x2 变成 0x0”变成稳定可复现的测试，而不是靠肉眼点编辑器。

改动范围：

- `crates/engine/src/document_runtime/tests.rs`
- 必要时新增更小的测试 helper

要补的测试：

- slash menu 或 kind 转换后得到 2x2 table。
- table block focus 后切到 paragraph，再切回 table，仍然是 2x2。
- table block 没有 cell focus 时按回车，不 split，不清空 rows。
- table cell focus 时按回车，只修改 cell 内容，不触发 block split。
- projection 连续刷新多次，table rows/cols 不变。

完成标准：

```text
cargo test -p cditor-runtime table --lib
cargo test -p cditor-app table --lib
```

这一步不解决问题也可以，但必须让后续每次改动都能测出是否退化。

### Step 1: 把 table invariant 收口到一个模块

现在 `default_table_payload`、`normalize_payload_for_kind`、`payload_for_kind_from_plain_text` 分散在多个文件里。后续要先收口，否则每修一条路径，另一条路径还会漏。

新增目录：

```text
crates/engine/src/document_runtime/table/
  mod.rs
  defaults.rs
  invariant.rs
```

模块职责：

- `defaults.rs` 只负责默认表格构造和 plain text 到 table 的转换。
- `invariant.rs` 只负责保证 `RichBlockKind::Table` 和 `BlockPayload::Table` 成对出现。
- 其他 runtime 文件不得自己手写默认 2x2。

需要迁移的现有逻辑：

- `crates/engine/src/document_runtime/mod.rs` 里的 `normalize_payload_for_kind`
- `crates/engine/src/document_runtime/mod.rs` 里的 `default_table_payload`
- `crates/engine/src/document_runtime/text_payload.rs` 里的 table 分支
- `crates/engine/src/document_runtime/structure_edit.rs` 里的 table payload 构造分支

完成标准：

- 全项目只有 table 模块能创建默认 table payload。
- `rg "default_table_payload|normalize_payload_for_kind" crates/engine/src/document_runtime` 能确认没有散落实现。
- `cargo test -p cditor-runtime table --lib` 通过。

### Step 2: 引入 engine 层 TableRuntime

这一步是核心。V2 要从“projection 临时读 payload”改成“table runtime 是状态源”。

新增目录：

```text
crates/engine/src/document_runtime/table/
  runtime.rs
  edit.rs
  focus.rs
  sync.rs
```

建议结构：

```rust
pub struct TableRuntime {
    rows: Vec<TableRowRuntime>,
    focused_cell: Option<TableCellPosition>,
    revision: u64,
    dirty: bool,
}
```

`DocumentRuntime` 增加：

```rust
table_runtimes: HashMap<BlockId, TableRuntime>
```

同步规则：

- block 加载为 table 时：从 payload 构建 runtime。
- block 转为 table 时：创建默认 2x2 runtime，原文本进第一个 cell。
- block 离开 table 时：移除 runtime。
- cell 编辑时：修改 runtime，标记 dirty，再同步回 payload window。
- payload 保存时：从 runtime 导出 `BlockPayload::Table`。

这一步完成后，table payload 就不是唯一状态源，而是 table runtime 的持久化结果。

完成标准：

- table cell 编辑不创建普通 `PieceTableTextModel`。
- table block 没有 cell focus 时普通文本输入不能覆盖 table runtime。
- 点击其他 block 或刷新 projection 后 runtime 仍然有 rows。
- `cargo test -p cditor-runtime table --lib` 通过。

### Step 3: Projection 读取 TableRuntime

当前 projection 从 payload window 构造 `ViewBlockSnapshot`。这一步要改成：table block 的 view state 从 `TableRuntime` 来。

建议新增：

```rust
pub struct TableViewState {
    pub rows: Vec<TableRowViewState>,
    pub focused_cell: Option<TableCellPosition>,
    pub focused_offset: Option<usize>,
    pub revision: u64,
}
```

`ViewBlockSnapshot` 对 table 有两种可选方案：

1. 保留 `payload: BlockPayload`，额外增加 `table_view: Option<TableViewState>`。
2. 把 block content 拆成 enum，例如 `ViewBlockContent::Table(TableViewState)`。

推荐第二种，更干净，也更符合“不同 block kind 有不同 view model”的方向。但如果改动面过大，先用第一种过渡。

完成标准：

- table render 不再直接信任 payload rows。
- projection 里如果 table runtime 不存在，要立刻通过 invariant 修复，而不是输出空 table。
- 连续 projection 不会出现 `rows=2` 后又无原因变成 `rows=0`。

### Step 4: GUI 表格拆目录，但先复用现有绘制

这一步不要先追求完整重写视觉。先把 `crates/app/src/gui/block/table.rs` 拆干净，让输入和绘制边界清楚。

目标目录：

```text
crates/app/src/gui/block/table/
  mod.rs
  render.rs
  cell.rs
  hit_test.rs
  layout.rs
  input.rs
  style.rs
```

职责划分：

- `render.rs` 负责 table/row/cell 结构。
- `cell.rs` 负责 cell 文本元素和 caret。
- `hit_test.rs` 负责鼠标位置到 cell offset。
- `layout.rs` 负责 cell layout cache。
- `input.rs` 负责 GUI 到 runtime 的输入桥接。
- `style.rs` 负责边框、背景、hover、focus 样式。

完成标准：

- `crates/app/src/gui/block/table.rs` 不再是大文件。
- 鼠标点击、滚轮、选中、hover 行为保持现状。
- 不引入视觉回退。

### Step 5: 把 cell 输入做成稳定 component

这一步再照旧版 `CditorTableCell` 做稳定 cell component。它的价值是解决 IME、光标、selection 和 layout cache 生命周期。

每个 cell 的 identity：

```text
block_id + row + col
```

cell component 输入：

- `TableCellViewState`
- active/focused 状态
- theme/style
- editable flag

cell component 输出事件：

- focus requested
- text replace requested
- caret moved
- composition started/updated/committed
- measured layout changed

完成标准：

- 中文 IME 在 cell 内完整工作。
- 光标位置和文字贴合。
- 点击 cell 不会让 table block 被普通编辑路径接管。
- cell 内容编辑后 undo/redo 可恢复。

### Step 6: 重排事件优先级

旧版最关键的一点是：表格 cell focus 时，表格输入路径先于一切 block 命令。

目标顺序：

```text
table cell input
slash menu input
code toolbar input
block drag/selection
generic block text input
structure commands
```

需要检查：

- `crates/app/src/gui/app/input/keyboard.rs`
- `crates/app/src/gui/input/mouse.rs`
- `crates/app/src/gui/overlay/slash_menu.rs`
- `crates/app/src/gui/block/code_toolbar.rs`

完成标准：

- cell focus 时按回车不会 split block。
- cell focus 时按 `/` 只是输入 `/`，不会打开 slash menu，除非产品明确要求 cell 内也支持 slash。
- cell focus 时 delete/backspace 只影响 cell 文本。

### Step 7: 持久化只从合法 runtime 导出

PostgreSQL 保存不能写入中间态。table block 保存前要从 `TableRuntime` 导出 payload。

需要检查：

- `crates/store-postgres/src/stores/payload.rs`
- `crates/engine/src/document_runtime/payload_window.rs`
- `crates/engine/src/document_runtime/store_loading.rs`
- app 层触发保存的 dirty snapshot

规则：

- table runtime dirty 才保存 table payload。
- 空 table 不能被静默保存，除非后续支持删除所有行列的明确命令。
- 加载坏数据时修复为默认 2x2，并产生一次 repair dirty 标记。

完成标准：

- 编辑表格后重启，表格仍然存在。
- 数据库里 table payload rows/cols 正确。
- 旧的坏数据不会让编辑器启动后渲染空白。

### Step 8: 删除临时日志和防御代码

表格稳定后，要清理之前为了定位问题加的 runtime/gui trace。

要删除：

- `[cditor][table][runtime]...`
- `[cditor][table][gui]...`
- 散落的兜底 normalize
- 和新 `TableRuntime` 重复的 table 特判

要保留：

- debug assertion
- 集中 invariant 修复
- 单元测试和回归测试

完成标准：

```text
cargo check --workspace
cargo test --workspace --quiet
```

### 推荐提交顺序

1. `test(table): capture table disappearance regressions`
2. `refactor(engine): centralize table payload invariant`
3. `feat(engine): add table runtime as source of truth`
4. `refactor(engine): project table view state from runtime`
5. `refactor(app): split table gui modules`
6. `feat(app): add stable table cell component`
7. `fix(app): route table cell input before block commands`
8. `fix(store): persist table payload from runtime`
9. `chore(table): remove temporary table traces`

这样拆的好处是，每个提交都能解释清楚为什么做、改了哪里、怎么测。如果中间出问题，也能知道是 engine 状态源、projection、GUI component 还是持久化出了问题。

## 验收标准

- slash menu 插入表格默认 2 行 2 列。
- 点击表格内任意 cell，表格不消失。
- 在 cell 内输入中文 IME，候选窗位置正确，提交后内容在 cell 内。
- 在 cell 内按回车，不触发 block split，不让表格消失。
- 点击其他 block，再点回表格，表格结构和内容都保留。
- 修改表格后保存到 PostgreSQL，重启编辑器后表格仍然存在。
- undo/redo 能恢复表格结构和 cell 内容。
- projection 日志不再出现正常 2x2 表格无用户删除行为却变成 `rows=0 cols=0`。

## 结论

这次不应该继续只修 `handle_enter` 或某个 render 分支。旧版稳定的原因是表格拥有独立 runtime、稳定 cell entity、原子 slash replacement、表格输入高优先级。V2 要照这个方向把表格从普通 payload 特判升级为一等 block runtime。

推荐下一步先做 Phase 1 和 Phase 2：把 table invariant 收口，并引入 `TableRuntime`。只要 runtime 成为表格状态源，后面的 GUI 和 IME 才有稳定基础。
