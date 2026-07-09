# Notion 级表格实现方案与任务清单

> 目标：把 Cditor V2 的表格从“能渲染的特殊 block”升级为接近 Notion 的完整表格编辑子系统。
>
> 更新规则：后续每完成一项，就把本文对应任务从 `[ ]` 改成 `[x]`，并在任务下方补测试、验收或风险说明。
>
> 架构约束：继续遵守 `doc/large-document-rich-text-architecture.md`。真实文档、表格数据、selection、layout height、scroll 坐标必须属于 runtime/engine；GPUI 只负责当前窗口投影和交互事件。

---

## 1. 当前问题

当前表格的主要问题不是单点 bug，而是系统边界还不清楚：

- 表格高度、文档 block 高度、scroll height 没有始终同步，导致表格下面的内容和表格重叠。
- 表格 cell 文本、IME、selection、caret、candidate rect 仍然依赖多处状态协作，容易出现错位。
- 表格行列 handle、menu、resize、reorder、merge/split 还没有完整状态机。
- GUI 里有表格渲染和部分交互，但 engine 里缺少统一 TableLayout 和表格操作事务。
- 表格样式还不够产品化：selection 背景、active cell、hover handle、菜单、resize indicator、row/column selected 状态都需要统一设计。

最终实现不能靠在 GUI 里继续补判断。表格应该成为一个独立子系统：

```text
TablePayload
  -> TableRuntime / TableLayout
  -> TableViewState projection
  -> GPUI table render
  -> input/drag/menu events
  -> table transaction
```

---

## 2. 产品目标

### 2.1 Notion 级基础体验

- 单元格内可以自然输入、IME、换行、删除、复制粘贴。
- 表格内容增长时，Auto 行高撑开，下面 block 自动下移。
- 点击 cell 进入编辑，点击行/列 handle 进入行列选择。
- active cell、selected range、row selection、column selection、table selection 视觉清晰且不冲突。
- 行列 resize 有实时预览，松手提交。
- 行列 reorder 有拖拽指示线，松手交换顺序。
- 合并/拆分单元格稳定，不丢内容、不破坏 focus。
- undo/redo 能恢复表格数据、样式、selection、layout height。

### 2.2 大文档约束

- 不能让 GPUI entity 成为表格数据真相。
- 当前 viewport 只渲染当前窗口内 block，但表格 block 的高度必须进入全局 `height_index`。
- cell layout cache 只作为候选框、hit-test、caret 绘制辅助，不作为表格真实状态。
- 远端/未加载 payload 可以使用 placeholder，但已加载 table block 必须保持 kind/payload/runtime invariant。

---

## 3. 目标工程目录

### 3.1 Engine

```text
crates/engine/src/document_runtime/table/
  mod.rs
  runtime.rs       # TableRuntime、payload/runtime 同步、invariant
  layout.rs        # TableLayout、row/col/cell rect、Auto/Px 尺寸
  selection.rs     # cell/range/row/column/table selection model
  input.rs         # focused cell、IME、text replacement、caret
  edit.rs          # insert/delete/move/resize/merge/split/align
  transaction.rs   # table undo/redo transaction helpers
  projection.rs    # TableViewState/TableVisibleCell 生成
  clipboard.rs     # table internal/external clipboard
  tests.rs
```

### 3.2 App / GUI

```text
crates/app/src/gui/block/table/
  mod.rs
  render.rs
  cell.rs
  text.rs
  handles.rs       # row/column hover handle
  menu.rs          # floating menu
  resize.rs        # row/column resize overlay + drag state
  reorder.rs       # row/column drag reorder overlay
  selection.rs
  style.rs
  tests.rs
```

### 3.3 原则

- 单个文件超过 700 行时必须拆分。
- runtime 负责状态和事务，GUI 负责显示和事件转发。
- 表格 layout 只在 engine 里算，GUI 不临时推导真实高度。
- 所有表格操作都必须有 runtime 单测；复杂交互再补 app 层单测。

---

## 4. 核心数据模型

### 4.1 表格运行时

```rust
pub struct TableRuntime {
    pub block_id: BlockId,
    pub table: TablePayload,
    pub revision: u64,
    pub selection: TableSelection,
    pub focused_cell: Option<TableCellPosition>,
}
```

### 4.2 表格布局

```rust
pub struct TableLayout {
    pub block_id: BlockId,
    pub content_version: u64,
    pub width_px: f32,
    pub height_px: f32,
    pub column_widths: Vec<f32>,
    pub row_heights: Vec<f32>,
    pub visible_cells: Vec<TableCellRect>,
}
```

### 4.3 表格选择

```rust
pub enum TableSelection {
    None,
    Cell(TableCellPosition),
    Range(TableRange),
    Row { block_id: BlockId, row: usize },
    Column { block_id: BlockId, col: usize },
    Table { block_id: BlockId },
}
```

### 4.4 表格编辑模式

```rust
pub enum TableInteractionMode {
    Idle,
    HoverCell(TableCellPosition),
    EditingCell(TableCellPosition),
    SelectingRange(TableRange),
    Resizing(TableResizeDrag),
    Reordering(TableReorderDrag),
    MenuOpen(TableSelection),
}
```

---

## 5. 必须保持的不变量

- `RichBlockKind::Table` 必须配套 `BlockPayload::Table`。
- table payload 为空时，只能在 normalize 层修复为默认 2x2，不能在 GUI 渲染时偷偷修。
- covered cell 不允许成为 input target。
- focused covered cell 必须映射到 origin cell。
- `InputTarget::TableCell` 必须和 `TableRuntime.focused_cell` 一致。
- 表格 cell 文本变化后，必须同步：
  - table payload/runtime；
  - content_version；
  - table layout height；
  - block layout height；
  - `height_index`；
  - `page_layout`；
  - scroll model total height。
- GUI 中 cell rect 必须来自 `TableLayout` / `TableViewState`。
- 表格 resize/reorder 预览态不能直接污染 payload，MouseUp 才提交事务。

---

## 6. 详细任务清单

### A. 文档与基线

- [x] A-001 阅读并确认 `doc/large-document-rich-text-architecture.md` 中 UI projection、height index、selection、IME 约束。
  - 确认约束：UI 只渲染当前窗口投影；真实 table payload、selection/focus、height index、scroll anchor、IME composition 状态必须在 runtime/engine；candidate rect 和 hit-test 使用当前 visual geometry cache，但不能成为数据真相。
- [x] A-002 对照旧版 `/Users/jychen/Desktop/Cditor/src` 表格实现，列出需要迁移的行为清单。
  - 旧版表格核心文件：`editor2/block/table.rs`、`editor2/component/table/mod.rs`、`editor2/text/element.rs`、`editor2/block/entity.rs`。
  - 需要迁移的行为：slash table 默认 2x2；cell click 请求焦点；focused cell 独立保存 selected range、selection direction、marked range；cell text replacement 按字符边界 clamp；Backspace/Delete 只删除 cell 内容；cell layout cache 提供 candidate rect；cell 内容变更同步 block payload 和 dirty/save queue；header row 和 active cell 有明确视觉状态。
  - 不能 1:1 照搬的部分：旧版把 `TableRuntime`、cell entity 和 layout cache 放在 GUI/block entity 中；V2 必须把 payload、focus/selection、layout height、transactions 放到 engine/runtime，GUI 只消费 `TableViewState` 和转发事件。
- [x] A-003 梳理当前 V2 表格文件和大文件风险，确认拆分目标。
  - 当前目录已拆成 `crates/engine/src/document_runtime/table/{runtime,layout,selection,input,edit,transaction,projection,clipboard,navigation,resize,reorder}.rs` 和 `crates/app/src/gui/block/table/{render,cell,text,selection,style,toolbar}.rs`。
  - 本轮继续拆分 `crates/app/src/gui/text/element.rs`：将平台布局/命中测试几何迁移到 `text/platform.rs`，将测试迁移到 `text/element_tests.rs`；`element.rs` 降至 700 行以内。
- [x] A-004 建立本计划文档，并把后续任务按编号跟踪。
- [x] A-005 增加手动验收清单：编辑、选择、resize、reorder、merge、undo、clipboard。
  - 手动验收入口见本文 R 组：覆盖 2x2 默认插入、多行 cell 高度、中文 IME、row/column handle、resize、reorder、merge/split、range selection、active cell/caret、滚动后 candidate rect 和下方 block 不重叠。

### B. Engine 目录拆分

- [x] B-001 把 `crates/engine/src/document_runtime/table.rs` 拆成 `table/mod.rs`。
- [x] B-002 新增 `table/runtime.rs`，承载 `TableRuntime` 和 payload/runtime 同步。
- [x] B-003 新增 `table/layout.rs`，承载 `TableLayout` 和几何计算。
- [x] B-004 新增 `table/selection.rs`，承载 `TableSelection`。
- [x] B-005 新增 `table/input.rs`，承载 focused cell、cell text replacement、IME fallback。
- [x] B-006 新增 `table/edit.rs`，承载 insert/delete/move/resize/merge/split/align。
- [x] B-007 新增 `table/projection.rs`，承载 `TableViewState` 生成。
- [x] B-008 新增 `table/transaction.rs`，承载 table undo/redo transaction helpers。
- [x] B-009 新增 `table/clipboard.rs`，承载 table clipboard 转换。
- [x] B-010 确保拆分后每个文件职责单一，单文件不超过 700 行。
- [x] B-011 拆分后运行 `cargo test -p cditor-runtime --lib`。
- [x] B-012 新增 `table/navigation.rs`，承载 focused cell 方向键和 Tab 导航。
- [x] B-013 增加 table row height / column width resize 的 core、runtime、projection 提交链路。

### C. Table Runtime Invariant

- [x] C-001 定义 `ensure_table_payload_for_kind(kind, payload)`，保证 kind/payload 成对。
- [x] C-002 `RichBlockKind::Table + 非 TablePayload` 自动转换为默认 2x2 table。
- [x] C-003 空 `TablePayload` 自动修复为默认 2x2。
- [x] C-004 从普通 block 转 table 时，原文本进入第一个 cell。
- [x] C-005 从 table 转普通 block 时，按 plain text/markdown 规则导出 cell 文本。
- [x] C-006 payload window 加载 table block 时同步 `TableRuntime`。
- [x] C-007 undo/redo 恢复 table block 时同步 `TableRuntime`。
- [x] C-008 composition preview 不允许把 table payload 投影成普通 rich text。
- [x] C-009 增加 kind/payload invariant 单测。
- [x] C-010 增加 payload loading 后 table runtime 不丢失单测。

### D. TableLayout 引擎

- [x] D-001 定义 `TableLayoutInput`，包含 table、available_width、theme-independent metrics。
- [x] D-002 定义 `TableLayout`，输出 row heights、column widths、visible cell rects、table height。
- [x] D-003 Auto column width：默认 120px，最小宽度 120px。
- [x] D-004 Px column width：尊重用户拖拽宽度。
- [x] D-005 Auto row height：由 cell 内容高度撑开。
- [x] D-006 Px row height：尊重用户拖拽高度，不被内容自动覆盖。
- [x] D-007 merged cell 的 width/height 使用 span 后的总尺寸。
- [x] D-008 covered cell 不生成 visible rect。
- [x] D-009 layout 输出稳定 y/x offsets，供 hit-test 和 toolbar 定位使用。
- [x] D-010 表格总高度 `table_height = sum(row_heights)`。
- [x] D-011 增加多行 cell 撑开 Auto row 单测。
- [x] D-012 增加表格在文档中间时后续 block 下移单测。
- [x] D-013 增加 Px row 不被内容撑开单测。
- [x] D-014 增加 merged cell span geometry 单测。
- [x] D-015 增加宽度变化导致文本换行后 row height 更新单测。

### E. Block Height 同步

- [x] E-001 实现 `refresh_table_block_height(block_id)`。
- [x] E-002 cell 文本变化后调用 `refresh_table_block_height`。
- [x] E-003 row resize commit 后调用 `refresh_table_block_height`。
- [x] E-004 column resize commit 后调用 `refresh_table_block_height`，因为换行可能改变 row height。
- [x] E-005 merge/split 后调用 `refresh_table_block_height`。
- [x] E-006 insert/delete row 后调用 `refresh_table_block_height`。
- [x] E-007 更新 `layout_meta.estimated_height/measured_height/dirty/layout_version`。
- [x] E-008 更新 `height_index`。
- [x] E-009 更新 `page_layout`。
- [x] E-010 更新 `scroll.model_total_height` 和 `displayed_total_height`。
- [x] E-011 高度变化时保持当前 viewport anchor 稳定。
  - 新增 `table_height_change_above_viewport_restores_viewport_anchor`：表格在 viewport 上方因 cell 回车变高时，`global_scroll_top` 按高度 delta 修正，当前视口锚点不漂。
- [x] E-012 增加“表格下面有内容，cell 回车后下面 block 下移”单测。
- [x] E-013 增加“表格高度变化不造成滚动条反跳”单测。

### F. Cell 输入与 IME

- [x] F-001 `InputTarget::TableCell` 成为 cell 输入唯一入口。
- [x] F-002 focused cell 保存 `selected_range`、`selection_reversed`、`marked_range`。
- [x] F-003 Enter 在 cell 内插入 `\n`。
- [x] F-004 Shift+Enter 行为与 Enter 一致，保留为 soft line break。
- [x] F-005 Tab 移动到下一个 cell。
- [x] F-006 Shift+Tab 移动到上一个 cell。
- [x] F-007 Arrow 在 cell 内移动；到边界后跨 cell。
- [x] F-008 Backspace/Delete 删除 cell 内字符，不触发 block 删除。
- [x] F-009 IME preview 保持在当前 cell。
- [x] F-010 IME commit 后 caret 留在 cell 内。
- [x] F-011 marked range 绘制不和 active cell 边框冲突。
  - active cell 改为独立 overlay border，marked range 背景仍限制在 text layout segment 内；自定义 caret 在 marked range 存在时隐藏。
- [x] F-012 candidate rect 使用 cell layout cache 的真实 caret rect。
- [x] F-013 covered cell focus 自动转 origin cell。
- [x] F-014 增加中文/日文/韩文/emoji 输入测试。
- [x] F-015 增加 cell 多行输入后候选框位置测试。

### G. Selection 模型

- [x] G-001 定义 cell selection。
- [x] G-002 定义 range selection。
- [x] G-003 定义 row selection。
- [x] G-004 定义 column selection。
- [x] G-005 定义 whole table selection。
- [x] G-006 明确 editing text selection 与 table selection 的切换规则。
  - cell click 进入 `InputTarget::TableCell` 并清 table axis selection；row/column handle click 清 text drag selection 并进入 table axis selection；Escape/blur 清 focused cell 并同步 editing input target 回 table block。
- [x] G-007 点击 cell：进入 editing cell。
  - `render_table_cell` 的 mouse down 进入 `focus_table_cell_from_gui`，清除 table axis selection 并聚焦对应 `InputTarget::TableCell`。
- [x] G-008 拖拽 cell：进入 range selection。
  - GUI 新增 `TableCellRangeSelection` 和 cell drag anchor/focus 状态；单击仍编辑 cell，拖到其它 cell 才显示矩形 range selection。
- [x] G-009 点击 row handle：选中整行。
  - row handle 调用 `select_table_axis_from_gui(TableAxis::Row, index)`，并通过 `table_row_selection_range` 映射为整行 `TableRange`。
- [x] G-010 点击 column handle：选中整列。
  - column handle 调用 `select_table_axis_from_gui(TableAxis::Column, index)`，并通过 `table_column_selection_range` 映射为整列 `TableRange`。
- [x] G-011 Escape：从 editing cell 回到 cell/table selection。
  - `blur_table_cell` 现在同时清 focused cell 和旧 `InputTarget::TableCell`，新增 `blur_table_cell_exits_cell_editing_without_writing_old_cell`。
- [x] G-012 选中整行/列时整块背景连续，不出现断裂白条。
  - selected cell 内部边线使用 selection background，避免整行/整列 selected cells 之间出现分割白条。
- [x] G-013 selection 投影不能依赖当前 GUI entity。
  - cell focus、axis selection range 和 table selection range 均由 runtime/table payload 计算；`table_cell_focus_is_projected_without_ui_entity_state` 覆盖 focused cell projection。
- [x] G-014 增加 range selection 单测。
- [x] G-015 增加 row/column selection 单测。

### H. GUI 样式

- [x] H-001 表格外边框、圆角、背景统一从 theme 取色。
- [x] H-002 cell border 使用主题 border，不出现过重线条。
- [x] H-003 header row/column 背景统一从 theme 取色。
- [x] H-004 active cell 使用明确蓝色边框。
  - 使用 `theme.table_active_border` 绘制 2px overlay border，不再只依赖普通 cell 右边框。
- [x] H-005 editing cell caret 不和 active border 冲突。
  - caret 仍由 text layout bounds 绘制在 cell padding 内，active border 作为不参与布局的 overlay 绘制在 cell 边界。
- [x] H-006 selected range 背景连续。
  - selected cell 的内部边线颜色改为 selection background，避免相邻 selected cells 之间出现白色/灰色缝隙。
- [x] H-007 row/column selection 背景覆盖整行/整列。
  - row/column selection 仍由 projection 范围决定，视觉上每个命中的 cell 使用同一 selection background 与 border color。
- [x] H-008 hover row/column handle 只在 hover 时出现。
  - handle 默认 opacity 0，依赖 `group_hover("table-cell-axis")` 显示；selected 状态下常驻。
- [x] H-009 selected handle 转为 gutter-like control。
  - selected handle 使用 gutter/action 主题色和 2x2 dot icon，和 block gutter 视觉靠齐。
- [x] H-010 resize indicator 使用主题 accent。
  - `crates/app/src/gui/block/table/resize.rs` 在表格最外层 overlay 绘制实时 resize indicator，颜色取 `theme.action_accent`，避免被 cell 内容或 block 背景遮挡。
- [x] H-011 reorder indicator 与 block drag indicator 风格一致。
  - `crates/app/src/gui/block/table/reorder.rs` 在表格外层绘制主题 accent drop indicator，和现有 drag/resize indicator 保持同一视觉语言。
- [x] H-012 empty cell placeholder 只在未编辑且为空时显示。
  - 新增 `table_cell_placeholder_is_hidden_while_editing_empty_cell`，确保 active empty cell 不显示 placeholder。
- [x] H-013 多行 cell 文本垂直位置稳定，不缩放、不偏移。
  - active/focus 视觉不再改变 padding、font-size、line-height；`table_cell_line_height_is_stable_for_empty_active_cells` 和 active overlay 样式测试覆盖该约束。
- [ ] H-014 表格视觉在 light theme 下接近 Notion。
- [x] H-015 为 dark theme 保留颜色变量，不硬编码浅色。

### I. Row / Column Menu

- [x] I-001 新增 `table/menu.rs`。
  - 新增菜单动作模型、row/column 轴向菜单项、搜索过滤、panel 高度和定位计算，并补 app 单测。
- [x] I-002 row handle click 打开 row menu。
  - row handle 选中后渲染纵向 table menu，row-specific action 映射到 runtime row edit。
- [x] I-003 column handle click 打开 column menu。
  - column handle 选中后渲染纵向 table menu，column-specific action 映射到 runtime column edit。
- [x] I-004 menu 定位贴近 handle，做窗口边距检测。
  - `table_menu_position` 支持 x clamp 和上下翻转，避免贴近窗口边缘时溢出。
- [x] I-005 menu 支持滚动，不溢出窗口。
  - `table_menu_panel_height` 限制最大可见行数，为后续滚动渲染提供固定高度。
- [x] I-006 menu 支持搜索/过滤操作。
  - `filter_table_menu_items` 支持 label 和 keywords 匹配。
- [x] I-007 row menu 支持上方插入、下方插入。
  - `InsertRowAbove/Below` 通过 `insert_table_row(block_id, index/index+1)` 提交。
- [x] I-008 row menu 支持删除行。
  - `DeleteRow` 通过 `delete_table_row(block_id, index)` 提交。
- [x] I-009 row menu 支持复制行。
  - 新增 `TablePayload::duplicate_row`、runtime `duplicate_table_row` 和 GUI menu action；复制内容、行高并更新 projection。
- [x] I-010 column menu 支持左侧插入、右侧插入。
  - `InsertColumnLeft/Right` 通过 `insert_table_column(block_id, index/index+1)` 提交。
- [x] I-011 column menu 支持删除列。
  - `DeleteColumn` 通过 `delete_table_column(block_id, index)` 提交。
- [x] I-012 column menu 支持复制列。
  - 新增 `TablePayload::duplicate_column`、runtime `duplicate_table_column` 和 GUI menu action；复制内容、列宽并更新 projection。
- [x] I-013 menu 支持对齐：左/中/右。
  - `Align(left/center/right)` 复用 `set_table_cell_align`。
- [x] I-014 menu 支持合并/拆分。
  - `MergeCells/SplitCell` 复用 runtime merge/split 事务链路。
- [x] I-015 menu 支持背景色入口。
  - 新增 `set_cell_background_color` core/runtime 链路，projection 透出 `background_color`，GUI 映射 `action_background` 主题 token 并从 menu action 提交。

### J. Row / Column Resize

- [x] J-001 新增 `table/resize.rs`。
- [x] J-002 hover column edge 显示 resize cursor/handle。
  - 表格级 overlay 为每列边缘提供宽命中区和细视觉线，使用 GPUI `cursor_col_resize()`。
- [x] J-003 column resize drag 时显示实时竖线。
  - app resize preview 已穿透到 document/block/table 渲染链路，拖拽时按列起点 + preview width 绘制实时竖线。
- [x] J-004 column resize preview 不提交 payload。
  - 新增 `GuiTableResizeDrag` preview 状态；drag move 只更新 `current_size_px`，mouseup commit 才调用 `set_table_column_width`。
- [x] J-005 MouseUp 提交 `TableResizeColumn` transaction。
- [x] J-006 column resize 后 candidate rect、cell rect 更新。
- [x] J-007 hover row edge 显示 resize cursor/handle。
  - 表格级 overlay 为每行边缘提供宽命中区和细视觉线，使用 GPUI `cursor_row_resize()`。
- [x] J-008 row resize drag 时显示实时横线。
  - 同一 preview 链路按行起点 + preview height 绘制实时横线。
- [x] J-009 row resize preview 不提交 payload。
  - 同一 resize 状态机支持 row axis；drag move 只更新 preview size，mouseup commit 才调用 `set_table_row_height`。
- [x] J-010 MouseUp 提交 `TableResizeRow` transaction。
- [x] J-011 row resize 后 block height 同步。
- [x] J-012 resize 最小值限制，避免负宽/负高。
- [x] J-013 resize 支持撤销/重做。
- [x] J-014 增加 column resize geometry 测试。
- [x] J-015 增加 row resize height sync 测试。

### K. Row / Column Reorder

- [x] K-001 新增 `table/reorder.rs`。
- [x] K-002 row handle drag 启动 row reorder。
  - axis handle mouse down 同时建立 `GuiTableReorderDrag`，普通点击仍保留 row selection。
- [x] K-003 column handle drag 启动 column reorder。
  - column handle 复用同一 drag state，按 column track sizes 推导 drop target。
- [x] K-004 drag 时显示 drop indicator。
  - app preview 状态穿透到 document/block/table 渲染链路，drag move 只更新 target index 并重绘 drop indicator。
- [x] K-005 row reorder 交换/移动 rows。
- [x] K-006 column reorder 交换/移动 cells 和 columns。
- [x] K-007 reorder 后 focus/selection 跟随原 row/column。
- [x] K-008 reorder 后 merge metadata 正确重映射。
  - core move row/column 支持不拆散 merged rectangle 的重排，并重映射 covered cell 的 origin 坐标；会拆散或让 origin 离开左上角的移动会返回错误。
- [x] K-009 reorder 后 row/column sizes 跟随移动。
- [x] K-010 reorder 支持 undo/redo。
- [x] K-011 reorder 不触发 block drag。
  - table axis handle 独立处理 mouse down 并 `stop_propagation`，拖动期间由 `table_reorder_drag` 优先消费 mouse move/up。
- [x] K-012 表格 block gutter drag 与 row/column drag 不冲突。
  - `start_table_reorder_from_gui` 会清理 gutter/block drag、resize drag、text drag selection，避免多种拖拽状态叠加。
- [x] K-013 增加 row reorder payload 测试。
- [x] K-014 增加 column reorder payload 测试。
- [x] K-015 增加 merged cells reorder 测试。
  - `cditor-core` 覆盖 row/column reorder 后 merge metadata remap 与 split rejection；`cditor-runtime` 覆盖 payload commit 层行为。

### L. Merge / Split

- [x] L-001 range selection 后 menu 显示 merge。
  - `render_table_range_toolbar` 使用 range 专属菜单，包含 align/merge/split/background；merge 操作复用 runtime `table_range_selection_range` 做边界校验。
- [x] L-002 合并时 origin cell 保留内容。
- [x] L-003 合并时其他 cell 内容合并到 origin，规则明确为按行拼接。
- [x] L-004 covered cell 不生成 visible rect。
- [x] L-005 covered cell 不接收 input。
- [x] L-006 点击 covered cell 聚焦 origin cell。
- [x] L-007 split 后恢复 covered cells。
- [x] L-008 split 后内容只保留在 origin cell，其余为空。
- [x] L-009 merge/split 后 block height 同步。
- [x] L-010 merge/split 支持 undo/redo。
- [x] L-011 merge 后 row/column resize 正确。
- [x] L-012 merge 后 row/column reorder 正确。
  - merge 后移动未合并行/列到合并块前方会整体平移合并块，runtime projection/payload 继续保持合法 origin/covered metadata。
- [x] L-013 增加 merge payload 测试。
- [x] L-014 增加 split payload 测试。
- [x] L-015 增加 covered cell hit-test 测试。

### M. Alignment / Style

- [x] M-001 cell 支持左对齐。
- [x] M-002 cell 支持居中。
- [x] M-003 cell 支持右对齐。
- [x] M-004 range selection 批量设置对齐。
- [x] M-005 row selection 批量设置对齐。
- [x] M-006 column selection 批量设置对齐。
- [x] M-007 对齐信息进入 payload。
- [x] M-008 对齐支持 undo/redo。
- [x] M-009 对齐后 caret/candidate rect 不偏移。
- [x] M-010 预留 cell background color payload。
- [x] M-011 预留 header style payload。
- [x] M-012 增加 align payload 测试。
- [x] M-013 增加 align render state 测试。

### N. Clipboard

- [x] N-001 复制单 cell 到内部 clipboard。
- [x] N-002 复制 range 到内部 clipboard。
- [x] N-003 复制 row/column 到内部 clipboard。
- [x] N-004 复制 whole table 到内部 clipboard。
- [x] N-005 粘贴给 Cditor 自己保留表格结构、merge、align、sizes。
- [x] N-006 粘贴到外部输出 markdown table/plain text。
- [x] N-007 外部 markdown table 粘贴进 Cditor 转为 table。
- [x] N-008 外部 TSV/CSV 粘贴进表格 range。
- [x] N-009 粘贴区域超过当前表格时自动扩展行列。
- [x] N-010 clipboard 支持 undo/redo。
- [x] N-011 增加 internal table clipboard 测试。
- [x] N-012 增加 markdown/plain text export 测试。
- [x] N-013 增加 TSV paste 测试。

### O. Undo / Redo 事务

- [x] O-001 定义 `TableSetCellText` transaction。
- [x] O-002 定义 `TableInsertRows` transaction。
- [x] O-003 定义 `TableDeleteRows` transaction。
- [x] O-004 定义 `TableInsertColumns` transaction。
- [x] O-005 定义 `TableDeleteColumns` transaction。
- [x] O-006 定义 `TableResizeRow` transaction。
- [x] O-007 定义 `TableResizeColumn` transaction。
- [x] O-008 定义 `TableMoveRows` transaction。
- [x] O-009 定义 `TableMoveColumns` transaction。
- [x] O-010 定义 `TableMergeCells` transaction。
- [x] O-011 定义 `TableSplitCell` transaction。
- [x] O-012 定义 `TableSetCellAlign` transaction。
  - core 新增 `TableEditOperation` 并接入 `EditOperation::Table` affected block 计算；store-postgres 新增 `DbTableEditOperation`，覆盖 table transaction JSON encode/decode round-trip。
- [x] O-013 undo 后恢复 payload。
  - runtime snapshot undo 已覆盖 table resize、merge/split、align 的 payload 恢复；相关测试包括 `table_resize_supports_undo_and_redo`、`merge_and_split_table_cells_support_undo_and_redo`、`table_cell_align_supports_undo_and_redo`。
- [x] O-014 undo 后恢复 focus/selection。
  - `TextSnapshot` 保存 focused table cell；restore 后恢复 table input target、selected range、marked range 和方向，`table_cell_align_supports_undo_and_redo` 覆盖 undo/redo 后 cell selection。
- [x] O-015 undo 后恢复 block height。
  - snapshot restore 后刷新 table block height；`table_resize_supports_undo_and_redo` 断言 undo 后 `table_view.height_px` 回到正确值。
- [x] O-016 redo 后恢复 payload/focus/height。
  - redo 使用同一 snapshot restore 路径，覆盖 table payload、focused cell selection 和 table height 的恢复。

### P. Persistence

- [x] P-001 Postgres payload schema 覆盖 rows/cells/spans。
- [x] P-002 Postgres payload schema 覆盖 columns width。
- [x] P-003 Postgres payload schema 覆盖 row height。
- [x] P-004 Postgres payload schema 覆盖 merge。
- [x] P-005 Postgres payload schema 覆盖 align。
- [x] P-006 Postgres payload schema 预留 cell style。
- [ ] P-007 保存后重新打开，表格结构一致。
- [ ] P-008 保存后重新打开，row/column sizes 一致。
- [ ] P-009 保存后重新打开，merge/align 一致。
- [ ] P-010 保存后重新打开，layout cache 不导致表格高度错误。

### Q. Performance

- [ ] Q-001 10w block 文档中表格 projection 不全量 layout 全文。
- [ ] Q-002 50k row table 使用虚拟化/分片策略，不一次渲染全部 cell。
- [ ] Q-003 当前 viewport 表格 cell layout 控制在 frame budget 内。
- [ ] Q-004 typing cell 时只更新当前 table/block 高度，不重算全局所有 block。
- [x] Q-005 resize drag 时 preview 轻量，不每帧提交事务。
  - drag move 只更新 `GuiTableResizeDrag.current_size_px` 并重绘 overlay，MouseUp 才调用 runtime resize commit。
- [x] Q-006 reorder drag 时只算当前表格 drop target。
  - `GuiTableReorderDrag` 保存当前表格 row/column track sizes，drag move 只用本表格局部 sizes 推导 target index，不扫描文档或其它表格。
- [ ] Q-007 merge/split 大 range 有性能预算。
- [ ] Q-008 增加 table typing latency acceptance。
- [ ] Q-009 增加 table resize drag frame budget acceptance。
- [ ] Q-010 增加 large table projection acceptance。

### R. GUI 验收

- [ ] R-001 插入 2x2 表格默认样式接近 Notion。
- [ ] R-002 cell 内输入多行，表格高度增长，下面 block 下移。
- [ ] R-003 cell 内中文 IME preview/commit 稳定。
- [ ] R-004 row handle hover/selected/menu 正常。
- [ ] R-005 column handle hover/selected/menu 正常。
- [ ] R-006 resize column 视觉和结果正确。
- [ ] R-007 resize row 视觉和结果正确。
- [ ] R-008 reorder row 视觉和结果正确。
- [ ] R-009 reorder column 视觉和结果正确。
- [ ] R-010 merge/split 视觉和结果正确。
- [x] R-011 range selection 背景连续。
  - range selection 复用 selected cell 背景和 selection border 颜色；`table_range_selection_selects_normalized_cell_rectangle` 覆盖矩形选区命中，既有 selected-cell border 测试覆盖无白缝背景。
- [ ] R-012 active cell 与 editing caret 不冲突。
- [ ] R-013 表格在页面顶部/中间/底部都能正确更新高度。
- [ ] R-014 表格在滚动后编辑，candidate rect 不错位。
- [ ] R-015 表格下方紧跟 heading/paragraph/list 时不重叠。

---

## 7. 实施顺序

1. Engine 目录拆分，建立清晰边界。
2. TableLayout 引擎和 block height 同步。
3. Cell 输入/IME/selection 状态收敛。
4. GUI 样式和 selection 视觉。
5. Row/column menu。
6. Resize。
7. Merge/split。
8. Reorder。
9. Clipboard。
10. Persistence 和 acceptance。

---

## 8. 第一阶段必须优先修复的问题

第一阶段先把“表格是一个稳定 block”做好：

- [x] S1-001 表格 cell 回车后 table height 增长。
- [x] S1-002 表格下面有内容时，下方 block 自动下移。
- [x] S1-003 表格高度变化进入 `height_index/page_layout/scroll`。
- [x] S1-004 Auto row height 和 manual row height 行为明确。
- [x] S1-005 cell 输入、IME、candidate rect 与新高度一致。
- [x] S1-006 增加完整回归测试，避免高度只在 table 内部变化。

完成记录：

- `table_cell_enter_updates_block_height_and_pushes_following_blocks_down` 覆盖 table 内部高度增长后，后续 block 使用新的 `height_index` offset。
- `table_height_change_during_scrollbar_drag_defers_displayed_total_update` 覆盖表格高度变化进入 scroll model 且不会打断滚动条拖拽。
- `table_cell_layout_cache_rejects_stale_content_version_for_candidate_bounds` 覆盖 cell 内容版本变化后旧 layout cache 不再用于 candidate rect / hit-test。

---

## 9. 完成定义

当以下条件全部满足，才认为 Notion 级表格第一版完成：

- 表格不会消失、不会被普通文本路径覆盖。
- 表格高度和文档流高度一致。
- cell 输入体验稳定，支持 IME、换行、undo/redo。
- row/column handle、selection、menu、resize、reorder、merge/split 都可用。
- internal clipboard 保留结构，external clipboard 输出可读文本。
- Postgres 保存/恢复完整保留表格结构和样式。
- `cargo check --workspace` 通过。
- `cargo test -p cditor-runtime --lib` 通过。
- `cargo test -p cditor-app --lib` 通过。
- 与表格相关的 acceptance 测试通过。
