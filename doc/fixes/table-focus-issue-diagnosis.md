# 表格功能问题诊断报告

> 日期：2026-07-27  
> 状态：已定位根本原因，需要修复焦点管理

## 问题现象

1. **Tab 切换单元格不工作**：按 Tab 键无反应
2. **Shift+方向键选中后删除需要按两次**：第一次按删除键无效，第二次才删除

## 根本原因

通过日志追踪发现：**问题不在 Tab 或删除逻辑，而在焦点管理**

### 日志证据

```
[routing] in_table_cell=true, action=Tab { backwards: false }
[routing] handle_bound_table_cell_action: Tab { backwards: false }
[routing] table_context present: true
[routing] focused_cell present: false  ← ❌ 关键问题
[table_port] focused_table_cell_text_position: None  ← ❌ Runtime 焦点丢失
```

### 事件流程

```
用户按 Tab 键
  ↓
routing.rs:75 ✅ 检测到在表格内 (in_table_cell=true)
  ↓
routing.rs:300 ✅ 进入 handle_bound_table_cell_action
  ↓
routing.rs:305 ❌ table_interaction().focused_cell 是 None
  ↓
routing.rs:378-380 ❌ return false (所有表格操作被拦截)
```

### 代码位置

**UI 层拦截**（routing.rs:378-380）：
```rust
if table_context.focused_cell.is_none() {
    return false;  // ← 所有表格操作在这里被拦截
}
```

**焦点查询**（table_port.rs:54-56）：
```rust
let focused_cell = runtime
    .focused_table_cell_text_position()  // ← 返回 None
    .map(|(block_id, row, col, offset, affinity)| { ... });
```

**焦点状态**（focus.rs:269）：
```rust
pub fn focused_table_cell_text_position(&self) -> Option<...> {
    self.selection.focused_table_cell?  // ← None
}
```

## 为什么焦点是 None？

### 可能的原因

1. **用户点击单元格后，焦点没有正确设置**
   - 点击事件没有发送 `FocusTableCell` 命令
   - 或者命令发送了但没有执行成功

2. **焦点在某个操作后被清除**
   - 某个操作（如 Shift+方向键选区）清除了焦点
   - 或者焦点被其他操作覆盖

3. **焦点状态不同步**
   - UI 层认为焦点在表格内（`in_table_cell=true`）
   - 但 Runtime 层的 `selection.focused_table_cell` 是 None
   - 这是状态不一致的典型表现

## 如何验证？

### 测试步骤

1. 在表格单元格中点击
2. 立即按 Tab 键
3. 查看日志：
   ```
   [table_port] focused_table_cell_text_position: None  ← 应该有值
   ```

### 预期行为

点击单元格后，应该看到：
```
[table_port] focused_table_cell_text_position: Some((block_id=123, row=0, col=0, ...))
```

## 需要修复的地方

### 1. 检查单元格点击处理

文件：`crates/cditor-editor-gpui/src/surfaces/table_cell.rs`

需要确认：
- 点击单元格时是否发送 `FocusTableCell` 命令
- 命令参数是否正确（block_id, row, col, offset）

### 2. 检查焦点命令处理

文件：`crates/cditor-runtime/src/document_runtime/command_selection.rs`

需要确认：
- `FocusTableCell` 命令是否正确更新 `selection.focused_table_cell`
- 命令执行是否有错误被静默吞掉

### 3. 检查焦点保持

需要确认：
- Shift+方向键选区操作是否保持焦点
- 其他操作（如 AI preview）是否意外清除焦点

## 之前的修复是否有效？

### Shift+方向键删除修复 ✅
**假设焦点正确时**，这个修复是有效的：
```rust
// table/input.rs
let selected_range = focused.selected_range();
if !selected_range.is_empty() {
    return self.replace_text_in_focused_range(Some(selected_range), "");
}
```

### Tab 切换修复 ✅
**假设焦点正确时**，这个修复是有效的：
```rust
// routing.rs
let in_table_cell = ...; 
if !in_table_cell && ai_session_active {
    // AI preview 只在非表格内拦截 Tab
}
```

**但是**：这两个修复都依赖于 `focused_cell` 不为 None，而当前的问题是焦点根本就是 None。

## 下一步行动

### 优先级 1：修复焦点设置

添加日志追踪完整的焦点生命周期：
1. 点击单元格时
2. 发送 FocusTableCell 命令时
3. Runtime 处理命令时
4. 焦点状态变化时

### 优先级 2：焦点保持

确保焦点在以下操作后保持：
- Shift+方向键选区
- 删除操作
- 输入文字

### 优先级 3：状态一致性

修复 UI 层和 Runtime 层的状态不一致：
- `in_table_cell` 检查应该基于 Runtime 的焦点状态
- 不应该有两套独立的状态判断

## 临时解决方案

如果焦点问题短期无法修复，可以考虑：

1. **放宽焦点检查**：
   ```rust
   // routing.rs:378-380
   // if table_context.focused_cell.is_none() {
   //     return false;  // ← 注释掉这个检查
   // }
   ```
   
   但这可能导致其他问题（在非表格上下文执行表格命令）

2. **自动恢复焦点**：
   在 `handle_bound_table_cell_action` 开始时，如果 `focused_cell` 是 None，尝试从 UI 状态恢复焦点

## 总结

- ✅ 之前的修复（删除选区、Tab 优先级）逻辑正确
- ❌ 但被焦点管理问题阻塞
- 🔧 需要修复焦点设置和保持机制
- 📊 需要更多日志来追踪焦点生命周期
