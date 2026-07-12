# V1 表格完整迁移方案

目标：在 CDitor V2 中复刻 `/Users/jychen/Desktop/Cditor` 的 V1 表格体验，包括样式、cell 聚焦、输入编辑、光标/选区和后续行列操作，同时保持 V2 架构约束。

## 0. 架构原则

- Runtime 是表格状态真相：focused cell、cell selection、cell text、content version、layout height 都在 runtime / payload / projection 中表达。
- GUI 只消费 projection，不把表格编辑状态保存在 GPUI Entity 中。
- 不把 10w 文档或大表格状态塞进 UI Entity/ListState。
- 表格输入、IME、保存不得同步等待 PostgreSQL。
- 表格高度变化必须回写 runtime layout/scroll，避免软换行或多行 cell 显示不全。

## 1. V1 能力对照

V1 参考文件：

- `/Users/jychen/Desktop/Cditor/src/editor2/block/table.rs`
- `/Users/jychen/Desktop/Cditor/src/editor2/component/table/mod.rs`

V1 已有能力：

- cell/row runtime 保存 `text/revision/layout`
- `focused_cell: Option<TableCellPosition>`
- `selected_range` / `selection_reversed` / `marked_range`
- 点击 cell 后 focus parent block，并触发 `FocusRequested { row, cell }`
- active cell 样式：
  - border：`theme.table_active_border` / `0x60a5fa`
  - background：`theme.action_background` / `0xdbeafe`
- header row 背景：`theme.table_header_background` / `0xf1f5f9`
- cell 几何：
  - radius：`8px`
  - min width：`120px`
  - padding x/y：`10px / 8px`
- cell 文本编辑：
  - `replace_focused_range`
  - `set_cursor_offset`
  - `move_cursor_left`
  - `move_cursor_right`

## 2. V2 当前状态

已完成：

- [x] 静态表格基础视觉：外框、header 背景、cell padding/min width、边框色。
- [x] V1 表格颜色常量已进入 `GuiTheme`。

未完成：

- [ ] cell 点击聚焦。
- [ ] active cell 样式由 runtime/projection 驱动。
- [ ] cell 内光标。
- [ ] cell 内选区。
- [ ] cell 文本输入。
- [ ] cell IME composition。
- [ ] Tab / Shift+Tab / Enter 在 cell 中的行为。
- [ ] 行列增删操作。
- [ ] cell 内容变化后的高度测量和 scroll/layout 修正。
- [ ] PostgreSQL autosave 保存 table cell 修改。

## 3. 迁移任务清单

### 阶段 A：表格聚焦与 active cell 样式

- [x] 写 V1 表格完整迁移方案。
- [x] 在 runtime 中增加 focused table cell 状态。
- [x] projection 暴露当前 block 的 focused table cell。
- [x] GUI table cell 点击时调用 runtime 聚焦 cell。
- [x] GUI 根据 projection 渲染 active cell border/background。
- [x] 添加 runtime/projection 单元测试。
- [x] 验证 `cargo test gui::block --lib`、`cargo test runtime::document_runtime --lib`、`cargo check`。

### 阶段 B：cell 文本输入基础

- [x] `TablePayload` 提供 cell plain text 读写 helper。
- [x] runtime 实现 `replace_focused_table_cell_range`。
- [x] keyboard/input 插入字符优先写入 focused table cell。
- [x] Backspace/Delete 作用于 focused table cell。
- [x] 修改后 content_version 增加，并触发 dirty/autosave。
- [x] 添加 table cell 输入测试。

### 阶段 C：cell 光标和选区

- [ ] projection 暴露 table cell caret offset / selection range / marked range。
- [ ] table cell 使用富文本 layout 元素渲染 caret/selection。
- [ ] 左右方向键移动 cell 内光标。
- [ ] 鼠标点击 cell 文本位置设置 caret。
- [ ] Shift+方向键扩展 cell 选区。

### 阶段 D：IME 和多行高度

- [ ] focused table cell 支持 IME composition preview。
- [ ] platform selected range / fallback range 支持 table cell。
- [ ] Shift+Enter 或多行输入后重新估算/测量 table block 高度。
- [ ] 高度变化修正 scroll anchor。

### 阶段 E：表格导航和结构操作

- [ ] Tab 移动到下一个 cell。
- [ ] Shift+Tab 移动到上一个 cell。
- [ ] Enter 行为对齐 V1。
- [ ] 如 V1 有行列增删入口，按 V2 runtime/projection 重做。
- [ ] 行列变化保存到 PostgreSQL table payload。

## 4. 注意事项

- 不能直接移植 V1 的 `CditorTableCell` / `CditorTableRow` entity runtime 作为 V2 真相。
- render 阶段不要 `view.update(...)` 当前 entity，避免 GPUI 重入 panic。
- cell click 可以通过事件 handler 更新 view/runtime，但不能在渲染时同步修改 runtime。
- 表格后续如果支持大表，需要保留 row virtualization 的设计空间。
