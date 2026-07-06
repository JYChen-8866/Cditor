# ding-note2 IME / 文本输入完整迁移分析

> 目标：不是继续打补丁，而是把 `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor` 的 IME、普通字符输入、selection、caret、marked range 机制完整分析清楚，再决定 V2 应该如何迁移。
>
> 当前用户反馈：V2 打字仍然跳到最后。说明仅修 `GuiInputCommand::InsertChar` 前的 `focus_block()` 不够，必须重新审视 V2 输入架构是否和 ding-note2 一致。
>
> 架构边界：继续遵守 `doc/large-document-rich-text-architecture.md`。V2 不能把 10w 文档真相交给 GPUI entity/ListState；但是**当前编辑 block 的 IME 输入状态必须像 ding-note2 一样精确、稳定、单一来源**。

---

## 1. 结论先行

### 1.1 ding-note2 的核心模型

ding-note2 不是“GUI command 自己猜位置再插入字符”。它的核心是：

```text
每个 Block 是自己的 EntityInputHandler
每个 Block 持有自己的 focus_handle
每个 Block 内部持有 selected_range / marked_range / selection_reversed
window.handle_input(&block_focus_handle, ElementInputHandler::new(text_bounds, block_entity), cx)
GPUI 平台输入只和当前 focused block 的 EntityInputHandler 对话
```

普通文字输入、IME composition、候选框位置、platform selected range 都围绕同一个 block runtime 状态工作。

### 1.2 V2 当前最大风险点

V2 当前不是 per-block input handler，而是：

```text
Root CditorV2View 实现 EntityInputHandler
RichTextElement.paint() 中用 root focus 调 window.handle_input(... view.clone())
Root View 再根据 runtime.focused_block_id() 找当前 block
```

这不是 ding-note2 的 1:1 模型。它可以工作，但必须保证：

1. root focus 不会在 render 中无条件抢焦点并触发平台选区重查。
2. `focused_block_id` 不会在输入周期中被鼠标、fallback、payload window、render 重置。
3. `selected_text_range()` 永远能返回当前 caret 的 collapsed range。
4. `replace_text_in_range(None, text)` 必须使用**同一个** runtime caret/selection/marked range。
5. layout cache / bounds / character_index_for_point 必须对应当前 focused block 和 content_version。

只要其中一个断，就可能出现“GPUI 认为选区在末尾 / V2 runtime 认为 caret 在中间 / 输入提交时又 fallback 到末尾”。

### 1.3 当前已确认的背离点

| 编号 | 背离点 | ding-note2 | V2 当前/曾经 | 风险 |
|---|---|---|---|---|
| D-001 | 输入 handler 粒度 | Block 自己实现 `EntityInputHandler` | Root `CditorV2View` 实现 input handler | 平台输入 range 查询和实际 block 解耦 |
| D-002 | focus handle 粒度 | 每个 block 自己的 `focus_handle` | root 共享一个 `FocusHandle` | 多 block 场景下 input session 不知道真正 block |
| D-003 | 普通字符 keydown | 不在 keydown 插入普通字符，只处理 Tab | V2 曾经 keydown 映射 `InsertChar` | 双输入通道导致 caret/selection 冲突 |
| D-004 | selected_range 真相 | block 内 `selected_range` 始终存在，collapsed 即 caret | V2 有 `editing.caret_anchor` + `focused_text_selection` + `document_selection` | 需要合成，容易顺序错 |
| D-005 | marked_range 真相 | block 内 `marked_range` 直接参与替换 | V2 composition 存在 `EditingSession`，再投影成 preview text | base text / preview text range 映射复杂 |
| D-006 | render focus | focused block 才注册 input | V2 root render 中 `if !focus.is_focused { window.focus(...) }` | render 可能主动抢焦点 |
| D-007 | `replace_and_mark_text_in_range(None, ...)` fallback | `range_utf16 -> marked_range -> selected_range` | V2 fallback 到 `editing.caret_anchor` | 若 caret 已被重置，则 composition 从末尾开始 |
| D-008 | `text_for_range`/`selected_text_range` 对象 | 当前 block 的 display text | root 根据 focused block 推断 | focused block 错即全错 |
| D-009 | 输入 bounds | `handle_input` bounds 是当前 block text bounds | V2 使用 `RichTextElement` bounds，但 handler 是 root | platform point/range 查询需再通过 focused layout cache |

---

## 2. ding-note2 详细链路

### 2.1 `EntityInputHandler`：输入范围优先级

参考文件：

- `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/input.rs`

#### 普通输入 `replace_text_in_range`

核心逻辑：

```rust
let visible_range = range_utf16
    .as_ref()
    .map(|range| self.range_from_utf16(range))
    .or(self.marked_range.clone())
    .unwrap_or(self.selected_range.clone());
self.replace_text_in_visible_range(visible_range, new_text, None, false, cx);
```

语义：

```text
explicit UTF16 range
  -> marked_range
  -> selected_range
```

注意：`selected_range` 即使 collapsed 也存在，所以 caret 就是 `selected_range.start..selected_range.end`。

#### IME `replace_and_mark_text_in_range`

核心逻辑：

```rust
let visible_range = range_utf16
    .as_ref()
    .map(|range| self.range_from_utf16(range))
    .or(self.marked_range.clone())
    .unwrap_or(self.selected_range.clone());
let selected_range_relative = new_selected_range_utf16
    .as_ref()
    .map(|range_utf16| Self::utf16_range_to_utf8_in(new_text, range_utf16));
self.replace_text_in_visible_range(
    visible_range,
    new_text,
    selected_range_relative,
    !new_text.is_empty(),
    cx,
);
```

语义：

```text
composition 更新时优先替换已有 marked_range
如果没有 marked_range，替换 selected_range/caret
new_selected_range 是 inserted text 内的相对 range
```

### 2.2 `selected_text_range()` 永远返回 block 内 selected_range

```rust
Some(UTF16Selection {
    range: self.range_to_utf16(&self.selected_range),
    reversed: self.selection_reversed,
})
```

这点非常关键：ding-note2 不需要从多个 runtime 状态合成 caret。它的 `selected_range` 永远是当前平台输入的 selection truth。

### 2.3 `marked_text_range()` 直接返回 block marked_range

```rust
self.marked_range
    .as_ref()
    .map(|range| self.range_to_utf16(range))
```

IME 平台层每次问 marked range，拿到的就是 block 当前 marked range。

### 2.4 `replace_text_in_visible_range` 如何更新 caret

参考文件：

- `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/runtime/mod.rs`

核心逻辑：

```rust
let inserted_range = clean_range.start..clean_range.start + new_text.len();
let marked_range = if mark_inserted_text && !new_text.is_empty() {
    Some(result.map_range(&inserted_range))
} else {
    None
};
let selected_range = selected_range_relative.as_ref().map(|relative| {
    let absolute = clean_range.start + relative.start..clean_range.start + relative.end;
    result.map_range(&absolute)
});
let cursor = selected_range
    .as_ref()
    .map(|range| range.end)
    .unwrap_or_else(|| result.map_offset(clean_range.start + new_text.len()));

self.apply_title_edit(
    result.tree,
    cursor,
    marked_range,
    selected_range.clone(),
    ...,
    cx,
);
```

语义：

1. 插入文本后，cursor 默认在 inserted end。
2. 如果 IME 给了 selected subrange，则 cursor 在 subrange end。
3. marked range 是插入文本映射后的真实 range。
4. 不会调用“focus block 到末尾”。

### 2.5 `cursor_offset()` 只依赖 selected_range

```rust
pub fn cursor_offset(&self) -> usize {
    if self.selection_reversed {
        self.selected_range.start
    } else {
        self.selected_range.end
    }
}
```

这说明 ding-note2 没有单独的 `editing.caret_anchor.text_offset` 和 selection 分叉。caret 是 selected_range collapsed 的特例。

### 2.6 `window.handle_input` 的注册位置

参考文件：

- `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/element.rs`

核心逻辑：

```rust
if focus_handle.is_focused(window) {
    let text_bounds = source_text_bounds(bounds, prepaint.source_line_number_gutter_width);
    window.handle_input(
        &focus_handle,
        ElementInputHandler::new(text_bounds, self.input.clone()),
        cx,
    );
}
```

要点：

1. 只有当前 block focus handle focused 时才注册 input。
2. handler 对象是 block 自己，不是 root editor。
3. bounds 是当前 block 的 text bounds。
4. `character_index_for_point`、`bounds_for_range` 都在 block 上直接用自己的 layout。

### 2.7 `on_key_down` 不处理普通字符

参考文件：

- `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/interactions.rs`

```rust
pub(crate) fn on_block_key_down(...) {
    if event.keystroke.key != "tab" {
        return;
    }
    ...
}
```

普通字符不从 keydown 插入。文字输入统一走平台输入/IME。

---

## 3. V2 当前链路

### 3.1 `RichTextElement.paint()` 注册 input

文件：`src/gui/text/element.rs`

```rust
if self.input_handler.focused {
    window.handle_input(
        &self.input_handler.focus,
        ElementInputHandler::new(bounds, self.input_handler.view.clone()),
        cx,
    );
}
```

差异：

| 项 | ding-note2 | V2 |
|---|---|---|
| focus handle | block focus handle | root view focus handle |
| handler entity | block entity | root `CditorV2View` |
| state owner | block.selected_range / marked_range | root runtime 合成 |
| bounds | block text bounds | rich text element bounds，但 handler 内再找 focused block cache |

### 3.2 `CditorV2View` 实现 `EntityInputHandler`

文件：`src/gui/app/cditor_v2_view.rs`

```rust
fn selected_text_range(...) -> Option<UTF16Selection> {
    let runtime = self.ready_runtime()?;
    platform_selected_text_range(runtime)
}
```

`platform_selected_text_range` 当前从这些状态合成：

```text
active_composition_selected_range
  -> active_composition_marked_range end
  -> focused_text_selection_range
  -> editing.caret_anchor.text_offset
```

这比 ding-note2 复杂很多。任何一个状态在输入周期里不一致，就会给 GPUI 错的 UTF16 selection。

### 3.3 `replace_text_in_range`

```rust
let range = ime_replacement_range(runtime, range_utf16);
runtime.replace_text_in_focused_range(range, text)
```

如果 GPUI 给 explicit range：走 `ime_replacement_range`。
如果 GPUI 不给 range：交给 runtime 用 composition/selection/caret fallback。

### 3.4 `replace_and_mark_text_in_range`

```rust
let range = ime_replacement_range(runtime, range_utf16).unwrap_or_else(|| {
    let caret = runtime.editing.as_ref()
        .map(|editing| editing.caret_anchor.text_offset as usize)
        .unwrap_or_else(|| runtime.focused_text().map(str::len).unwrap_or(0));
    caret..caret
});
runtime.begin_or_update_composition_with_selection(block_id, range, new_text, selected_range)
```

这里是一个高风险点：ding-note2 的 fallback 是 `marked_range -> selected_range`，V2 当前 fallback 是 `caret_anchor`。如果 `caret_anchor` 已经被任何路径重置到末尾，IME composition 就从末尾开始。

### 3.5 root render 中主动 focus

文件：`src/gui/app/cditor_v2_view.rs`

```rust
if !focus.is_focused(window) {
    window.focus(&focus, cx);
}
```

这和 ding-note2 不同。ding-note2 是 block 获得 focus 后，该 block 注册 input；V2 是 root 每次 render 保证 root focused。潜在风险：

1. 鼠标点击中间后，root focus 变化触发平台 input session 重建。
2. 平台随后调用 `selected_text_range()`，如果 runtime caret 尚未完成更新或 focused block 不对，就可能得到末尾。
3. 多个 focused block / projection 重建时，root focus 无法区分具体 block。

---

## 4. 为什么“打字跳到最后”仍可能发生

下面按可能性排序。

### 4.1 高概率：IME fallback 使用了被重置后的 `editing.caret_anchor`

触发链：

```text
点击文本中间
  -> mouse down 计算 offset
  -> runtime.set_caret_offset(block, offset)
  -> render/root focus/input session 触发 GPUI 查询
  -> replace_and_mark_text_in_range(None, text, ...)
  -> V2 fallback 到 editing.caret_anchor
```

如果中间任何路径调用 `focus_block(block_id)`，caret_anchor 会变成 text_len。

当前仍可能调用 `focus_block` 的路径：

| 路径 | 文件 | 风险 |
|---|---|---|
| `focus_block_from_gui_at_position` 无 offset 时 | `cditor_v2_view.rs` | hit-test 失败时，如果 block 未 focused，直接末尾 |
| gutter click | `cditor_v2_view.rs` | gutter focus 到末尾 |
| `set_document_text_selection` focus block 不同时 | `document_runtime.rs` | 跨 block/focus block 时先末尾再设 focus offset |
| `begin_or_update_composition_with_selection` block 不同时 | `document_runtime.rs` | IME 进来时若 focused_block 不对，先末尾 |
| `ensure_runtime_focus_for_insert_char` | `cditor_v2_view.rs` | 普通字符 keydown 已应停用，但代码还在 |

### 4.2 高概率：V2 不是 per-block `EntityInputHandler`

因为 handler 是 root view，GPUI 的 input session 只知道 root focus。它调用：

- `selected_text_range()`
- `marked_text_range()`
- `text_for_range()`
- `bounds_for_range()`
- `character_index_for_point()`

这些函数都要依赖 `runtime.focused_block_id()`。如果 focused block 和用户实际点击/输入 block 有一帧不一致，就会拿错 text/range。ding-note2 没有这个问题，因为当前 block 自己就是 handler。

### 4.3 中概率：`handle_input` bounds 与 handler focused block 不一致

V2 每个 focused rich text element 注册：

```rust
ElementInputHandler::new(bounds, view.clone())
```

但是 root handler 的 `character_index_for_point` 又去找：

```rust
runtime.focused_text_for_platform_input()
current_text_layout_cache(runtime, block_id)
```

如果 bounds 来自 A block，而 `focused_block_id` 是 B block，平台 range/point 查询就错。

### 4.4 中概率：点击 hit-test 失败导致 focus 到末尾

`focus_block_from_gui_at_position`：

```rust
let offset = position.and_then(|position| self.text_offset_for_block_at_position(block_id, position));
if let Some(offset) = offset {
    runtime.focus_block_at_offset(block_id, offset);
} else {
    if runtime.focused_block_id() != Some(block_id) {
        runtime.focus_block(block_id); // 到末尾
    }
}
```

如果 layout cache 过期、block rect 没建好、fallback origin 算错，`offset` 为 `None`，新 focused block 就直接到末尾。

### 4.5 中概率：markdown shortcut / inline parser 改写后 caret 映射不完整

ding-note2 的 `replace_text_in_visible_range` 在 rich tree reparse 后通过 `result.map_offset(...)` 映射 cursor。

V2 当前 `replace_text_in_focused_range`：

```rust
editing.caret_anchor.text_offset = inserted.end as u64;
sync_payload_from_model_after_replace(...);
self.apply_inline_markdown_shortcut(block_id)?;
```

如果 `apply_inline_markdown_shortcut` 改写 payload/text 后没有同步修正 caret，就可能出现 caret 与渲染文本不同步。这个主要影响 markdown shortcut，不一定是普通打字跳尾的主因，但必须纳入迁移。

---

## 5. V2 应该迁移成什么样

### 5.1 最接近 ding-note2 的目标结构

在保持 V2 大文档 runtime truth 的前提下，新增一个“当前编辑 block 输入会话”层，而不是直接让 root view 模拟 block handler。

```text
DocumentRuntime
  ├─ document/index/payload/layout/scroll truth
  └─ EditingInputSession
       ├─ block_id
       ├─ selected_range: Range<usize>
       ├─ selection_reversed: bool
       ├─ marked_range: Option<Range<usize>>
       ├─ marked_text: Option<String> / composition state
       ├─ cursor_offset() = selected_range start/end
       └─ content_version
```

关键：**collapsed caret 也必须表现为 selected_range**，不要让平台输入在 caret 和 selection 两套状态之间来回猜。

### 5.2 root handler 可保留，但语义必须变成“当前 EditingInputSession handler”

如果暂时不做 per-block entity，也必须让 root `EntityInputHandler` 完全代理当前 editing session：

```text
selected_text_range() 只返回 EditingInputSession.selected_range
marked_text_range()   只返回 EditingInputSession.marked_range
replace_text_in_range(None) fallback = marked_range -> selected_range
replace_and_mark_text_in_range(None) fallback = marked_range -> selected_range
```

不能在 IME fallback 中直接读 `editing.caret_anchor`；`caret_anchor` 应该由 `selected_range` 派生或同步。

### 5.3 更完整的 1:1 方案：BlockInputProxy

新增一个轻量 proxy，不作为 10w 文档真相，只作为当前 focused block 的 GPUI input adapter：

```text
RichTextElement(focused block)
  -> window.handle_input(&block_input_focus, ElementInputHandler::new(text_bounds, BlockInputProxy))
BlockInputProxy
  -> 持有 view entity + block_id
  -> EntityInputHandler 每次先校验 block_id == runtime.focused_block_id()
  -> 读写 runtime.editing_input_session
```

这样可以对齐 ding-note2 的 per-block handler，同时不破坏 V2 “runtime 是文档真相”的架构。

---

## 6. 迁移任务清单

### A. 先补观察与复现，不再盲改

- [x] A-001 增加输入链路 trace 开关：记录 `selected_text_range / marked_text_range / replace_text_in_range / replace_and_mark_text_in_range / focus_block / set_caret_offset`。
- [x] A-002 trace 字段必须包含：block_id、text_len、caret_anchor、selected_range、marked_range、range_utf16、converted_utf8_range、new_text、content_version。
- [ ] A-003 做一个最小复现脚本/手动步骤文档：点击 `abcdef` 的 `c|d` 中间，输入 `X`，记录每一步 trace。
- [x] A-004 明确到底是哪一步把 caret 变成 text_len：trace 确认不是 block 错位，而是 `replace_and_mark_text_in_range(None, ...)` 在 active composition 时 fallback 到 preview caret，导致 base range 从 marked base range 漂移到末尾/错误字符边界。

### B. 对齐 ding-note2 的 input state 模型

- [ ] B-001 在 runtime 引入或重构 `EditingInputSession`，显式保存 `selected_range`、`selection_reversed`、`marked_range`。
- [ ] B-002 collapsed caret 统一表示为 `selected_range = offset..offset`。
- [ ] B-003 `editing.caret_anchor.text_offset` 只作为 scroll/caret geometry anchor，不作为平台输入 fallback 的唯一 truth。
- [ ] B-004 `set_caret_offset` 同步设置 `selected_range = offset..offset`。
- [ ] B-005 `set_document_text_selection` 同步设置 session selection，包括 reversed。
- [ ] B-006 `move_caret_left/right/up/down` 同步更新 session selected_range。
- [ ] B-007 `delete/enter/tab` 后同步 session selected_range。

### C. 对齐 `EntityInputHandler` 语义

- [ ] C-001 `selected_text_range()` 只从当前 editing input session 输出 UTF16 selection。
- [ ] C-002 `marked_text_range()` 只从 session marked_range 输出 UTF16 range。
- [ ] C-003 `replace_text_in_range` 的 fallback 改为：`explicit UTF16 range -> marked_range -> selected_range`。
- [x] C-004 `replace_and_mark_text_in_range` 的 fallback 改为：`explicit UTF16 range -> active composition base/marked range -> selected_range -> caret`。
- [ ] C-005 `new_selected_range_utf16` 按 ding-note2 转为 inserted text 内 UTF8 relative range。
- [ ] C-006 替换后 cursor = selected subrange end，否则 inserted end。
- [ ] C-007 composition update 后 session marked_range = inserted mapped range。
- [ ] C-008 commit composition 后 clear marked_range，selected_range collapse 到 inserted end。
- [ ] C-009 cancel/unmark 后 clear marked_range，但 selected_range 保持合理 caret。

### D. 修 root handler / per-block handler 架构背离

- [ ] D-001 评估并选择：保留 root handler 但严格代理 session，还是新增 `BlockInputProxy`。
- [ ] D-002 推荐实现 `BlockInputProxy { view, block_id }`，让 `handle_input` 至少携带 block_id。
- [ ] D-003 `handle_input` 注册时使用当前 block 的 text bounds，并在 handler 中校验 block_id。
- [ ] D-004 如果 block_id 与 runtime.focused_block_id 不一致，拒绝输入或先安全同步，不允许 fallback 到末尾。
- [ ] D-005 root render 不应无条件 `window.focus(&root_focus)` 干扰 block input session；需要改为用户交互后 focus，或 block input proxy focus。
- [ ] D-006 同一帧只允许一个 focused block 注册 input handler。

### E. 点击/命中测试稳定性

- [ ] E-001 `focus_block_from_gui_at_position` 在 hit-test 失败时不能默认 focus 到末尾；应该保持旧 caret 或 fallback 到最近估算 offset。
- [ ] E-002 layout cache 过期时 fallback 必须可用，不能返回 `None`。
- [ ] E-003 fallback origin 要覆盖 list/heading/code/quote padding 与 gutter，不得错位。
- [ ] E-004 增加点击 `abcdef` 每个字符位置的 offset 单测/集成测试。
- [ ] E-005 增加 code block 内点击 offset 测试。
- [ ] E-006 增加滚动后点击 offset 测试，验证 global_scroll_top 参与正确。

### F. markdown shortcut / rich text reparse 后 caret 映射

- [ ] F-001 `apply_inline_markdown_shortcut` 如果改变 visible text，需要返回 caret 映射结果。
- [ ] F-002 对齐 ding-note2 `result.map_offset(...)` 思路，不能简单保留旧 inserted.end。
- [ ] F-003 增加 `**abc**`、`[x](url)`、inline code、删除 delimiter 后 caret 位置测试。

### G. 测试矩阵

- [ ] G-001 普通英文：点击 `ab|cd` 输入 `X` => `abXcd`，caret `abX|cd`。
- [ ] G-002 中文 IME：点击 `ab|cd` composition `你` preview => `ab你cd`，commit 后 caret `ab你|cd`。
- [ ] G-003 日文 IME：composition 多阶段 update 不跳尾。
- [ ] G-004 emoji/surrogate pair：UTF16 range 不拆 emoji。
- [ ] G-005 有 selection：选中 `bc` 输入 `X` => `aXd`。
- [ ] G-006 有 marked_range：composition update 替换 marked，而不是 selected/caret。
- [ ] G-007 点击后立即输入，第一字符不跳尾。
- [ ] G-008 连续快速输入，后续字符不替换前一个字符。
- [ ] G-009 鼠标拖选后输入，替换 selection。
- [ ] G-010 滚动后点击中间输入，不跳尾。
- [ ] G-011 code block 输入、composition、点击 offset。
- [ ] G-012 list item 输入、composition、点击 offset。

---

## 7. Trace 使用方法

已加入环境变量开关，默认关闭，不影响正常性能。

```sh
CDITOR_TRACE_INPUT=1 cargo run --example minimal_postgres_editor
```

或者运行当前 GUI example 时加同样环境变量。开启后重点看三类日志：

```text
[cditor][input][text][handle_input]
[cditor][input][gui][selected_text_range]
[cditor][input][gui][replace_and_mark_text_in_range]
[cditor][input][gui][replace_text_in_range]
[cditor][input][runtime][focus_block]
[cditor][input][runtime][set_caret_offset]
[cditor][input][runtime][replace_text_in_focused_range.range]
[cditor][input][runtime][replace_text_in_focused_range.end]
```

### 7.1 判断跳尾原因的读法

#### 情况 1：点击后立刻出现 `focus_block caret_to_text_len`

```text
[runtime][set_caret_offset] block=1 clamped_offset=3 ...
[runtime][focus_block] previous_focus=Some(1) next_block=1 caret_to_text_len=6
```

说明某条路径在点击之后又调用了 `focus_block`，把 caret 重置到末尾。

#### 情况 2：`selected_text_range` 返回末尾

```text
[gui][selected_text_range] focused=Some(1) selection=Some(UTF16Selection { range: 6..6, ... })
```

而前面点击 offset 是 3，说明平台输入查询时 runtime selection/caret 已经错了。

#### 情况 3：`replace_and_mark_text_in_range(None)` fallback 到末尾

```text
[gui][replace_and_mark_text_in_range] range_utf16=None range_from_ime=None resolved_utf8=6..6
```

说明 GPUI 没给 explicit range，V2 fallback 使用了错误 caret。

#### 情况 4：`handle_input` 注册 block 与 focused block 不一致

```text
[text][handle_input] block=2 ...
[gui][selected_text_range] focused=Some(1) ...
```

说明 root handler 与 block input bounds 解耦，必须上 `BlockInputProxy` 或 per-block session 校验。

### 7.2 本次 trace 的已确认根因

用户提供的 trace 中出现了决定性证据：

```text
[gui][replace_and_mark_text_in_range] block=1 range_utf16=None range_from_ime=None resolved_utf8=28..28 new_text_len=3
[runtime][begin_or_update_composition] block=1 requested_range=28..28 clamped_range=27..27 preview_len=3
```

以及后续：

```text
[text][handle_input] block=1 ... caret=Some(43) marked=Some(42..43)
[gui][replace_and_mark_text_in_range] block=1 range_utf16=None range_from_ime=None resolved_utf8=43..43 new_text_len=2
[runtime][begin_or_update_composition] block=1 requested_range=43..43 clamped_range=42..45
```

这说明：

1. `handle_input` block 和 focused block 都是 1，不是 block 错位。
2. `marked_text_range` 能返回 marked range，说明 composition 状态存在。
3. 但是 GPUI 调用 `replace_and_mark_text_in_range(None, ...)` 时，V2 fallback 用了 `caret..caret`。
4. 这个 caret 是 preview text 中的 caret，不是 base document text 的 marked range。
5. runtime 再把 preview caret range 放到 base text 上做 `safe_char_range`，于是 range 被 clamp/snap 到错误字符边界，导致 IME update 越来越偏。

修复：`replace_and_mark_text_in_range(None, ...)` fallback 改成：

```text
active composition base range -> focused selected range -> caret
```

也就是已有 composition 时永远复用 base `composition.range_start..composition.range_end`，对齐 ding-note2 的 `marked_range -> selected_range` 规则。

---

## 8. 推荐实施顺序

1. **先加 trace，不改行为**：确认真实跳尾点。
2. **把 runtime input state 收敛为 selected_range/marked_range 模型**：对齐 ding-note2 的状态真相。
3. **修 EntityInputHandler fallback 顺序**：严格 `range_utf16 -> marked -> selected`。
4. **修点击 hit-test 失败 fallback**：不允许 `None -> focus_block(end)`。
5. **再考虑 BlockInputProxy**：如果 root handler 仍有一帧错位，就必须引入 per-block proxy。
6. **补完整测试矩阵**。
7. **最后删掉过渡性的 `GuiInputCommand::InsertChar` 普通输入路径**或保留但只作为非平台输入 fallback，默认不触发。

---

## 9. 当前必须停止的错误方向

- 不要再用 `focus_block(block_id)` 修输入问题，它语义就是到末尾。
- 不要让 `replace_and_mark_text_in_range(None, ...)` fallback 到 `text_len`。
- 不要让 root render 无条件抢焦点影响 block input session。
- 不要只测 runtime `insert_char`，这绕过了 GPUI IME 的真实路径。
- 不要只测单字符 command path，真实问题在 platform input + selection/caret 同步。
- 不要把当前 block input 状态散落在 `caret_anchor`、`focused_text_selection`、`document_selection`、`composition` 四套状态里却没有统一投影。

---

## 10. 当前代码状态备注

本轮分析前已经做过两类改动：

1. `GuiInputCommand::InsertChar` 不再每次 `focus_block`。
2. 普通字符 keydown 已改为 `Ignore`，意图让文字走 GPUI input handler。
3. runtime replacement 优先级已局部调整为 composition 优先于 selection。

但这些仍然不足以宣称完成，因为 V2 的根结构还不是 ding-note2 的 per-block input state 模型。下一步必须按本文 A 组先加 trace，用证据定位跳尾点，再迁移 B/C/D。