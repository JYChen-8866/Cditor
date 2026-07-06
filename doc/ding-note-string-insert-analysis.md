# ding-note2 字符串插入行为分析与 V2 修复方案

> 目标：分析 `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor` 如何处理字符串插入、selection replacement、IME marked replacement，并修复 CDitor-V2 中“插入字符串会替换字符串，然后 caret 跑到最后面”的问题。
>
> 架构约束：继续遵守 `doc/large-document-rich-text-architecture.md`。runtime 是 selection/caret/IME/text truth；GUI 只发 command 或 platform input 请求，不能在输入前重置 runtime caret。

---

## 1. ding-note2 关键源码

| 能力 | 文件 | 关键点 |
|---|---|---|
| GPUI 输入桥接 | `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/input.rs` | `EntityInputHandler` 把 UTF16 range 转 UTF8 range，按 `range_utf16 -> marked_range -> selected_range` 选择替换范围 |
| 字符串插入核心 | `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/runtime/mod.rs` | `replace_text_in_visible_range(visible_range, new_text, selected_range_relative, mark_inserted_text, cx)` |
| 跨 block selection | `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/editor/events.rs` + `selection.rs` | 跨 block selection 先发 editor event，不在单 block 内强行替换 |

---

## 2. ding-note2 的插入规则

### 2.1 普通 platform input

`replace_text_in_range` 的核心优先级：

```rust
let visible_range = range_utf16
    .as_ref()
    .map(|range| self.range_from_utf16(range))
    .or(self.marked_range.clone())
    .unwrap_or(self.selected_range.clone());
self.replace_text_in_visible_range(visible_range, new_text, None, false, cx);
```

结论：

1. 如果 GPUI 给了 explicit UTF16 range，就替换这个 range。
2. 否则如果有 IME marked range，就替换 marked range。
3. 否则替换当前 selected_range。
4. 如果 selected_range 是 collapsed，就等价于在 caret 插入。

**不会在插入前重新 focus block，也不会把 caret 重置到末尾。**

### 2.2 IME composition

`replace_and_mark_text_in_range` 同样使用：

```rust
range_utf16 -> marked_range -> selected_range
```

并把 `new_selected_range_utf16` 转为相对 UTF8 range：

```rust
let selected_range_relative = new_selected_range_utf16
    .as_ref()
    .map(|range_utf16| Self::utf16_range_to_utf8_in(new_text, range_utf16));
```

然后在 `replace_text_in_visible_range` 内：

```rust
let selected_range = selected_range_relative.as_ref().map(|relative| {
    let absolute = clean_range.start + relative.start..clean_range.start + relative.end;
    result.map_range(&absolute)
});
let cursor = selected_range
    .as_ref()
    .map(|range| range.end)
    .unwrap_or_else(|| result.map_offset(clean_range.start + new_text.len()));
```

结论：

- IME 有 selected subrange 时，caret 跟随 selected subrange end。
- 否则 caret 在 inserted text 后面。
- 不会跑到整行末尾。

### 2.3 rich text 样式保留

`replace_text_in_visible_range` 不是简单把整行重建为 plain text，而是通过 inline tree 的 range edit：

```rust
base_title.replace_visible_range_with_link_references(...)
```

这可以保留未编辑区域的 inline styles，并让插入文本继承当前位置 attributes。

---

## 3. V2 问题定位

### 3.1 caret 跑到最后面的直接原因

V2 GUI command 当前有这个逻辑：

```rust
GuiInputCommand::InsertChar(ch) => {
    let block_id = runtime.focused_block_id().unwrap_or(3);
    runtime.focus_block(block_id); // 问题：每次输入前都重置 caret 到 text_len
    if runtime.insert_char(ch).is_ok() {
        self.mark_dirty(cx);
    }
}
```

`DocumentRuntime::focus_block(block_id)` 的语义是 focus block，并把 caret 放到文本末尾：

```rust
let text_len = ...;
EditingSession::start(... text_offset: text_len ...)
```

所以当用户点击中间或移动 caret 后，只要通过 `InsertChar` command 输入字符，GUI 会先把 caret 重置到末尾，再调用 runtime insert。

这与 ding-note2 相背离：ding-note2 插入前不会重新 focus；它只使用已有 `selected_range/cursor_offset/marked_range`。

### 3.2 字符串替换问题

V2 runtime `replace_text_in_focused_range(None, text)` 的优先级已经接近 ding-note2：

```text
explicit range
  -> focused selection
  -> active composition range
  -> caret..caret
```

但是如果 GUI 先调用 `focus_block()`，selection/caret 就被清空并重置到末尾，于是 runtime 只能按末尾 collapsed range 插入。

### 3.3 V2 第二处背离：普通字符同时走 keydown 与 IME/input handler

继续对比 ding-note2 后确认：`Block::on_block_key_down` 只处理 Tab 这类控制键，普通字符不在 keydown 中插入，而是 100% 交给 GPUI `EntityInputHandler`：

```rust
pub(crate) fn on_block_key_down(...) {
    if event.keystroke.key != "tab" {
        return;
    }
    ...
}
```

V2 原先在 root `on_key_down` 中把普通字符映射为 `GuiInputCommand::InsertChar`，同时 text element 又在 `paint()` 中注册了 `window.handle_input(...)`。这会形成两条文字输入通道：

1. `KeyDownEvent -> GuiInputCommand::InsertChar -> runtime.insert_char`
2. `EntityInputHandler::replace_text_in_range / replace_and_mark_text_in_range -> runtime.replace_text_in_focused_range / begin_or_update_composition`

这与 ding-note2 的 IME 模型背离，容易让平台选区、marked range、runtime caret 在同一输入周期内互相覆盖。

### 3.4 V2 第三处背离：composition/marked range 优先级低于 selection

Ding-note2 的 replacement range 优先级是：

```text
explicit UTF16 range -> marked_range -> selected_range
```

V2 `replace_text_in_focused_range(None, text)` 曾经是：

```text
selection -> active composition -> caret
```

如果 composition 开始前存在旧 selection，后续提交/替换可能会先消费旧 selection，而不是当前 marked range。已改为：

```text
explicit range -> active composition/marked range -> focused selection -> caret
```

并且 composition update 会清理旧 focused/document selection，保持 marked range 是 IME 阶段的唯一替换真相。

### 3.5 样式丢失问题已同步修复

之前 V2 的 payload sync 会把 rich text 整行重建成 plain span。已改为 range-aware span replace：

- 删除 bold span 内字符，剩余文本保留 bold。
- 在 bold span 中插入字符，新字符继承 bold。
- `**abc**` 回归测试确认生成 `Bold` 而不是 `Italic`。

---

## 4. 修复方案

### A. GUI InsertChar 不得重置 focused block

- [x] A-001 分析 ding-note2：插入前不调用 focus reset。
- [x] A-002 修改 V2 `GuiInputCommand::InsertChar`：已有 focused block 时不再调用 `runtime.focus_block(block_id)`。
- [x] A-003 仅当 runtime 完全没有 focused block 时，才 fallback focus 默认 block。
- [x] A-004 保持 `DocumentRuntime::insert_char` 使用当前 caret / selection。
- [x] A-005 对齐 ding-note2：普通字符不再由 root `on_key_down` 插入，统一交给 GPUI `EntityInputHandler` / IME 通道。

### B. 字符串 replacement 语义对齐 ding-note2

- [x] B-001 `replace_text_in_range` 使用 `range_utf16 -> composition/marked -> selection/caret`。
- [x] B-002 `replace_and_mark_text_in_range` 使用 `new_selected_range`。
- [x] B-003 增加测试：caret 在字符串中间，GUI InsertChar 不跑到末尾。
- [x] B-004 增加测试：`replace_text_in_focused_range(None, "XYZ")` 在 caret 中间插入字符串，不替换整行。
- [x] B-005 增加测试：有 selection 时字符串替换 selection，caret 在 inserted end。
- [x] B-006 runtime replacement 优先级改为 `explicit range -> active composition/marked -> selection -> caret`。
- [x] B-007 composition update 清理旧 selection，避免 IME marked range 被旧 selection 覆盖。
- [x] B-008 增加测试：active composition 优先于旧 selection。

### C. inline style 保留

- [x] C-001 删除 bold span 内字符保留 bold。
- [x] C-002 bold span 中插入字符继承 bold。
- [x] C-003 `**abc**` 解析为 Bold，不是 Italic。

### D. 性能约束

- [x] D-001 插入字符串只改当前 block payload，不触发全文 parse。
- [x] D-002 不同步等待 PostgreSQL。
- [x] D-003 不使用 GPUI entity/ListState 作为文档真相。

---

## 5. 实施顺序

1. 先修 GUI `InsertChar` 不重置 caret。
2. 补 runtime string insertion / selection replacement 回归测试。
3. 跑 `runtime::document_runtime`、`gui::input`、`gui::app`、`cargo check`。
