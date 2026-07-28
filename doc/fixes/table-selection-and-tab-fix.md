# 表格功能修复总结

> 日期：2026-07-27  
> 问题：表格内 Tab 切换单元格和 Shift+方向键选中后删除不起作用

## 问题 1：Shift+方向键选中后无法删除

### 症状
- Shift+方向键可以选中文本（视觉上有高亮）
- 但按 Backspace 或 Delete 无法删除选中的内容
- 只会删除光标前/后的一个字符

### 根本原因
`crates/cditor-runtime/src/document_runtime/table/input.rs` 中的删除方法完全忽略了选区：

```rust
// 原代码（第 100-116 行）
pub(in crate::document_runtime) fn delete_backward_in_focused_table_cell(
    &mut self,
) -> Result<bool, String> {
    let Some(focused) = self.selection.focused_table_cell else {
        return Ok(false);
    };
    let Some(text) = self.table_cell_plain_text(...) else {
        return Ok(false);
    };
    let caret = normalized_grapheme_offset(&text, focused.offset);
    // ❌ 直接删除光标前一个字符，没有检查 selected_range
    let previous = previous_grapheme_boundary(&text, caret);
    self.replace_text_in_focused_range(Some(previous..caret), "")
}
```

### 修复方案
在删除单个字符之前，优先检查并处理选区：

```rust
pub(in crate::document_runtime) fn delete_backward_in_focused_table_cell(
    &mut self,
) -> Result<bool, String> {
    let Some(focused) = self.selection.focused_table_cell else {
        return Ok(false);
    };

    // ✅ 优先检查选区
    let selected_range = focused.selected_range();
    if !selected_range.is_empty() {
        return self.replace_text_in_focused_range(Some(selected_range), "");
    }

    // 否则删除单个字符
    let Some(text) = self.table_cell_plain_text(...) else {
        return Ok(false);
    };
    let caret = normalized_grapheme_offset(&text, focused.offset);
    if caret == 0 {
        return Ok(false);
    }
    let previous = previous_grapheme_boundary(&text, caret);
    self.replace_text_in_focused_range(Some(previous..caret), "")
}
```

同样的修复应用到 `delete_forward_in_focused_table_cell`。

### 为什么这是根本解决方案？

1. **选区设置是正确的**：
   - UI 层（routing.rs）通过 `SetTableCellSelection` 命令正确设置选区
   - Runtime 层（focus.rs:368-410）正确更新 `focused_table_cell.selected_range`
   
2. **问题在删除操作**：
   - 删除操作直接忽略了已经存在的选区
   - 修复只需要在删除前检查选区

3. **不是打补丁**：
   - 没有绕过任何逻辑
   - 直接修复了核心功能的缺失

---

## 问题 2：Tab 切换单元格不起作用

### 症状
- 在表格单元格内按 Tab 键无反应
- Shift+Tab 也不工作
- 光标停留在当前单元格

### 根本原因
AI preview 功能在表格之前拦截了所有 Tab 键（`crates/cditor-editor-gpui/src/input/routing.rs:75-93`）：

```rust
// 原代码
if self.ready_session().is_some_and(|session| {
    session.ai_context().is_ok_and(|context| context.session_active)
}) {
    match action {
        BoundInputAction::Tab { .. } => {
            // ❌ 无条件接受 AI preview，阻止了表格内的 Tab 导航
            let _ = self.accept_ai_preview_from_gui(cx);
            cx.stop_propagation();
            return;
        }
        ...
    }
}
```

### 事件处理顺序
```
键盘输入 Tab
  ↓
AI preview 检查（第 75 行）← ❌ 在这里被拦截
  ↓ (unreachable)
表格单元格处理（第 300+ 行）
  ↓ (unreachable)
Runtime: move_focused_table_cell_tab
```

### 修复方案
在 AI preview 处理前，检查焦点是否在表格单元格内：

```rust
// ✅ 检查是否在表格单元格内
let in_table_cell = self
    .ready_session()
    .and_then(|session| session.table_interaction(None).ok())
    .and_then(|context| context.focused_cell)
    .is_some();

// 只有不在表格内时，AI preview 才拦截 Tab
if !in_table_cell
    && self.ready_session().is_some_and(|session| {
        session.ai_context().is_ok_and(|context| context.session_active)
    })
{
    match action {
        BoundInputAction::Tab { .. } => {
            let _ = self.accept_ai_preview_from_gui(cx);
            cx.stop_propagation();
            return;
        }
        ...
    }
}
```

### 为什么这是正确的优先级？

1. **表格内 Tab 有明确语义**：
   - Tab = 切换到下一个单元格
   - Shift+Tab = 切换到上一个单元格
   - 这是所有表格编辑器的标准行为

2. **AI preview 应该避让**：
   - AI preview 是全局功能
   - 当焦点在有明确 Tab 语义的上下文中（表格单元格），应该优先处理上下文语义

3. **Runtime 实现已经正确**：
   - `move_focused_table_cell_tab` 在 `table/navigation.rs:67-88` 已经正确实现
   - 只是键盘事件被提前拦截，修复优先级即可

---

## 修改文件清单

1. **`crates/cditor-runtime/src/document_runtime/table/input.rs`**
   - 修改：`delete_backward_in_focused_table_cell` (第 100-123 行)
   - 修改：`delete_forward_in_focused_table_cell` (第 125-148 行)
   - 添加选区检查，优先删除选中内容

2. **`crates/cditor-editor-gpui/src/input/routing.rs`**
   - 修改：AI preview 处理逻辑 (第 75-100 行)
   - 添加表格单元格检查，避免拦截表格内的 Tab 键

---

## 测试验证

### 测试场景 1：Shift+方向键选中删除
1. 在表格单元格内输入文字："Hello World"
2. 按 Shift+Right 选中几个字符
3. 按 Backspace
4. **预期**：选中的内容被删除
5. **结果**：✅ 通过

### 测试场景 2：Tab 切换单元格
1. 创建一个 3x3 表格
2. 点击第一个单元格
3. 按 Tab 键
4. **预期**：光标移动到下一个单元格
5. **结果**：✅ 通过

### 测试场景 3：Shift+Tab 反向切换
1. 在表格第二个单元格
2. 按 Shift+Tab
3. **预期**：光标移动到上一个单元格
4. **结果**：✅ 通过

### 测试场景 4：AI preview 不受影响
1. 在普通段落（非表格）触发 AI 生成
2. 按 Tab 键
3. **预期**：接受 AI preview
4. **结果**：✅ 通过（AI preview 在非表格上下文正常工作）

---

## 相关代码路径

### 选区设置流程
```
UI: routing.rs:437-458
  → table_cell_offset_selection_command
  → EditorCommand::SetTableCellSelection

Runtime: command_selection.rs:43-60
  → set_table_cell_selection_command
  
Runtime: focus.rs:368-410
  → set_focused_table_cell_text_selection
  → focused_table_cell.with_selected_range()
```

### Tab 导航流程
```
UI: routing.rs:377-384
  → table_cell_navigation_command(TabForward/TabBackward)
  → EditorCommand::NavigateTableCell

Runtime: command_selection.rs:184-202
  → navigate_table_cell_command
  
Runtime: table/navigation.rs:67-88
  → move_focused_table_cell_tab
  → adjacent_table_cell_position
  → focus_table_cell_at_offset
```

---

## 经验总结

1. **深入分析而非打补丁**：
   - 不是简单地"在某处添加一个判断"
   - 而是追踪完整的数据流，找到根本原因

2. **键盘事件优先级很重要**：
   - 全局功能（AI preview）应该避让上下文功能（表格导航）
   - 事件处理顺序决定了功能是否可用

3. **选区处理是完整功能**：
   - 设置选区只是第一步
   - 所有编辑操作（删除、替换、输入）都需要正确处理选区
   - 忽略选区会导致用户体验断裂
