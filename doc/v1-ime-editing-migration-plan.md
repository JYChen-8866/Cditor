# V1 IME / 编辑输入能力迁移分析与任务清单

> 目标：把 `/Users/jychen/Desktop/Cditor` V1/editor2 的文本输入、IME composition、点击定位、选区替换能力迁移到 CDitor-V2，并解释“为什么不能在字符中间插入字符”。
>
> 架构约束：继续遵守 `doc/large-document-rich-text-architecture.md`。runtime 是文本/selection/IME 状态真相；GUI text element 只负责 hit-test、layout cache、paint、`window.handle_input`。不能恢复 root IME bridge，不能让 GPUI entity 成为文档真相。

---

## 1. V1 参考源码

| 能力 | V1 文件 | 关键实现 |
|---|---|---|
| 点击定位 caret | `/Users/jychen/Desktop/Cditor/src/editor2/block/render.rs` | mouse down 调 `offset_for_point(event.position)`，再 `set_cursor_offset(offset)` |
| point -> text offset | `/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs` | `offset_for_point` 使用 block `layout_cache.bounds/lines/line_height` + `text_element::index_for_mouse_position` |
| 平台输入 handler | `/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs` | block entity 实现 `EntityInputHandler` |
| text_for_range | 同上 | UTF16 range -> UTF8 range -> clamp char boundary |
| selected_text_range | 同上 | 返回 `UTF16Selection`，使用当前 `selected_range` |
| marked_text_range | 同上 | 返回 IME marked range，table cell 也支持 |
| replace_text_in_range | 同上 | 使用 platform range / marked_range / selected_range / caret 替换 |
| replace_and_mark_text_in_range | 同上 | 替换后设置 `marked_range`，并按 `new_selected_range_utf16` 设置 marked text 内部 selection |
| table cell IME | 同上 | table focused cell 有独立 replacement range / marked range |

V1 关键语义：

```rust
// mouse down
let offset = this.offset_for_point(event.position);
if event.modifiers.shift {
    this.extend_selection_to_offset(offset);
} else {
    this.set_cursor_offset(offset);
}
```

```rust
// platform input
let range = self.current_replacement_range(range_utf16);
self.replace_text_range(range, new_text);
```

`current_replacement_range` 优先级：

```text
platform range_utf16
  -> marked_range
  -> selected_range
```

普通输入没有 range 时，`selected_range` 如果是 collapsed，就是当前 caret。

---

## 2. V2 当前链路

| 层 | V2 文件 | 当前实现 |
|---|---|---|
| text element | `src/gui/text/element.rs` | focused 时在 `Element::paint()` 调 `window.handle_input`，符合当前架构约束 |
| text layout cache | `src/gui/text/element.rs` -> `CditorV2View::update_text_layout_cache` | 每次 paint 后回写 `RichTextPlatformLayout` 到 view cache |
| mouse focus | `src/gui/input/mouse.rs` -> `focus_block_from_gui_at_position` | 用 `text_layouts[block_id]` + `platform_index_for_point` 算 offset |
| platform input | `src/gui/app/cditor_v2_view.rs` `EntityInputHandler` | 实现了 `text_for_range/selected_text_range/marked_text_range/replace_text_in_range/replace_and_mark_text_in_range/bounds_for_range/character_index_for_point` |
| runtime replace | `DocumentRuntime::replace_text_in_focused_range` | 支持 explicit range、selection、composition range、caret |
| runtime composition | `begin_or_update_composition/commit_composition/cancel_composition` | 有基础 composition preview/commit/cancel |

---

## 3. 为什么之前不能在字符中间插入字符

根因在 GUI 点击定位 fallback：

```rust
let offset = position.and_then(|p| self.text_offset_for_block_at_position(block_id, p));
if let Some(offset) = offset {
    runtime.focus_block_at_offset(block_id, offset);
} else {
    runtime.focus_block(block_id); // 这里会把 caret 放到文本末尾
}
```

`text_offset_for_block_at_position` 依赖 `text_layouts`：

```rust
self.text_layouts
    .get(&block_id)
    .map(|cache| platform_index_for_point(cache, position))
```

当 layout cache 缺失、还没 paint、刚滚动/刚加载、或 cache 空窗时，点击字符中间拿不到 offset，于是 fallback 到 `runtime.focus_block(block_id)`。而 `focus_block` 默认：

```rust
caret = text_len
```

所以用户感觉是：点中间也不能插，输入总跑到末尾。

已修复：

- 若 hit-test cache 缺失且 block 已 focused，不再调用 `focus_block()` 重置 caret 到末尾。
- runtime 层新增测试证明：
  - `insert_char` 使用 middle caret。
  - `replace_text_in_focused_range(None, text)` 可在 caret 中间插入。
  - IME preview + commit 可从文本中间开始。

---

## 4. V1 vs V2 能力对照表

| 能力 | V1 状态 | V2 当前状态 | 缺口 | 优先级 |
|---|---|---|---|---|
| `window.handle_input` 接入 | block text element paint 中接入 | 已在 `RichTextGpuiElement::paint()` 接入 | 已完成 | P0 ✅ |
| 点击字符定位 caret | V1 使用 block layout cache，失败时默认 len | V2 使用 view text_layout cache，并校验 cache content_version | 已修复 cache miss 时不重置已 focused caret；已过滤 stale cache；有同窗口参考 cache 时启用当前 clicked block 轻量 fallback | P0 部分完成 |
| 字符中间插入 | V1 `selected_range` collapsed at caret | V2 runtime 支持，新增测试通过；GUI hit-test 不再使用 stale cache | 已有局部 fallback；首次窗口完全无 cache 时仍保持安全不重置 caret | P0 部分完成 |
| 平台 `text_for_range` | UTF16->UTF8 + clamp | V2 已实现 | 需要更多 emoji/CJK 测试覆盖 GUI handler | P1 |
| `selected_text_range` | 返回真实 selection/caret | V2 已实现 focused block selection/caret | reversed selection 未完整保留；跨 block 输入 handler 只面向 focused block | P1 |
| `marked_text_range` | 支持 block/table cell marked range | V2 支持 active composition marked range | table cell IME 未完整 | P2 |
| `replace_text_in_range` | range/marked/selection/caret 替换 | V2 已实现 | 要补更多 platform range UTF16 边界测试 | P1 |
| `replace_and_mark_text_in_range` | 替换并设置 marked range + marked 内 selected range | V2 已把 `new_selected_range` 转成 UTF-8 相对范围并存入 runtime composition | 已完成；候选框 range 优先跟随 composition selected subrange | P0/P1 ✅ |
| IME composition preview | V1 直接替换显示并 underline marked | V2 active composition preview + marked range underline + selected subrange | 已完成基础链路 | P0/P1 ✅ |
| IME commit | V1 Changed + save | V2 `commit_composition` + `EditTransactionKind::CompositionCommit` | 已完成基础事务边界 | P1 ✅ |
| IME cancel/unmark | V1 清 marked_range | V2 `cancel_composition` 恢复 before text/selection | 已完成 runtime 语义 | P1 ✅ |
| 候选框 bounds | V1 cursor/range bounds | V2 `selected_text_range` 优先返回 composition selected subrange，`bounds_for_range` 用该 range 算 bounds | 基础完成；cache miss fallback 仍需增强 | P1 ✅ |
| `character_index_for_point` | V1 index_for_mouse_position | V2 `platform_index_for_point` | 基础完成；hard line/wrapped line/CJK 测试不足 | P1 |
| 拖选文本 | V1 drag anchor/focus offset | V2 有 `GuiTextDragSelection` + `DocumentSelection` | 跨未加载 block / selection painting 还要补 | P1 |
| table cell IME | V1 table runtime 独立支持 | V2 尚未完整 table cell editor | 缺 | P2 |
| code language query input | V1 IME/text input 可推入 query | V2 toolbar 目前偏静态 | 缺 | P2 |

---

## 5. 已完成项

- [x] I-001 V2 text element 在 `Element::paint()` 中调用 `window.handle_input`，没有恢复 root IME bridge。
- [x] I-002 `EntityInputHandler::text_for_range` 支持 UTF16 -> UTF8 range。
- [x] I-003 `selected_text_range` 返回 focused text selection/caret。
- [x] I-004 `marked_text_range` 返回 active composition marked range。
- [x] I-005 `replace_text_in_range` 通过 runtime 替换 explicit range / selection / composition / caret。
- [x] I-006 `bounds_for_range` 返回 IME candidate rect 所需 bounds。
- [x] I-007 `character_index_for_point` 接入 GPUI hit-test。
- [x] I-008 修复 layout cache miss 时点击 focused block 不再把 caret 重置到末尾。
- [x] I-009 添加 `insert_char_uses_middle_caret_offset` 测试。
- [x] I-010 添加 `replace_text_in_focused_range_can_insert_in_middle_without_selection` 测试。
- [x] I-011 添加 `ime_preview_and_commit_can_start_in_middle_of_text` 测试。

---

## 6. 待补任务清单

### A. 点击定位 / 中间插入完整化

- [x] A-001 修复 cache miss 时 `focus_block()` 重置 caret 到末尾的问题。
- [x] A-002 `text_offset_for_block_at_position` 校验 cache content_version，避免用旧 cache。
- [x] A-003 cache miss 时构造轻量 fallback hit-test：用当前 block payload + width + style 做同步轻量估算，只限当前 clicked block。
- [x] A-004 点击 code block padding/toolbar/content 时坐标要映射到 code content 内，不误判到末尾。
- [ ] A-005 点击空 block 应 caret=0，不应 len/fallback 混乱。
- [ ] A-006 测试：首次点击可见 paragraph 中间后输入插入中间。
- [ ] A-007 测试：滚动后点击新 projection block 中间不插入末尾。
- [ ] A-008 测试：CJK/emoji 点击位置不落入非法 UTF8 边界。

### B. IME selected subrange / marked range 完整迁移

- [x] B-001 扩展 runtime composition state，保存 marked text 内 `selected_range` 或 caret-in-marked。
- [x] B-002 `replace_and_mark_text_in_range` 使用 `new_selected_range`，对齐 V1 2199-2207 行逻辑。
- [x] B-003 `selected_text_range` 在 active composition 时优先返回 marked 内 selected subrange，而不是一律 range.end caret。
- [x] B-004 marked underline 仍绘制整个 marked range；custom caret 在 composition active 时隐藏。
- [x] B-005 candidate rect 跟随 selected subrange/caret，而不是固定 marked end。
- [x] B-006 测试：拼音组合过程中 selected subrange 在 marked 文本中间。
- [x] B-007 测试：emoji/CJK composition 不拆 surrogate pair。

### C. IME cancel / commit 事务化

- [x] C-001 `begin_or_update_composition` 保存 before_text/before_selection，cancel 时恢复 preview 前文本。
- [x] C-002 `commit_composition` 记录 `EditTransactionKind::CompositionCommit`，和普通 typing undo 分组隔离。
- [x] C-003 composition commit 后 Postgres save 走 debounce，不阻塞 IME hot path。
- [x] C-004 stale composition content_version 防护。
- [x] C-005 测试：composition cancel 恢复原文。
- [x] C-006 测试：composition commit 是独立 undo step。

### D. Selection / 输入替换

- [ ] D-001 reversed selection 保留并返回给 `UTF16Selection.reversed`。
- [ ] D-002 Shift+点击扩展 selection 到 hit-test offset，对齐 V1 render.rs。
- [ ] D-003 鼠标拖选跨 block 时，focused block input handler 的 selected range 与 document selection 一致。
- [ ] D-004 `replace_text_in_range` 对跨 block selection 应走 `delete_document_selection + insert`，不是只看 focused block。
- [ ] D-005 测试：选中中间字符后输入替换。
- [ ] D-006 测试：跨 block selection 后输入替换为单段文本。

### E. Table / Code 特殊编辑

- [ ] E-001 table cell 独立 `text_for_range/selected_text_range/marked_text_range`，对齐 V1 table runtime。
- [ ] E-002 table cell `replace_text_in_range` 只更新 cell payload，不动整篇 payload。
- [ ] E-003 code block language toolbar query 支持 text input / backspace / enter。
- [ ] E-004 code block content IME 与普通文本共用 runtime composition。
- [ ] E-005 测试：table cell IME preview/commit/cancel。

### F. 性能 / 架构约束

- [ ] F-001 input/IME hot path 不同步写 PostgreSQL。
- [ ] F-002 当前 editing/composition block 必须 pin。
- [x] F-003 hit-test fallback 只对 clicked block 做轻量计算，不能全窗口/全文 shaping。
- [ ] F-004 composition preview 不触发全局 page reflow。
- [ ] F-005 scroll wheel/scrollbar active 时 composition height correction 不反跳。

---

## 7. 推荐实施顺序

1. **A：点击定位完整化**，先彻底解决“不能在字符中间插入”。
2. **B：IME selected subrange**，补齐 V1 `new_selected_range_utf16` 语义。
3. **C：composition cancel/commit 事务化**。
4. **D：selection replacement**，统一跨 block selection 输入替换。
5. **E：table/code 特殊编辑**。
