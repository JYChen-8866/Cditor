# V1 有序列表 / 无序列表 / 任务列表：Enter、Tab、Shift+Tab 迁移分析与任务清单

> 目标：把 V1 `editor2` 已验证的列表编辑交互完整迁移到 V2 runtime 架构；V2 仍保持“大文档 runtime/projection 为真相”，不能把 GPUI entity / ListState 重新作为结构真相。

## 范围

本文只覆盖三类列表 block：

- 无序列表：V1 `BlockKind::BulletedListItem` → V2 `RichBlockKind::BulletedList`
- 有序列表：V1 `BlockKind::NumberedListItem` → V2 `RichBlockKind::NumberedList`
- 任务列表：V1 `BlockKind::TaskListItem { checked }` → V2 `RichBlockKind::Todo { checked }`

重点行为：

- 普通 `Enter`
- `Cmd/Ctrl+Enter`
- `Tab`
- `Shift+Tab` / `BackTab`
- 行首 `Backspace` 取消当前 block 样式并退回 Paragraph
- 空列表项退出列表
- 有序列表 ordinal / 层级缩进保持
- 与大文档性能、结构事务、Postgres 保存链路兼容

---

## V1 行为来源

只读参考路径：

- `/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs`
- `/Users/jychen/Desktop/Cditor/src/editor2/runtime/indexed_document.rs`
- `/Users/jychen/Desktop/Cditor/src/editor2/runtime/tree.rs`

### V1 keydown 分层

V1 `CditorBlock::on_key_down` 对列表相关键的处理顺序：

1. IME in progress 时直接 return。
2. `Cmd/Ctrl+Enter`：拆分当前 block，但 `inherit_kind=false`，新 block 是 Paragraph。
3. Table focused cell 特殊处理 Tab，不进入列表缩进。
4. code language menu / slash menu 特殊处理。
5. `Tab` / `Shift+Tab`：
   - soft-enter block：插入/删除 4 个空格。
   - 普通 block：发 `RequestIndent` / `RequestOutdent`。
6. 普通 `Enter`：
   - code fence shortcut 优先。
   - soft-enter block 插入 `\n`。
   - 普通 block `split_at_cursor()`，发 `RequestNewline { inherit_kind: true }`。
7. 普通 `Backspace`：
   - V1 对非 soft-enter block 在 caret=0 时优先 merge/delete；对 soft-enter block caret=0 时通常无动作。
   - V2 目标行为按当前产品要求增强：caret=0 且当前 block 是可文本化样式块时，先取消样式并退回 Paragraph，保留正文，不合并/删除。

V1 关键代码语义：

```rust
// entity.rs
"enter" => {
    if let Some((old, new)) = self.apply_code_fence_shortcut_on_enter() {
        ...
    } else if uses_soft_enter(self.kind()) {
        self.paste_plain_text("\n");
    } else {
        let trailing = self.split_at_cursor();
        cx.emit(CditorBlockEvent::RequestNewline {
            block_id: self.id(),
            trailing,
            inherit_kind: true,
        });
    }
}
```

```rust
// entity.rs
if is_tab_key && !modifiers.platform && !modifiers.control && !modifiers.alt {
    if uses_soft_enter(self.kind()) {
        if is_outdent_key { self.outdent_soft_line() } else { self.insert_soft_tab() };
    } else if is_outdent_key {
        cx.emit(CditorBlockEvent::RequestOutdent { block_id: self.id() });
    } else {
        cx.emit(CditorBlockEvent::RequestIndent { block_id: self.id() });
    }
}
```

### V1 Enter 新建 block 语义

V1 `indexed_document.rs::insert_block_after`：

```rust
let new_kind = if inherit_kind {
    newline_sibling_kind(&current_record.kind)
} else {
    BlockKind::Paragraph
};
let mut block = BlockRecord::new(new_kind, trailing);
block.parent = parent_id;
```

`newline_sibling_kind`：

```rust
TaskListItem { .. } => TaskListItem { checked: false }
BulletedListItem => BulletedListItem
NumberedListItem => NumberedListItem
Quote => Quote
Callout { variant } => Callout { variant }
_ => Paragraph
```

结论：

| 场景 | V1 行为 |
|---|---|
| 非空 bullet 按 Enter | 当前 block 在 caret 前保留 leading，后续 trailing 进入新 bullet；新 block 和当前同 parent |
| 非空 numbered 按 Enter | 同上，新 block 是 numbered；ordinal 由 list projection 重新计算 |
| 非空 task 按 Enter | 同上，新 block 是 task 且 `checked=false` |
| Cmd/Ctrl+Enter | 拆分但新 block 强制 Paragraph |
| 空 root list item 按 Enter | 当前 block 转 Paragraph |
| 空 nested list item 按 Enter | outdent 一层 |

### V1 空列表项退出逻辑

V1 `insert_block_after` 在插入前判断：

```rust
if inline_spans_are_empty(&trailing)
    && current_record.title_is_empty()
    && exits_on_empty_enter(&current_record.kind)
{
    let depth = self.list_info_for_index(after_index).depth;
    if depth > 0 && is_list_item_kind(&current_record.kind) {
        return self.outdent_block(after_id, cx);
    }
    current.set_kind(BlockKind::Paragraph);
    return SetBlockKind Paragraph;
}
```

含义：

- 空列表项 Enter 不创建新 block。
- root 空列表项退出为 Paragraph。
- nested 空列表项不退出列表，而是降低一级缩进。

### V1 Tab / Shift+Tab 语义

V1 有两套历史实现：

#### `indexed_document.rs` 当前 flat-list 版本

`indent_block`：

- 找到当前 block 的 flat index。
- 找到上一个 visible block。
- 若上一个 block 不支持 children，则不缩进。
- 新 depth = previous depth + 1。
- 发结构事务 `MoveBlock { new_parent_id: Some(previous_id), new_position: 0 }`。

`outdent_block`：

- depth=0 不处理。
- 新 depth = current depth - 1。
- 当前实现里 transaction 是 `new_parent_id: None, new_position: index`，但内存侧主要靠 `list_info_overrides_by_id` 改显示层级。

#### `tree.rs` 更准确的树语义参考

`indent`：

- 当前 block 必须不是同 parent 下第一个 sibling。
- previous sibling 必须支持 children。
- 移到 previous sibling children 末尾。

`outdent`：

- 当前 block 必须有 parent。
- 移到 parent 的 parent 下，位置是 parent 后一位。

V2 应采用 `tree.rs` 语义作为最终结构真相，因为 V2 已经是 runtime index/tree projection，不应只改 list_info override。

---

## V2 当前状态对照

### 已具备

- [x] 键盘映射：`Tab` → `GuiInputCommand::IndentBlock`。
- [x] 键盘映射：`Shift+Tab` → `GuiInputCommand::OutdentBlock`。
- [x] 键盘映射：普通 `Enter` → `GuiInputCommand::HandleEnter`。
- [x] 键盘映射：`Cmd/Ctrl+Enter` → `GuiInputCommand::InsertParagraphAfterFocused`。
- [x] `handle_enter` 已处理 code fence shortcut。
- [x] `handle_enter` 已处理 Code / Quote / Callout / RawMarkdown soft line break。
- [x] 空 root list Enter：当前 list block 转 Paragraph。
- [x] 空 nested list Enter：调用 `outdent_block`。
- [x] `Tab` 基础 indent：需要 previous block 支持 children。
- [x] `Shift+Tab` 基础 outdent：depth=0 不处理，nested block 降一级。
- [x] list projection 已能生成 `depth`、numbered ordinal、Todo prefix。
- [x] Todo checkbox toggle 已写回 payload kind。
- [x] Postgres 保存链路已能保存 payload、blocks、index snapshot、structure transactions。

### 与 V1 完整行为的主要差距

当前 Enter / Tab / Shift+Tab 的 V1 列表核心语义已迁移完成：

- [x] 非空列表 Enter 按 V1 继承列表 kind。
- [x] Enter 按 caret 完整 split 当前文本，trailing 进入新 block。
- [x] Task Enter 新 item 强制 unchecked。
- [x] Cmd/Ctrl+Enter split trailing，但新 block 强制 Paragraph。
- [x] Tab indent 采用 `tree.rs` 语义：移到 previous sibling 的 children 末尾。
- [x] Shift+Tab outdent 采用 `tree.rs` 语义：移到 parent subtree 后面，成为 parent 的 sibling。
- [x] Indent/Outdent 记录结构事务，并进入 undo/redo 与 Postgres pending transaction。
- [x] 列表 Enter/Tab 后 focus/caret 与 V1 对齐：split 聚焦新 block，outdent/退出列表保持当前 block focus。
- [x] 有序列表 ordinal 覆盖 Enter / indent / outdent 后的重新计算。
- [x] 行首 Backspace 取消可文本化 block 样式：Heading / Quote / Callout / List / Todo / Toggle / Code / Math / Mermaid / Html / RawMarkdown / Footnote / Comment / Custom → Paragraph，保留正文和 caret=0。

剩余工作不在本列表键盘核心语义内，主要是后续 UI action menu / 连续 edge auto-scroll ticker 等体验增强。

---

## V2 最终设计

### Runtime 原则

- Runtime 是结构真相。
- UI 只发 command，不直接改 list_info。
- 所有 Enter/Tab 结构变化必须通过 runtime 修改 `DocumentIndex`。
- 大文档只做 O(subtree + visible window) 或 O(records) 的必要结构 rebuild；不在输入热路径做 Postgres 同步写。
- 保存使用现有 debounce Postgres saver。

### 新增核心 API 设计

#### 1. `handle_enter` 拆分为策略函数

```rust
pub fn handle_enter(&mut self) -> Result<(), String> {
    let block_id = focused?;
    let kind = kind(block_id);
    let text = text(block_id);

    if code_fence_shortcut { ... }
    if empty list item { return handle_empty_list_enter(block_id); }
    if uses_soft_enter(kind) { return insert_soft_line_break(); }
    return split_focused_block_at_caret(EnterSplitMode::InheritV1Kind);
}
```

#### 2. split mode

```rust
enum EnterSplitMode {
    InheritV1Kind,
    ForceParagraph,
}
```

普通 Enter 用 `InheritV1Kind`。

Cmd/Ctrl+Enter 用 `ForceParagraph`。

#### 3. V1 newline sibling kind 映射

```rust
fn newline_sibling_kind_for_v1(kind: &RichBlockKind) -> RichBlockKind {
    match kind {
        RichBlockKind::Todo { .. } => RichBlockKind::Todo { checked: false },
        RichBlockKind::BulletedList => RichBlockKind::BulletedList,
        RichBlockKind::NumberedList => RichBlockKind::NumberedList,
        RichBlockKind::Quote => RichBlockKind::Quote,
        RichBlockKind::Callout { variant } => RichBlockKind::Callout { variant: *variant },
        _ => RichBlockKind::Paragraph,
    }
}
```

#### 4. split payload

对 rich text：

- leading spans 留在当前 block。
- trailing spans 放入新 block。
- mark 范围按 inline span range 切分。

对 code/raw/plain text：

- 若走 soft-enter，不 split block。
- 若 Cmd/Ctrl+Enter 强制 split，可按 plain text 切分；Code 是否继承为 Paragraph 需保持 V1 `inherit_kind=false` 行为：new Paragraph 包含 trailing plain text。

#### 5. 插入位置和 parent

普通 split 新 block：

- parent = current parent。
- insert index = current subtree end? 需要按 V1 行为判定：V1 flat `insert_root_record_after(after_index, &block)`，对当前可见 flat index 后插入；最终 V2 应保证新 block 是当前 block 的 sibling，而不是 child。
- 对有 children 的 list item，按 Notion 类行为通常应插在当前 block subtree 后，避免把新 sibling 插到 children 前面。该点需要用 V1 实际 `insert_root_record_after` 语义二次确认；V2 推荐用 `subtree_end(current)`，保证树稳定。

### Tab / Shift+Tab 最终结构语义

#### Tab

仅对 list item / list-capable block 执行结构缩进：

1. 找当前 block 的 parent 和 sibling index。
2. sibling index == 0：不处理。
3. previous sibling 必须支持 children。
4. 当前 block 整个 subtree 移到 previous sibling children 末尾。
5. depth delta = +1。
6. focus 保持当前 block。
7. 记录 structure transaction。
8. projection 重建 list ordinal。

#### Shift+Tab

1. depth == 0 或 parent == None：不处理。
2. 找 parent 的 parent。
3. 当前 block 整个 subtree 移到 parent 后一个 sibling 位置。
4. depth delta = -1。
5. focus 保持当前 block。
6. 记录 structure transaction。
7. projection 重建 list ordinal。

### soft-enter block 的 Tab

V1 对 `uses_soft_enter(kind)` 的 block：

- `Tab`：当前行插入 4 个空格。
- `Shift+Tab`：删除当前行开头最多 4 个空格。

V2 当前把所有 `Tab` 都当结构 indent；后续要补 Code / Quote / Callout / RawMarkdown soft-tab 行为，尤其 Code block。

---

## 任务列表

### A. V1 行为分析

- [x] A-001 对照 V1 `entity.rs` 普通 Enter 行为。
- [x] A-002 对照 V1 `entity.rs` `Cmd/Ctrl+Enter` 行为。
- [x] A-003 对照 V1 `entity.rs` `Tab` / `Shift+Tab` 键名兼容行为。
- [x] A-004 对照 V1 `indexed_document.rs::insert_block_after` 空列表项退出行为。
- [x] A-005 对照 V1 `newline_sibling_kind` 对 bullet / numbered / task 的继承规则。
- [x] A-006 对照 V1 `tree.rs` indent/outdent 准确树语义。

### B. V2 当前能力确认

- [x] B-001 确认 V2 keyboard：普通 Enter / Cmd+Enter / Tab / Shift+Tab 已有 command 映射。
- [x] B-002 确认 V2 空 root list Enter 已转 Paragraph。
- [x] B-003 确认 V2 空 nested list Enter 已 outdent。
- [x] B-004 确认 V2 list projection 已支持 depth / ordinal / todo prefix。
- [x] B-005 确认 V2 Postgres saver 可保存结构和 payload。

### C. Enter：非空列表完整迁移

- [x] C-001 新增 `EnterSplitMode::{InheritV1Kind, ForceParagraph}`。
- [x] C-002 新增 `newline_sibling_kind_for_v1(&RichBlockKind)`。
- [x] C-003 新增 rich spans 按 caret offset 切分 helper，保留 inline marks。
- [x] C-004 新增 plain text / code text 切分 helper。
- [x] C-005 新增 `split_focused_block_at_caret(mode)` runtime API。
- [x] C-006 普通 bullet Enter：新 block 继承 `BulletedList`。
- [x] C-007 普通 numbered Enter：新 block 继承 `NumberedList`。
- [x] C-008 普通 task Enter：新 block 为 `Todo { checked: false }`。
- [x] C-009 普通 paragraph Enter：新 block 为 Paragraph。
- [x] C-010 普通 quote/callout/code/raw markdown Enter 仍走 soft line break，不引入回归。
- [x] C-011 Cmd/Ctrl+Enter 改为 split trailing，但新 block 强制 Paragraph。
- [x] C-012 split 后当前 block content_version 更新，新 block content_version 初始化。
- [x] C-013 split 后 focus 到新 block，caret 在新 block 开头。
- [x] C-014 split 后高度估算使用统一 `block_metrics.rs`，不破坏虚拟滚动。
- [x] C-015 split 后触发 Postgres payload + structure 保存。

### D. Empty list Enter 细化

- [x] D-001 root 空 list Enter：转 Paragraph。
- [x] D-002 nested 空 list Enter：outdent 一层。
- [x] D-003 root 空 task Enter：转 Paragraph 且清掉 checkbox prefix。
- [x] D-004 nested 空 task Enter：保留 task kind，仅降低层级；按 V1 空 nested list outdent 语义处理。
- [x] D-005 空 list 判断使用 plain_text trim，而不是只看 spans 数量。
- [x] D-006 空 list Enter 不创建新 block，不改变 scroll top。

### E. Tab：Indent 完整迁移

- [x] E-001 新增 `indent_block_v1_tree_semantics(block_id)`。
- [x] E-002 当前 block 是同 parent 第一个 sibling 时不处理。
- [x] E-003 previous sibling 不支持 children 时不处理。
- [x] E-004 当前 subtree 移到 previous sibling children 末尾。
- [x] E-005 subtree 所有 descendant depth +1。
- [x] E-006 focus 保持当前 block。
- [x] E-007 操作必须记录轻量 structure transaction。
- [x] E-008 操作必须进入 global undo/redo 顺序队列。
- [x] E-009 操作必须进入 Postgres pending structure transactions。
- [x] E-010 有序列表 ordinal 在 indent 后按新层级重新计算。

### F. Shift+Tab：Outdent 完整迁移

- [x] F-001 新增 `outdent_block_v1_tree_semantics(block_id)`。
- [x] F-002 root block / depth=0 不处理。
- [x] F-003 找到 parent 和 grandparent。
- [x] F-004 当前 subtree 移到 parent subtree 后，成为 parent 的 sibling。
- [x] F-005 subtree 所有 descendant depth -1。
- [x] F-006 focus 保持当前 block。
- [x] F-007 操作必须记录轻量 structure transaction。
- [x] F-008 操作必须进入 global undo/redo 顺序队列。
- [x] F-009 操作必须进入 Postgres pending structure transactions。
- [x] F-010 有序列表 ordinal 在 outdent 后按新层级重新计算。

### G. Soft-enter block 的 Tab / Shift+Tab


- [x] G-001 定义 V2 `uses_soft_enter_for_tab(kind)`，覆盖 Code / RawMarkdown / Quote / Callout，按 V1 `uses_soft_enter` 口径处理。
- [x] G-002 Code block `Tab` 在 caret 插入 4 个空格。
- [x] G-003 Code block `Shift+Tab` 删除当前行行首最多 4 个空格。
- [x] G-004 RawMarkdown 同 Code 行为。
- [x] G-005 soft-tab 后只保存 payload，不保存结构。
- [x] G-006 soft-tab 后刷新当前 block 高度。

### G2. 行首 Backspace 取消 block 样式

- [x] G2-001 对照 V1：V1 行首 Backspace 对非 soft-enter 倾向 merge/delete，空 Enter 对 list/quote/callout 退 Paragraph。
- [x] G2-002 按当前 V2 产品要求增强：caret=0 时先取消可文本化 block 样式，不合并、不删除。
- [x] G2-003 Heading → Paragraph，保留正文。
- [x] G2-004 BulletedList / NumberedList / Todo → Paragraph，清掉 prefix/checkbox，保留正文。
- [x] G2-005 Quote / Callout / Toggle → Paragraph，保留正文。
- [x] G2-006 Code / RawMarkdown / Html / Math / Mermaid → Paragraph，保留 plain text。
- [x] G2-007 FootnoteDefinition / Comment / Custom → Paragraph，保留 plain text。
- [x] G2-008 Paragraph caret=0 不处理，避免误删/误改。
- [x] G2-009 不处理 Table / Image / File / Attachment / Whiteboard / MindMap / Embed / Database / Divider / Separator，避免丢结构化数据。
- [x] G2-010 行为落在 runtime，不由 UI entity 决定；undo 使用现有 text snapshot，不引入 UI 真相。

### H. 测试任务

- [x] H-001 `enter_on_bulleted_list_splits_and_inherits_kind`。
- [x] H-002 `enter_on_numbered_list_splits_and_inherits_kind`。
- [x] H-003 `enter_on_todo_splits_and_new_item_is_unchecked`。
- [x] H-004 `enter_splits_trailing_rich_spans_and_preserves_marks`。
- [x] H-005 `command_enter_splits_but_forces_paragraph`。
- [x] H-006 `enter_on_empty_root_list_turns_it_into_paragraph` 已有。
- [x] H-007 `enter_on_empty_nested_list_outdents_it` 已有。
- [x] H-008 `tab_indents_block_under_previous_sibling_children_tail`。
- [x] H-009 `tab_first_sibling_does_nothing`。
- [x] H-010 `tab_previous_non_container_does_nothing`。
- [x] H-011 `shift_tab_outdents_block_after_parent_subtree`。
- [x] H-012 `shift_tab_root_block_does_nothing`。
- [x] H-013 `indent_outdent_preserve_subtree_children`。
- [x] H-014 `indent_outdent_queue_structure_transactions`。
- [x] H-015 `indent_outdent_undo_redo_restores_tree`。
- [x] H-016 `numbered_ordinal_recomputes_after_enter_indent_outdent`。
- [x] H-017 `code_tab_inserts_four_spaces_without_structure_change`。
- [x] H-018 `code_shift_tab_removes_line_indent_without_structure_change`。
- [x] H-019 `raw_markdown_tab_and_shift_tab_are_payload_only`。
- [x] H-020 `enter_on_empty_root_todo_turns_paragraph_and_clears_checkbox`。
- [x] H-021 `enter_on_empty_nested_todo_outdents_and_preserves_todo_kind`。
- [x] H-022 `enter_on_whitespace_only_list_item_uses_trim_empty_check`。
- [x] H-023 `enter_on_empty_list_does_not_create_block_or_move_scroll_top`。
- [x] H-024 `backspace_at_start_resets_textual_block_styles_to_paragraph`。
- [x] H-025 `backspace_at_start_resets_code_and_html_payloads_to_paragraph_without_losing_text`。
- [x] H-026 `backspace_at_start_keeps_plain_paragraph_unchanged`。

### I. 性能 / 架构约束任务

- [ ] I-001 Enter split 只修改当前 block、新 block、结构索引，不扫描 payload 全文档。
- [ ] I-002 Tab/Shift+Tab 只移动 source subtree，避免 full payload clone。
- [ ] I-003 projection 重建可接受 O(index)；不得引入 UI entity truth。
- [ ] I-004 不恢复 GPUI `ListState` 作为滚动或结构真相。
- [ ] I-005 不在 keydown/IME 同步写 Postgres。
- [ ] I-006 高度变化必须走 `BlockLayoutMeta` / `block_metrics.rs`。
- [ ] I-007 10w 文档 Enter/Tab 操作后 scroll_top 不跳。

---

## 推荐实施顺序

1. 先做 `split_focused_block_at_caret`，补齐 Enter 对列表 kind / trailing 的 V1 行为。
2. 再把 `insert_paragraph_after_focused` 改为 Cmd/Ctrl+Enter 的 split paragraph 语义，或新增单独 API 避免影响其它调用。
3. 重写 `indent_block/outdent_block` 为 V1 tree semantics，并复用已经成熟的 `move_block_subtree_to_parent_untracked` / structure transaction 机制。
4. 最后补 Code/RawMarkdown soft-tab，因为它是 payload-only，不应混在结构迁移里。

## 风险点

- V1 `indexed_document.rs` 的 indent/outdent 有 display override 色彩；V2 不能照搬 override，必须以 runtime tree/index 为真相。
- V1 普通 Enter 使用 `insert_root_record_after(after_index, &block)`；V2 若当前 block 有 children，建议插入到 current subtree 后，避免破坏树 preorder。实现前需要再用 V1 实测确认“list item 有 children 时 Enter 插在哪里”。
- Task list Enter 必须重置 checked=false，否则会复制已完成状态，和 V1 不一致。
- 有序列表 ordinal 不能存死，必须 projection 阶段按当前 visible tree 计算。
