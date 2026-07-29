# V1 编辑器操作迁移总表：Block 合并 / 分列 / 方向键跨 Block / 删除移动

> 目标：把 `/Users/jychen/Desktop/Cditor` V1/editor2 中已经验证过的编辑器交互系统迁移到 CDitor-V2。本文先列完整操作表和任务拆解，后续实现时每完成一项勾选一项。
>
> 硬约束：继续遵守 `doc/large-document-rich-text-architecture.md`：runtime/document index/selection/layout/scroll 是真相；UI 只发 command 和渲染 projection；不能用 GPUI entity / ListState 作为 10w 文档结构或滚动真相；输入/IME hot path 不同步等待 PostgreSQL、全量 payload、全局 layout。

---

## 1. V1 参考源码

| 功能 | V1 文件 | 关键点 |
|---|---|---|
| Block 事件全集 | `/Users/jychen/Desktop/Cditor/src/editor2/block/event.rs` | `RequestNewline`、`RequestMergeIntoPrevious`、`RequestDelete`、`RequestIndent`、`RequestOutdent`、`RequestMoveUp/Down`、`RequestFocusPrevious/Next` |
| 键盘分发 | `/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs` | left/right/up/down/delete/backspace/enter/tab/shortcut 先在 block entity 判断，再发 runtime 事件 |
| Indexed runtime | `/Users/jychen/Desktop/Cditor/src/editor2/runtime/indexed_document.rs` | 大文档版本的 merge/delete/focus/move/selection/drag 行为 |
| Tree runtime | `/Users/jychen/Desktop/Cditor/src/editor2/runtime/document.rs` + `tree.rs` | 更准确的树结构移动语义，可作为 V2 结构真相参考 |
| 表格内部编辑 | `/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs` | table focused cell 的 Backspace/Tab/方向键特殊处理 |

---

## 2. V1 已确认的核心行为

### 2.1 Backspace / Delete

V1 `entity.rs`：

```rust
"backspace" => {
    if !selection.empty { RequestDeleteSelection }
    else if cursor_offset == 0 && !uses_soft_enter(kind) {
        if empty { RequestDelete }
        else { RequestMergeIntoPrevious { content } }
    } else if delete_backward() { Changed }
}
```

V1 `delete`：

```rust
"delete" => {
    if !selection.empty { RequestDeleteSelection }
    else if delete_forward() { Changed }
    else if empty && !uses_soft_enter(kind) { RequestDelete }
}
```

V1 runtime `merge_block_into_previous`：

1. 找 previous visible block。
2. 把当前 block plain text append 到 previous 末尾。
3. previous selection 设置为 appended range。
4. 删除当前 block。
5. focus previous。
6. 发 `ReplaceInlineSpans(previous)` + `DeleteBlock(current)` 结构事务。

V2 已增强一条产品规则：caret=0 且当前是可文本化样式块时，先退回 Paragraph，保留正文，不合并/删除。

### 2.2 方向键

V1 `entity.rs`：

| 键 | V1 行为 |
|---|---|
| Left | block 内移动；若有 selection，收缩到 selection start |
| Shift+Left | block 内扩展 selection |
| Right | block 内移动；若有 selection，收缩到 selection end |
| Shift+Right | block 内扩展 selection |
| Up | block 内按 layout cache 垂直找上一行 offset；找不到时 `RequestFocusPrevious` |
| Shift+Up | block 内扩展 selection；找不到时跨 block focus previous/selection endpoint |
| Down | block 内按 layout cache 垂直找下一行 offset；找不到时 `RequestFocusNext` |
| Shift+Down | block 内扩展 selection；找不到时跨 block focus next/selection endpoint |

V1 runtime `focus_adjacent_block(block_id, ±1)` 聚焦相邻 visible block。

### 2.3 Enter / Cmd+Enter / Tab / Shift+Tab

已在 `doc/archive/migrations/v1-list-enter-tab-shifttab-migration-plan.md` 细化。当前 V2 基本完成：

- 普通 Enter split/inherit kind。
- Cmd/Ctrl+Enter split but new Paragraph。
- 空列表 Enter 退出或 outdent。
- Tab/Shift+Tab 结构 indent/outdent。
- Code/RawMarkdown soft-tab。

### 2.4 Block Move Up / Down

V1：

- `Cmd/Ctrl+Shift+Up/Down` 或 `Alt+Up/Down` 发 `RequestMoveUp/Down`。
- runtime 调 `move_block_by_index_delta` 或 tree runtime 的 `tree.move_up/down`。
- 移动后 focus 保持当前 block。
- 结构事务为 `MoveBlock`。

V2 当前有 gutter drag reorder，但键盘 move up/down 未接入 command。

### 2.5 跨 block selection / 鼠标拖选

V1 indexed runtime：

- `TextSelectionDragStart { block_id, offset }` 保存 anchor。
- root `on_mouse_move` 根据 pointer 找当前 hydrated block 和 offset。
- `apply_text_selection_range(anchor, focus)` 按 index 范围给每个 hydrated block 设置局部 selection。
- copy/cut 从 hydrated entities 读；V2 不应照搬，应从 runtime `DocumentSelection + store/cache` 读。

V2 已有 `DocumentSelection` 和部分鼠标拖选，但方向键跨 block selection 还不完整。

### 2.6 Slash menu / block kind apply

V1：

- `/` query 只在 Paragraph、无 selection、slash 前为空白或行首时触发。
- Up/Down 移动 slash menu selection。
- Enter apply selected slash item。
- Escape close。

V2 当前尚未完整迁移 slash menu。

### 2.7 Table 内部编辑

V1 table focused cell 特殊逻辑：

| 键 | V1 行为 |
|---|---|
| Backspace | 删除 cell 内文本 |
| Tab | 下一个 cell |
| Shift+Tab | 上一个 cell |
| Left | cell 内左移；到边界时切上一个 cell |
| Right | cell 内右移；到边界时切下一个 cell |
| Up/Down | 上/下 cell |
| Escape | 退出 table cell focus |

V2 有 `BlockEditorModel` 的 table hit-test 基础，但真实 table cell 编辑还未完整迁移。

### 2.8 Block 分列 / Columns

V1 editor2 当前扫描到的 `BlockKind` 与 runtime event 中没有稳定的 column block 事务入口；`BlockKind` 也未看到明确 `Column/Columns` 类型。结论：

- V1/editor2 没有像 Notion columns 那样完整可迁移的 column block 实现，至少不是在已扫描的 editor2 主链路里。
- V2 若要做“block 分列”，应作为新能力设计，不能伪称从 V1 直接搬。
- 要符合大文档架构：Columns 应是结构化 container block 或 layout attrs，不应是 UI-only flex 排版。

建议 V2 设计：

```text
ColumnsGroup block
  Column block 1
    child blocks...
  Column block 2
    child blocks...
```

或：

```text
block_attrs.layout = { display: columns, column_group_id, column_index, column_width }
```

优先推荐第一种 container 结构，便于 selection、copy/paste、drag、Postgres 持久化和可见索引处理。

---

## 3. V2 当前状态对照表

| 类别 | 操作 | V1 行为 | V2 当前状态 | 差距 | 优先级 |
|---|---|---|---|---|---|
| 文本删除 | Backspace 删除字符 | grapheme/char boundary 删除 | 已有 `delete_backward`，支持 grapheme 测试 | 已补行首退样式、行首 merge previous、空 block 删除 | P0 ✅ |
| 文本删除 | Delete 删除字符 | 删除后一个字符；末尾空 block 可删 | 已有 `delete_forward` | 已补末尾合并 next block、空 block forward 删除 | P1 ✅ |
| 样式取消 | 行首 Backspace 退 Paragraph | V1 主要靠空 Enter；当前产品要求 Backspace 退样式 | 已完成可文本化 block → Paragraph | 需补 undo/persistence 更细事务类型可观测 | P0 ✅ |
| Block 合并 | 非空 block 行首 Backspace | merge into previous，focus previous，selection=appended range | 已实现 | children/subtree 当前先禁止合并，避免静默丢失 | P0 ✅ |
| Block 删除 | 空 block Backspace/Delete | 删除当前 block，focus previous/next；最后一个 block 不删 | 已实现 | children/subtree 当前先禁止删除，避免静默丢失 | P0 ✅ |
| Block split | Enter | split 当前 block，trailing 到新 block | 已完成大部分 | 继续覆盖 code/html/raw 边界测试 | P0 ✅ |
| Cmd+Enter | split new Paragraph | 已完成 | - | P1 ✅ |
| 方向键 | Left/Right block 内 | 已有水平 caret | 已补边界跨 previous/next block | P0 ✅ |
| 方向键 | Shift+Left/Right | 已有同 block selection | 已补跨 block selection 扩展 | P0 ✅ |
| 方向键 | Up/Down block 内垂直 | V1 用 layout cache 找目标 offset；边界跨 block | 已有 Up/Down command + runtime fallback | 完整 layout x/y 垂直目标仍待补 | P0 部分完成 |
| 方向键 | Shift+Up/Down | 跨行/跨 block selection | 已有跨 block fallback selection | 完整 layout x/y 垂直选区仍待补 | P1 部分完成 |
| Focus | Focus previous/next | runtime 按 visible index 聚焦相邻 block | 已新增 adjacent API | - | P0 ✅ |
| Selection | 鼠标跨 block 拖选 | V1 entity 范围；V2 应 runtime selection | 已部分有 mouse drag/document selection | 需完善跨未加载 block selection 与 copy/cut/delete | P0/P1 |
| Copy | 跨 block copy | V1 hydrated entities；V2 应 store/index | V2 selected_document_text 依赖 loaded text_models | 需 payload window miss 时从 store/cache 异步/按需解析 | P1 |
| Cut/Delete selection | 跨 block cut/delete | V1 删除 selected text / full blocks | 已抽 `delete_document_selection`，GUI Cut 已调用 | payload miss/store 异步读取仍待补 | P0 部分完成 |
| Paste | Markdown/native paste | V1 structured markdown paste | V2 structured markdown paste 已完成较多 | Native block clipboard 仍缺 | P1 |
| Indent | Tab structure indent | 已迁移 tree semantics | 已完成 | - | P0 ✅ |
| Outdent | Shift+Tab | 已迁移 tree semantics | 已完成 | - | P0 ✅ |
| Move block | Alt/Cmd+Shift Up/Down | `RequestMoveUp/Down` | gutter drag 已有；键盘未接 | 新增 command + runtime move adjacent | P1 |
| Gutter drag | 拖 block | 已迁移较多 | 已完成大部分 | 持续优化 viewport edge | P1 ✅ |
| Slash menu | `/` + arrows + enter | V1 有完整事件 | 未完整 | 需要 runtime/UI projection slash state | P2 |
| Table cell | cell 编辑/方向键/tab | V1 有专门逻辑 | V2 只有部分模型/hit-test | 需要 table runtime/editor | P2 |
| Code language menu | toolbar language select | V1 有 dropdown/search/copy | V2 当前主要视觉静态 | 需要 UI command + payload kind update + copy | P2 |
| Block 分列 | Columns | V1 未发现稳定实现 | V2 未设计 | 新设计 ColumnsGroup/Column 结构，不从 UI patch | P2/P3 |
| Undo/Redo | text + structure 顺序 | V1 local undo + save tx | V2 已有 text/structure/paste 顺序 | 合并/删除/columns 后需接入轻量 undo | P0 |
| Persist | 结构 tx | V1 RuntimeSaveEvent | V2 pending tx + PG saver | 合并/删除/move up/down/columns 都要入 tx | P0 |

---

## 4. 工程目录设计

现有目录基本可继续使用，新增/调整建议：

```text
src/
  gui/
    input/
      command.rs          # 增加 Up/Down/MoveBlockUp/MoveBlockDown/SlashMenu commands
      keyboard.rs         # 对齐 V1 key map
      mouse.rs            # 跨 block drag selection 已在这里继续完善
  runtime/
    document_runtime.rs   # 当前集中实现；短期先补齐，后续拆分
    block_navigation.rs   # 建议新增：visible adjacent focus、caret block boundary navigation
    block_merge.rs        # 建议新增：merge/delete focused block 和 selection delete
    block_columns.rs      # 建议新增：columns 结构事务（新能力，不从 V1 直接拷）
  core/
    block/
      columns.rs          # ColumnsGroup / Column descriptor、能力声明
    edit/
      mod.rs              # 已有 MergeBlocks/SplitBlock/DeleteBlock/MoveBlockToParent，补事务使用
```

拆分原则：

- P0 可先在 `DocumentRuntime` 内实现，保持最小调用路径；完成后再拆文件。
- 所有跨 block 操作都走 `DocumentIndex / VisibleDocumentIndex`，不从 GUI block rect 推断结构真相。
- 所有影响结构的操作都要产出轻量 undo step 和 pending structure transaction。
- 任何高度变化必须更新 `BlockLayoutMeta / BlockHeightIndex / PageLayoutIndex`，不能只 notify UI。

---

## 5. 任务清单

### A. V1 行为补充分析

- [x] A-001 扫描 V1 `CditorBlockEvent` 事件全集。
- [x] A-002 分析 V1 Backspace/Delete/merge/delete block。
- [x] A-003 分析 V1 Left/Right/Up/Down 和跨 block focus。
- [x] A-004 分析 V1 MoveUp/MoveDown。
- [x] A-005 分析 V1 table cell 特殊键盘逻辑。
- [x] A-006 确认 V1/editor2 未发现稳定 Columns 主链路实现。

### B. P0：Block 合并 / 删除

- [x] B-001 新增 `merge_focused_block_into_previous()` runtime API。
  - [x] 当前 block 必须不是第一个 visible block。
  - [x] 当前 block 文本 append 到 previous block。
  - [x] 当前 block subtree 处理策略明确：若 current 有 children，当前先禁止，避免静默丢。
  - [x] previous block payload 按 kind 合并：RichText 保留 spans；Code/Html 按 plain text 追加。
  - [x] 删除 current block index/payload/text_model。
  - [x] focus previous，caret/selection 落到 append 起点或 appended range。
  - [x] 更新 height/page/scroll，当前 viewport 不跳。
  - [x] 记录 undo：轻量保存 previous before/current record+payload。
  - [x] pending tx 包含 `MergeBlocks`。
- [x] B-002 新增 `delete_focused_empty_block_backward()`。
  - [x] 空 block Backspace 删除当前 block。
  - [x] 最后一个 block 不删除，只保持空 Paragraph。
  - [x] focus previous；若没有 previous，则 focus next。
- [x] B-003 新增 `delete_focused_empty_block_forward()`。
  - [x] 空 block Delete 删除当前 block。
  - [x] focus next；若没有 next，则 focus previous。
- [x] B-004 Delete at end 合并 next block。
  - [x] caret 在当前文本末尾。
  - [x] next block text append 到 current。
  - [x] 删除 next block。
- [x] B-005 测试：非空 paragraph 行首 Backspace 合并 previous。
- [x] B-006 测试：list item 行首先退 Paragraph，再第二次 Backspace 合并。
- [x] B-007 测试：空 block Backspace/Delete 删除并 focus 合理。
- [x] B-008 测试：最后一个 block 不被删空。
- [x] B-009 测试：merge/delete 不改变 `global_scroll_top`。

### C. P0：方向键跨 block

- [x] C-001 `GuiInputCommand` 增加 `MoveCaretUp/Down { extend_selection }`。
- [x] C-002 `keyboard.rs` 映射 Up/Down/Shift+Up/Shift+Down。
- [x] C-003 runtime 新增 `focus_adjacent_visible_block(block_id, direction)`。
  - [x] 使用 `VisibleDocumentIndex`。
  - [x] 跳过隐藏/folded children。
  - [x] 保持 current editing block pin。
- [x] C-004 Left at start：无 selection 时 focus previous block end。
- [x] C-005 Right at end：无 selection 时 focus next block start。
- [x] C-006 Shift+Left at start：扩展 `DocumentSelection` 到 previous block end。
- [x] C-007 Shift+Right at end：扩展 `DocumentSelection` 到 next block start。
- [x] C-008 Up/Down 同 block 第一版：没有 layout target 时退化为 previous/end 或 next/start。
- [x] C-009 Up/Down 完整版：接入 text layout cache 的 caret x/y，按当前 caret x 和上一/下一视觉行 y 找目标 offset；超出当前 block 文本 bounds 时退回跨 block。
- [x] C-010 测试：Left/Right 跨 block focus。
- [x] C-011 测试：Shift+Left/Right 跨 block selection。
- [ ] C-012 测试：Up/Down 在 block 内多行移动，边界跨 block。
  - [x] runtime 目标 offset 移动与 Shift selection 单测。
  - [ ] GUI text layout cache 多行视觉移动集成测试。
- [ ] C-013 测试：跨未加载窗口时不要求 hydrate 全文，必要时使用 projection/window planner。

### D. P0：跨 block selection 删除 / Cut

- [x] D-001 抽出通用 `delete_document_selection()`，不要只为 paste collapse 私有服务。
- [x] D-002 同 block selection 走 text replace。
- [x] D-003 跨 block selection：start prefix + end suffix 合并到 start block。
- [x] D-004 删除中间完整 blocks。
- [x] D-005 处理 selected full blocks：保留一个空 Paragraph 或按 V1 keep first。
- [x] D-006 copy/cut selection 从 runtime 读取，不依赖 UI entity。
- [ ] D-007 undo/redo 单步恢复跨 block 删除。
- [x] D-008 Postgres pending transaction 覆盖 delete range + payload replace。
- [x] D-009 测试：跨 3 blocks 删除。
- [ ] D-010 测试：跨 block cut 写 clipboard 并删除。

### E. P1：键盘 Move Up / Down

- [ ] E-001 `GuiInputCommand` 增加 `MoveBlockUp/MoveBlockDown`。
- [ ] E-002 `keyboard.rs` 映射 V1：`Alt+Up/Down` 与 `Cmd/Ctrl+Shift+Up/Down`。
- [ ] E-003 runtime 新增 `move_focused_block_by_visible_delta(-1/+1)`。
- [ ] E-004 移动 subtree，不只移动单 block。
- [ ] E-005 禁止移动进自己 subtree。
- [ ] E-006 focus 保持 moved block。
- [ ] E-007 复用现有 `move_block_subtree_before/to_parent` undo/persistence。
- [ ] E-008 测试：MoveUp/MoveDown reorder。
- [ ] E-009 测试：带 children subtree 一起移动。

### F. P1/P2：Slash menu

- [ ] F-001 runtime/gui 定义 slash menu transient state，不写文档真相。
- [ ] F-002 `/` query 检测规则对齐 V1。
- [ ] F-003 Up/Down 选择 menu item。
- [ ] F-004 Enter apply selected item：改 block kind / 插入复杂 block。
- [ ] F-005 Escape close。
- [ ] F-006 slash menu 不阻塞 IME。
- [ ] F-007 测试 query、apply、close。

### G. P2：Table 内部编辑

- [ ] G-001 定义 table focused cell runtime state。
- [ ] G-002 Backspace 删除 cell 内文本。
- [ ] G-003 Tab/Shift+Tab 移动 cell focus。
- [ ] G-004 Left/Right 边界跨 cell。
- [ ] G-005 Up/Down 移动 cell row。
- [ ] G-006 Escape 退出 cell focus 回 block。
- [ ] G-007 cell edit 后只更新 table payload，不重建全文 payload。
- [ ] G-008 测试 table keyboard matrix。

### H. P2/P3：Block 分列 / Columns 新设计

- [ ] H-001 明确 ColumnsGroup / Column 数据模型。
- [ ] H-002 `RichBlockKind` 增加 ColumnsGroup / Column，或用 `Custom` descriptor 先落地。
- [ ] H-003 PostgreSQL kind tag / payload / attrs 迁移。
- [ ] H-004 `VisibleDocumentIndex` 支持 columns 展开顺序。
- [ ] H-005 `BlockHeightIndex` 支持 columns group 高度：max(column heights) + chrome。
- [ ] H-006 selection 在 columns 中按视觉顺序还是文档顺序，先定规则。
- [ ] H-007 gutter drag 支持拖入/拖出 column。
- [ ] H-008 copy/paste columns native format。
- [ ] H-009 测试 columns height、selection、drag、persist。

### I. 性能与架构验收

- [x] I-001 所有 P0 操作不得引入 UI entity 真相。
- [x] I-002 所有 P0 操作不得使用 GPUI ListState 管全局滚动。
- [x] I-003 输入/IME hot path 不同步等待 PostgreSQL。
- [x] I-004 merge/delete/move 只操作 affected subtree / affected block payload，不 clone 10w payload。
- [x] I-005 height update 走 `BlockLayoutMeta / BlockHeightIndex / PageLayoutIndex`。
- [x] I-006 当前 editing block pin 不被 merge/delete/focus 切换破坏。
- [ ] I-007 10w demo 下 P0 操作后 projection window 仍约 100~120 blocks。
- [ ] I-008 scrollbar drag 期间不因新高度提交反跳。

---

## 6. 推荐实施顺序

1. **Block 合并/删除 P0**：这是 Backspace/Delete 的根逻辑，优先级最高。
2. **方向键跨 block P0**：补齐 Left/Right 边界、Up/Down command 和 focus adjacent。
3. **跨 block selection delete/cut P0**：依赖方向键和 selection model。
4. **键盘 MoveUp/MoveDown P1**：复用已有 gutter drag 的结构 move。
5. **Slash menu P1/P2**：完善 block 类型创建入口。
6. **Table cell P2**：复杂块内部编辑独立实现。
7. **Columns P2/P3**：V1 没有稳定实现，按 V2 大文档架构新设计。
