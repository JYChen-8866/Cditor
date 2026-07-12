# Notion 级表格交互重构方案

> 目标：不要继续用零散补丁修表格。把表格升级为一个有独立交互模型、布局模型、overlay 模型和事务模型的编辑器子系统，行为对齐 Notion。
>
> 架构约束：继续遵守 `doc/large-document-rich-text-architecture.md`。真实数据、selection、layout height、scroll 状态必须在 runtime；GPUI 只消费 projection、渲染当前窗口、转发事件。

## 1. 当前问题判断

当前表格的问题不是单个滚动条 bug，而是交互边界不成熟：

- 表格横向滚动条由 app UI 根据 `ScrollHandle.bounds/max_offset` 即时决定显示，缺少来自 runtime 的稳定 scroll geometry，所以会出现下方有内容、布局刷新、viewport bounds 未稳定时滚动条消失。
- 表格菜单原来挂在 table block 内部，容易受 table overflow、block 裁剪、局部坐标影响。菜单应该是 editor overlay，不属于 table content tree。
- 行列选择、range selection、active cell、菜单打开、resize、reorder、横向滚动拖拽之间没有统一状态机，多个状态可能互相覆盖。
- GUI 层承担了过多“真实行为判断”，例如什么时候显示 handle、何时显示 scrollbar、菜单锚点如何 clamp，这些应该基于 runtime projection 的稳定几何。
- 表格没有完整的 Notion 操作契约：点击、双击、拖选、handle hover、拖拽行列、菜单、快捷键、复制粘贴、merge/split、undo/redo 应该统一定义。

结论：应重构为 `TableRuntime -> TableLayout -> TableInteractionProjection -> Editor Overlay + Table Content` 的模型，而不是继续在 `render.rs` 里补条件。

## 2. Notion 表格应有的核心体验

### 2.1 基础表格

- 默认插入 3 行 3 列，并占满当前编辑器内容宽度。
- 单元格支持富文本输入、中文 IME、换行、删除、选择、复制粘贴。
- 行高默认 auto，由内容撑开；用户拖拽后变为 px row height。
- 列宽默认按 available width 均分，最小宽度 120px；用户拖拽后保存 px column width。
- 表格内容变高后，后续 block 必须下移，文档滚动高度同步更新。

### 2.2 选择模型

- 点击 cell：进入 active cell 编辑态。
- 拖拽 cell：形成 range selection。
- 点击行 handle：选择整行。
- 点击列 handle：选择整列。
- 点击左上角 table handle：选择整张表。
- Shift + click：扩展 range。
- Esc：从编辑态退出到 cell selection，再退出到普通 block focus。
- selected range、row、column、table selection 互斥；active cell 和 range selection 互斥。

### 2.3 Handle 与菜单

- hover cell 时显示当前 row/column 的轻量 handle。
- 选中 row/column 后 handle 常驻并高亮。
- 点击 handle 打开菜单，菜单属于 editor overlay 层。
- 菜单位置使用 table layout geometry + editor viewport clamp，不受 table scroll viewport 裁剪。
- 菜单支持搜索、键盘上下选择、Enter 执行、Esc 关闭、点击外部关闭。

### 2.4 行列操作

- 插入上/下方行。
- 插入左/右侧列。
- 删除行/列，至少保留 1 行 1 列。
- 复制行/列。
- 清空行/列内容。
- 行列拖拽 reorder，拖动时显示 drop line，松手提交事务。
- 行列 resize，拖动时显示 resize line，松手提交事务。
- 对齐：左、中、右；可作用于 cell/range/row/column/table。

### 2.5 合并与拆分

- range selection 可以合并单元格。
- merged cell 保留左上角 origin cell 内容，其它 covered cell 内容按规则追加或保留到 undo snapshot。
- covered cell 不可被 focus、hit-test、输入。
- split 恢复 covered cells，并保证 selection/focus 落在合法 cell。

### 2.6 横向滚动

- 表格宽度大于 viewport 时，底部显示自定义横向滚动条。
- 滚动条位置在表格外部 chrome 区域，不在 table grid 内部。
- 滚动条只响应拖拽，不接管滚轮事件。
- 普通滚轮始终滚动文档；Shift + wheel 是否横向滚动需要产品明确，默认不做。
- 横向 scroll offset 属于 table interaction state，不应该只存在于 GPUI `ScrollHandle`。
- 表格下方有内容、文档滚动、布局重算、窗口 resize 后，滚动条都必须稳定显示或稳定隐藏，不允许闪烁。

### 2.7 Clipboard 与快捷键

- Copy range/row/column/table 输出 TSV/Markdown，并保留 internal rich clipboard snapshot。
- Paste TSV 到 focused cell 时按矩形区域扩展/覆盖。
- Paste 富文本到 cell 时保留 inline marks。
- Backspace/Delete：
  - 编辑态删除文字；
  - range/row/column selection 清空内容；
  - table selection 删除整表或转为空段落，需产品确认。
- Tab / Shift+Tab 在 cell 间移动，最后一个 cell 可插入新行。
- Enter 在 cell 内换行；Cmd/Ctrl+Enter 结束编辑或跳到下一行，需产品确认。

## 3. 正确架构

### 3.1 Runtime 真相

新增或完善：

```text
TableRuntime
  payload
  selection
  interaction_mode
  horizontal_scroll_offset_px
  layout_revision
```

`horizontal_scroll_offset_px` 需要进入 runtime/app state 的可投影状态，不能只依赖 GPUI `ScrollHandle`。这样重渲染、滚动文档、下方 block 出现时，横向滚动条不会因为 UI handle bounds 未稳定而消失。

### 3.2 Layout 真相

`TableLayout` 输出：

```text
table_width_px
table_height_px
viewport_width_px
horizontal_scroll_max_px
horizontal_scroll_offset_px
row_rects
column_rects
visible_cell_rects
scrollbar_track_rect
scrollbar_thumb_rect
menu_anchor_rect
```

GUI 不自行判断 scrollbar 是否显示，只消费 `horizontal_scroll_max_px > 0` 和 `scrollbar_thumb_rect`。

### 3.3 Interaction 状态机

统一定义：

```rust
enum TableInteractionMode {
    Idle,
    EditingCell(TableCellPosition),
    SelectingRange { anchor: TableCellPosition, head: TableCellPosition },
    AxisSelected(TableAxisSelection),
    TableSelected(BlockId),
    Resizing(TableResizeDrag),
    Reordering(TableReorderDrag),
    HScrolling(TableHScrollDrag),
    MenuOpen(TableMenuState),
}
```

所有鼠标、键盘、菜单事件先进入状态机，由状态机决定：

- 是否改变 selection；
- 是否改变 focus；
- 是否打开/关闭 menu；
- 是否进入 drag；
- 是否提交 transaction；
- 是否通知文档滚动。

### 3.4 Editor Overlay 分层

表格渲染拆成两类：

```text
Table content layer
  grid
  cells
  active cell border
  selected cells background

Editor overlay layer
  row/column handles
  table menu
  resize line
  reorder drop line
  horizontal scrollbar chrome
  IME candidate rect bridge
```

原则：会浮出表格、需要超过 table viewport、需要跨 block clamp 的东西，都属于 editor overlay。

## 4. 推荐工程拆分

### 4.1 Engine

```text
crates/runtime/src/document_runtime/table/
  runtime.rs
  layout.rs
  selection.rs
  interaction.rs
  scroll.rs
  edit.rs
  clipboard.rs
  projection.rs
```

### 4.2 App

```text
crates/app/src/gui/block/table/
  render.rs          # 只画 grid/cells
  cell.rs
  text.rs
  style.rs

crates/app/src/gui/overlay/table/
  mod.rs
  handles.rs
  menu.rs
  scrollbar.rs
  resize.rs
  reorder.rs
```

表格菜单、横向滚动条、resize line、drop line 最终都应从 `gui/overlay/table` 渲染，而不是 table block 内部 child。

## 5. 滚动条消失的修复方向

不要继续依赖 `ScrollHandle.bounds()` 作为是否显示 scrollbar 的唯一条件。成熟方案：

1. runtime projection 给出 `table_width_px`。
2. editor layout/projection 给出 table viewport width，或 app 在 layout pass 后把 viewport width 作为 table viewport measurement 回写到 app state。
3. 计算：

```text
scroll_max = max(0, table_width_px + table_gutter_px - viewport_width_px)
thumb_width = max(32, track_width * viewport_width_px / content_width_px)
thumb_left = scroll_offset / scroll_max * (track_width - thumb_width)
```

4. 只要 `scroll_max > 0` 就显示 scrollbar。
5. 当 viewport width 暂时未知时，保留上一次稳定 measurement，不能直接隐藏。
6. window resize / table resize / column resize / document width change 后重新测量并 clamp offset。

## 6. 实施顺序

### Phase 1：止血但不乱补

- 把 table menu、scrollbar、resize line、drop line 全部迁到 editor overlay。
- 移除 table 内部 wheel handler。
- 为每个 table block 建立稳定 `TableViewportMeasurement`。
- 横向 scrollbar 根据 projection + measurement 显示，不再根据瞬时 `ScrollHandle.bounds` 决定。
- 增加回归测试：表格下方有内容时 scrollbar 不消失。

### Phase 2：状态机

- 新增 `TableInteractionMode`。
- 把 axis selection、range selection、resize、reorder、menu open、hscroll drag 全部归一。
- 定义状态转换表，并补单测。
- 清理 GUI 中散落的 `selected_table_axis`、`table_range_selection_drag`、`table_hscroll_drag` 互相抢状态的问题。

### Phase 3：Notion 操作补齐

- 完整行列菜单。
- range 合并/拆分。
- table handle。
- keyboard navigation。
- clipboard TSV/Markdown/internal rich snapshot。
- undo/redo 覆盖所有 table transaction。

### Phase 4：质量门禁

- 单测覆盖 runtime edit/layout/selection/scroll。
- app 层测试覆盖 menu/scrollbar geometry helper。
- 手动验收覆盖中文 IME、长表格、宽表格、下方 block、窗口 resize、文档滚动、undo/redo。

## 7. 验收清单

- 默认插入 3x3，宽度填满编辑器内容区域。
- 3 列时无横向滚动条；列多或列宽大于 viewport 时稳定显示横向滚动条。
- 表格下方有任意 block 时，横向滚动条仍显示在表格底部 chrome 区，不被下方内容覆盖或挤没。
- 普通滚轮只滚动文档，不改变表格横向 offset。
- 拖动横向滚动条只改变表格横向 offset，不影响文档滚动。
- 菜单永远在 editor overlay，不被表格 viewport 裁剪。
- 菜单打开时，文档滚动后菜单跟随 projection 或自动关闭，二选一并保持一致。
- 行列 resize/reorder 预览不污染 payload，松手才提交。
- cell 输入导致行高变化时，下面 block 下移，文档滚动条总高度同步更新。
- range/row/column/table selection 视觉不冲突。
- IME candidate rect 跟随 cell caret。
- undo/redo 能恢复表格内容、行列尺寸、merge/split、selection/focus。
