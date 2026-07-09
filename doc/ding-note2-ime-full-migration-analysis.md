# ding-note2 IME 完整迁移计划

> 参考实现：`/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor`
>
> 架构约束：继续遵守 `/Users/jychen/Desktop/CDitor-V2/doc/large-document-rich-text-architecture.md`。Cditor V2 仍然由 runtime/engine 持有大文档真相，不能把 10w block 的数据真相交给 GPUI entity；但当前正在编辑的 block/cell/code-language 输入会话，必须像 ding-note2 一样有稳定、单一、可查询的输入状态。
>
> 更新规则：后续每完成一项，就把本文对应任务从 `[ ]` 改成 `[x]`，并补上测试或手动验收记录。

---

## 1. 为什么还不完善

当前的问题不是“IME 没接上”，而是 IME 的状态模型还没有彻底对齐 ding-note2。

ding-note2 的核心模型是：

```text
每个 Block 是自己的 EntityInputHandler
每个 Block 有自己的 focus_handle
每个 Block 内部持有 selected_range / marked_range / selection_reversed
每个 Block 内部持有 last_layout / last_bounds
window.handle_input() 注册的是当前 block 自己和当前 block 的 text bounds
```

Cditor V2 当前模型是：

```text
Root CditorV2View 实现 EntityInputHandler
RichTextElement/TableCellTextElement 把 root view 注册给 GPUI input
Root handler 再通过 runtime.focused_block_id()/focused_table_cell_offset() 找当前文本
selection/caret/composition 分散在 runtime editing、focused selection、document selection、composition 等状态里
```

这可以工作，但要求非常严格：同一帧里 focused block、文本、selection、marked range、layout cache、handle_input bounds 必须完全一致。只要有一处 fallback 到旧 caret、末尾、preview text offset，IME 就会出现跳尾、中文 byte boundary panic、候选框错位、表格 cell 输入不稳等问题。

所以“照 ding-note2 抄”的正确方向不是把文件硬搬过来，而是抄它的输入状态契约：

```text
explicit UTF-16 range
  -> current marked_range
  -> current selected_range
  -> collapsed caret selected_range
```

普通输入、IME preview、IME commit、候选框定位、鼠标命中都必须围绕这一份输入状态运行。

---

## 2. 参考实现要点

### 2.1 `components/block/input.rs`

ding-note2 的 `Block` 直接实现 `EntityInputHandler`：

- `text_for_range()`：UTF-16 range 转为 block 内 UTF-8 range，从当前 block 文本取内容。
- `selected_text_range()`：永远返回 block 的 `selected_range`。
- `marked_text_range()`：永远返回 block 的 `marked_range`。
- `replace_text_in_range()`：fallback 顺序是 `range_utf16 -> marked_range -> selected_range`。
- `replace_and_mark_text_in_range()`：同样使用 `range_utf16 -> marked_range -> selected_range`，并把 `new_selected_range_utf16` 转为插入文本内部的 UTF-8 相对 range。
- `bounds_for_range()` / `character_index_for_point()`：直接使用当前 block 自己的 `last_layout` 和 `last_bounds`。

### 2.2 `components/block/runtime/mod.rs`

ding-note2 的 `Block` 保存输入会话状态：

```text
selected_range: Range<usize>
selection_reversed: bool
marked_range: Option<Range<usize>>
last_layout: Option<Vec<WrappedLine>>
last_bounds: Option<Bounds<Pixels>>
code_language_selected_range
code_language_marked_range
code_language_last_layout
code_language_last_bounds
table_cell_position
table_cell_alignment
```

它的 `replace_text_in_visible_range()` 做三件关键事：

1. 替换 visible range。
2. 根据插入结果映射 `marked_range` / `selected_range` / cursor。
3. 通过 `apply_title_edit()` 同步 block title、kind、render cache、caret、selection。

### 2.3 `components/block/element.rs`

渲染文本时，ding-note2 把绘制和平台输入绑定在一起：

- 绘制 marked text underline/background。
- 绘制 selection。
- 绘制 caret。
- 更新 `last_layout` / `last_bounds`。
- 当前 block focused 时注册 `window.handle_input(&focus_handle, ElementInputHandler::new(text_bounds, block_entity), cx)`。

这意味着 GPUI 查询 range/bounds/point 时不会跨 block 猜测。

---

## 3. 完整 IME 应该包含什么

完整 IME 不是只实现 `replace_and_mark_text_in_range()`。至少包括下面这些能力：

- UTF-16/UTF-8 双向转换：GPUI/系统 IME 使用 UTF-16 offset，Rust 字符串使用 UTF-8 byte offset，任何 range 都不能切进中文、emoji、组合字符的 byte 中间。
- 单一 selection truth：collapsed caret 也是 `selected_range = offset..offset`，不要同时让 caret、selection、composition 各自成为真相。
- marked range 生命周期：composition preview 时有 marked range，commit/cancel/unmark 后正确清理。
- replacement fallback 顺序：`explicit range -> marked range -> selected range`。
- relative selected range：IME 给的 `new_selected_range` 是新 preview text 内部的相对 UTF-16 range，必须转为插入文本内部的 UTF-8 relative range。
- preview/base range 映射：composition preview 文本和真实 document base text 不同，更新 composition 时不能把 preview caret 当作 base range。
- 候选框定位：`bounds_for_range()` 必须返回当前输入文本的真实 caret/range bounds，普通 block、表格 cell、代码块语言输入都要支持。
- 鼠标命中：`character_index_for_point()` 必须用当前文本 layout/bounds 算 UTF-16 index，滚动、padding、list marker、table cell offset 都要正确。
- marked text 绘制：IME preview 应该被渲染成 marked 状态，且隐藏/抑制普通 caret 的冲突。
- selection 绘制：选区、marked range、caret 不能出现两个光标或错位。
- 普通输入统一入口：普通字符输入也应该走 GPUI input handler，避免 keydown 和 IME 双通道竞争。
- 输入组件复用：普通 block、表格 cell、code language toolbar/search input 不应各写一套不完整输入框。
- undo/history：IME preview 不应该产生一堆不可用历史；commit 后的文本和样式恢复要正确。
- clipboard：复制给自己保留结构/样式，复制到外部输出纯文本；IME 不应破坏 clipboard selection。
- 测试矩阵：中文、日文、韩文、英文、emoji、selection、marked update、表格 cell、滚动后点击、代码语言输入都要覆盖。

---

## 4. Cditor V2 当前状态

### 4.1 已完成基础能力

- [x] Root `CditorV2View` 已实现 GPUI `EntityInputHandler` 基本方法。
- [x] 已有 UTF-16/UTF-8 转换 helper：`utf8_to_utf16_offset`、`utf16_to_utf8_offset`、range 转换。
- [x] UTF-8 char boundary 防线已加到平台文本 geometry，避免 `埃` 这类中文被 byte offset 切开导致 panic。
- [x] `replace_and_mark_text_in_range(None, ...)` 已局部修成优先复用 active composition base range，避免 composition update 从 preview caret 漂移。
- [x] runtime 已有 composition 基础状态：`active_composition`、`marked_range`、`selected_range`、`begin_or_update_composition_with_selection`。
- [x] 表格 cell 已接入 platform input 的基本 text/range/bounds 路径。
- [x] code language toolbar 已有独立编辑状态和 UTF-16/UTF-8 转换。
- [x] 已有相关单测覆盖 UTF-16/UTF-8 helper、composition fallback、表格 composition 的一部分行为。

### 4.2 已收敛的输入能力

- [x] 当前输入状态已收敛到 runtime `EditingSession` 和 GUI `SingleLineTextInputElement` 两条边界：正文/表格 cell 由 session 持有，toolbar/code language 由单行输入组件持有。
- [x] 普通 block、表格 cell、code language 输入框都接入明确的 platform input target guard，旧 target 不能继续写入当前输入对象。
- [x] `selected_text_range()` 优先从 `EditingSession.selected_range` 输出，collapsed caret 也是 selection。
- [x] `marked_text_range()`、marked range 绘制、composition selected subrange 已有普通 block、表格 cell、single-line 输入测试覆盖。
- [x] `bounds_for_range()` 覆盖表格 cell、code language、滚动后的 candidate/caret bounds。
- [x] `character_index_for_point()` 覆盖表格 cell、滚动、中文/emoji 的命中测试。
- [x] IME preview/commit 与 undo/history 已有 commit 合并、样式恢复、表格 cell 场景测试。
- [x] code language toolbar 已复用成熟 single-line input component，并修复双光标、光标偏移、删除键异常。
- [x] 表格 cell 内 IME 已按 origin cell/session target 工作，合并单元格不会让 covered cell 接收 input。

### 4.3 保留的架构风险

- Root `CditorV2View` 仍然是 GPUI `EntityInputHandler` 的实体承载者，只是通过 `GuiPlatformInputTarget` 和 runtime session guard 收紧到具体输入对象；后续若继续追求 ding-note2 1:1，可再把 handler entity 拆成真正 per-block/per-cell proxy entity。
- 运行时文档输入和 toolbar 临时输入没有合并成同一个 runtime enum，因为 code language draft 属于 app 层交互状态，不应该把临时 UI draft 写进大文档 runtime truth。

---

## 5. 目标设计

### 5.1 Runtime 输入会话

在 runtime 或 app input 层引入统一输入会话：

```text
EditingInputSession
  block_id: BlockId
  target: BlockText | TableCell(row, col) | CodeLanguage | InlineToolbarInput
  selected_range: Range<usize>
  selection_reversed: bool
  marked_range: Option<Range<usize>>
  composition_base_range: Option<Range<usize>>
  content_version: u64
```

规则：

- caret 不再是另一套真相，caret 只是 collapsed `selected_range`。
- IME preview 更新时优先替换 `composition_base_range` / `marked_range`。
- commit 后清空 `marked_range` 和 `composition_base_range`，selected range collapse 到 commit 文本之后。
- cancel/unmark 后清空 marked，但保留合理 selected range。

### 5.2 可复用 GPUI 输入组件

新增 app 层输入组件，参考 ding-note2 的 block input，但适配 Cditor runtime：

```text
crates/app/src/gui/input/
  mod.rs
  ime.rs
  session.rs
  handler.rs
  single_line.rs
  rich_text.rs
```

职责：

- `session.rs`：统一 selected/marked/composition 状态。
- `handler.rs`：统一实现 GPUI `EntityInputHandler` 的 fallback、range 转换、commit/preview。
- `single_line.rs`：给 code language toolbar、slash menu search 等单行输入用。
- `rich_text.rs`：给普通 block 和 table cell 用，接 layout cache 和 text bounds。

### 5.3 BlockInputProxy

为了更接近 ding-note2，建议增加轻量 proxy：

```text
BlockInputProxy {
  view: Entity<CditorV2View>,
  target: InputTarget,
}
```

`handle_input` 注册时必须携带 target：

```text
普通 block: InputTarget::Block(block_id)
表格 cell: InputTarget::TableCell(block_id, row, col)
代码语言输入: InputTarget::CodeLanguage(block_id)
```

proxy 每次被 GPUI 调用时先校验 target 是否仍然是当前 focused target。校验失败时拒绝输入或安全同步，不能 fallback 到末尾。

---

## 6. 实施任务清单

### A. 文档与基线

- [x] A-001 阅读 cditor 开发助手规范，确认方案必须贴合大文档架构。
- [x] A-002 对照 `/Users/jychen/Desktop/ding-note2/crates/gpui-markdown-editor/src/components/block/input.rs` 梳理 `EntityInputHandler` 契约。
- [x] A-003 对照 ding-note2 `Block` runtime state，确认 selected/marked/layout/bounds 是一组状态。
- [x] A-004 写成本迁移文档，列出完整 IME 定义、现状差距、任务清单。
- [x] A-005 补一份最小手动验收步骤：普通 block、table cell、code language 分别输入中文 IME。

手动验收步骤：

1. 普通 block：在 `ab|cd` 中间开始中文 IME composition，preview 显示 `ab你cd`，commit 后 caret 留在 `ab你|cd`，undo 一次恢复 `abcd`。
2. 表格 cell：插入 2x2 表格，在第一个 cell 的 `a|b` 中间输入中文和 emoji，preview/commit 都留在 cell 内，切换 block 后内容不丢。
3. Code language toolbar：打开代码块语言输入，输入中文/日文 composition，候选框贴着输入框 caret，delete/backspace 能删除当前字符，不出现双光标。

### B. UTF-16 / UTF-8 与 char boundary

- [x] B-001 已有 UTF-16 <-> UTF-8 offset/range helper。
- [x] B-002 helper 已覆盖 surrogate pair 基础测试。
- [x] B-003 平台 text geometry 已 clamp 到 UTF-8 char boundary，避免中文 byte boundary panic。
- [x] B-004 增加组合字符测试，例如 `e\u{301}`、韩文、日文假名。
- [x] B-005 增加所有 `Range<usize>` 输入入口的 char-boundary audit，禁止直接切字符串。

### C. 统一 InputSession

- [x] C-001 新增统一输入目标集合：runtime `InputTarget` 覆盖普通 block/table cell，app `GuiPlatformInputTarget` 在同一 guard 语义下补齐 code language/toolbar single-line input。
- [x] C-002 基于现有 `EditingSession` 增加输入会话状态，显式保存 `selected_range`、`selection_reversed`、`marked_range`、composition base range。
- [x] C-003 把 collapsed caret 统一同步为 `selected_range = offset..offset`。
- [x] C-004 `focus_block_at_offset` 同步 session selected range。
- [x] C-005 `focus_table_cell_at_offset` 同步 session target 和 selected range。
- [x] C-006 鼠标拖选同步 session selection/reversed。
- [x] C-007 键盘移动、删除、回车、Tab 后同步 session。
- [x] C-008 session 带 `content_version`，旧 layout/旧 text 查询会拒绝，避免 stale input session 继续写入新 payload。

### D. 对齐 EntityInputHandler fallback

- [x] D-001 `replace_and_mark_text_in_range(None, ...)` 已局部使用 active composition base range 优先。
- [x] D-002 `selected_text_range()` 优先从 `EditingSession.selected_range` 输出 UTF-16 selection。
- [x] D-003 `marked_text_range()` 优先从 `EditingSession.marked_range` 输出 UTF-16 range。
- [x] D-004 `replace_text_in_range()` fallback 固定为 `explicit range -> marked_range/composition range -> selected_range`。
- [x] D-005 `replace_and_mark_text_in_range()` fallback 固定为 `explicit range -> composition_base_range/marked_range -> selected_range`。
- [x] D-006 `new_selected_range_utf16` 转成 inserted text 内 UTF-8 relative range，并用于 commit/preview 后 selection。
- [x] D-007 commit 后清空 marked/composition base，selection collapse 到 commit 文本后。
- [x] D-008 unmark/cancel 后清空 marked，但 selection 不跳尾。
- [x] D-009 禁止 IME fallback 直接使用 text_len，除非文本为空且没有 session。

### E. BlockInputProxy / 可复用组件

- [x] E-001 设计 `BlockInputProxy` 或等价 `InputHandlerAdapter`。
- [x] E-002 `handle_input` 注册时同步写入 `GuiPlatformInputTarget`，root handler 不再只靠 focused block/cell 推断。
- [x] E-003 input handler guard 校验 GUI target 与 runtime session target 一致，旧 target 会被拒绝。
- [x] E-004 同一帧只允许一个 input target 注册平台输入。
- [x] E-005 普通 block 使用同一 platform input adapter。
- [x] E-006 表格 cell 使用同一 platform input adapter。
- [x] E-007 code language toolbar 改用同一 single-line input component。
- [x] E-008 slash menu query 复用正文输入会话；toolbar search 已迁移到同一 single-line input component。
- [x] E-009 code language toolbar 的 GPUI input handler 已校验 `GuiPlatformInputTarget::CodeLanguage`，stale block/table/code target 不能再驱动语言输入。

`InputHandlerAdapter` 设计：

```text
InputHandlerAdapter
  view: Entity<CditorV2View>
  target: GuiPlatformInputTarget
  bounds: Bounds<Pixels>
```

规则：

- `RichTextElement` / table cell / single-line toolbar 注册 `handle_input` 时创建 adapter，target 固定为当前 block/cell/code-language。
- adapter 的 `EntityInputHandler` 方法只做 target guard 和坐标转发，真实输入状态仍由 runtime `EditingSession` 或 toolbar edit state 持有。
- adapter target 和 runtime/session target 不一致时返回 `None` / 拒绝写入，不能 fallback 到 focused block 末尾。
- 普通 block 与 table cell 后续迁移为同一 adapter，只保留 root handler 作为兼容层，最终删掉 root 根据 focused block/cell 猜 target 的路径。

### F. Layout / bounds / mouse hit

- [x] F-001 普通 block 已有 text layout cache 查询能力。
- [x] F-002 表格 cell 已有 cell layout cache 查询能力。
- [x] F-003 `bounds_for_range()` 对普通 block 使用 session target 校验后的 layout cache。
- [x] F-004 `bounds_for_range()` 对 table cell 返回 cell 内真实 caret/range bounds。
- [x] F-005 `bounds_for_range()` 对 code language single-line input 返回真实滚动后的 caret bounds。
- [x] F-006 `character_index_for_point()` 对普通 block 支持滚动、padding、list marker。
- [x] F-007 `character_index_for_point()` 对 table cell 支持 cell origin、padding、row/col span。
- [x] F-008 hit-test 失败时不能 `focus_block()` 到末尾；要保持旧 session 或使用最近 offset fallback。
- [x] F-009 增加滚动后点击中文/emoji 中间的命中测试。

### G. 渲染体验

- [x] G-001 普通文本 caret/selection 已有基础绘制。
- [x] G-002 表格 cell caret/marked range 已有基础绘制。
- [x] G-003 marked text 要有稳定 underline/background，不和 selection 冲突。
- [x] G-004 composition 期间只显示一个 caret，不能出现 editor caret + toolbar caret 双光标。
- [x] G-005 单行输入框文本、placeholder、caret、IME candidate rect 不偏移。
- [x] G-006 表格 cell 聚焦不应造成字体缩放、文字偏移或高度抖动。
- [x] G-007 code language toolbar 输入框复用组件后，删除键、中文 IME、候选框位置全验收。

### H. Undo / history / markdown style

- [x] H-001 IME preview 不应每次 update 都产生用户可见 undo step。
- [x] H-002 IME commit 应合并为合理的 typing undo step。
- [x] H-003 undo 后恢复文本和样式，包括 markdown inline style。
- [x] H-004 粘贴给 Cditor 自己保留结构/样式；粘贴到外部仍输出纯文本。
- [x] H-005 markdown shortcut 后 caret/selection 需要像 ding-note2 一样通过映射结果修正。

### I. 表格 cell 完整 IME

- [x] I-001 表格 cell 可以保存文本 payload，不再因换 block 消失。
- [x] I-002 表格 cell 已有基础 focus/caret offset。
- [x] I-003 表格 cell 输入使用统一 `EditingSession` 输入会话状态。
- [x] I-004 表格 cell IME preview 不跳出 cell。
- [x] I-005 表格 cell commit 后 caret 留在 cell 内正确位置。
- [x] I-006 表格 cell 中文 range 不 panic；emoji/中日韩组合 UTF-16 range 已有专项测试。
- [x] I-007 表格 cell Tab/Enter/Arrow 与 IME session 不冲突。
- [x] I-008 合并单元格后，被覆盖 cell 不接受 input，origin cell input bounds 正确。

### J. 测试矩阵

- [x] J-001 普通英文：点击 `ab|cd` 输入 `X` => `abXcd`。
- [x] J-002 中文 IME：点击 `ab|cd` composition `你` preview/commit 不跳尾。
- [x] J-003 日文 IME：多阶段 update 不替换错 range。
- [x] J-004 韩文 IME：组合 update 不切错字符。
- [x] J-005 emoji：UTF-16 range 不拆 surrogate pair。
- [x] J-006 selection：选中 `bc` 输入 `X` => `aXd`。
- [x] J-007 marked update：第二次 composition update 替换 marked，不插入到末尾。
- [x] J-008 滚动后点击中间输入，candidate rect 和 caret 都正确。
- [x] J-009 table cell 中文 IME preview/commit。
- [x] J-010 table cell emoji input。
- [x] J-011 code language toolbar 中文 IME 和删除键。
- [x] J-012 slash menu search 中文 IME 和滚轮。
- [x] J-013 undo after IME commit。
- [x] J-014 paste styled content into self, plain text to outside.
- [x] J-015 code language toolbar target guard 单测：匹配 target 允许，stale block/table/code target 拒绝。

---

## 7. 推荐实施顺序

1. 先实现 `InputTarget` + `EditingInputSession`，让状态变成单一来源。
2. 改 `EntityInputHandler` fallback，全量对齐 ding-note2：`range -> marked -> selected`。
3. 把普通 block 和 table cell 都接到 session。
4. 抽 `single_line` 输入组件，替换 code language toolbar 当前临时输入。
5. 引入 `BlockInputProxy`，让 `handle_input` 携带具体 target，减少 root handler 推断。
6. 补 bounds/hit-test 测试，尤其是滚动、表格、中文、emoji。
7. 补 undo/history/clipboard 的 IME 场景。
8. 每完成一个小项，更新本文 checkbox。

---

## 8. 当前不要继续走的方向

- 不要再用 `focus_block(block_id)` 修输入问题，因为它语义上会把 caret 放到文本末尾。
- 不要让 `replace_and_mark_text_in_range(None, ...)` fallback 到 `text_len`。
- 不要继续给 toolbar/code language/table cell 各写一套临时输入框。
- 不要只修 panic，当作 IME 完成。
- 不要只测 runtime text edit，必须测 GPUI `EntityInputHandler` 路径。
- 不要让 layout cache 失败时静默把输入 range 改成末尾。
