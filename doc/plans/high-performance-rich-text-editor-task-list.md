# 高性能富文本编辑器接入任务清单（细化版）

本文档用于跟踪 **V2 高性能富文本编辑器 GUI 接入**。每个任务都必须满足：

1. 可实施：说明大概要改哪些文件、怎么改。
2. 可验证：至少有一个测试、诊断或手动验收方法。
3. 可验收：有明确完成定义。
4. 可勾选：完成一项后勾选对应 checkbox。

## 当前基线

已完成：

- V2 core：`DocumentIndex`、`VisibleDocumentIndex`、`BlockHeightIndex`、`PageLayoutIndex`、`VirtualScrollState`。
- Runtime：`DocumentRuntime` 最小闭环。
- GUI：`CditorV2View` 最小 GPUI 窗口。
- Rich text model：`RichTextDocument`、`RichBlockRecord`、`RichBlockKind`、`BlockPayload`、`InlineSpan`、`InlineMark`。
- Markdown：block shortcut、inline shortcut、parse/import。
- GUI rich text renderer：普通 GPUI element 版 bold / italic / underline / strike / code / link。

未完成核心：

- 真实窗口化渲染。
- 自定义高性能 text element。
- caret / selection / IME geometry。
- layout height 测量回写。
- EntityCache + Pin。
- 跨页 selection / paste。
- Code/Table/Image 复杂 block 高性能 renderer。

---

# Phase 1：GUI 接入真实 RenderWindow / WindowPlanner

目标：GUI 不再全量渲染 visible blocks，而是根据 `VirtualScrollState` 和 `PageLayoutIndex` 只渲染当前窗口。

## 1.1 拆分 projection：全量 projection 与窗口 projection

- [x] 1.1.1 新增 `DocumentRuntime::projection_for_window()`，保留当前 `projection()` 作为兼容入口。

**涉及文件**

- `src/runtime/document_runtime.rs`
- `src/runtime/view_projection.rs`

**怎么做**

1. 新增方法：

   ```rust
   pub fn projection_for_window(&self) -> EditorViewProjection
   ```

2. 初始实现可以先调用现有 `projection()`。
3. 后续任务再替换内部逻辑。
4. `CditorV2View` 先改为调用 `projection_for_window()`。

**验收标准**

- GUI 行为不变。
- 后续窗口化改动不需要再改 GUI 调用点。

**验证**

```bash
cargo test document_runtime
cargo check
```

---

- [x] 1.1.2 为 `EditorViewProjection` 增加窗口前后占位高度字段。

**涉及文件**

- `src/runtime/view_projection.rs`
- `src/runtime/document_runtime.rs`
- `src/gui/cditor_v2.rs`

**怎么做**

1. 在 `EditorViewProjection` 增加：

   ```rust
   pub before_window_height: f64,
   pub after_window_height: f64,
   pub total_visible_blocks: usize,
   ```

2. 当前全量 projection 下先填：

   ```rust
   before_window_height = 0.0
   after_window_height = 0.0
   total_visible_blocks = visible_index.total_visible_count()
   ```

3. GUI debug 文案显示这三个字段。

**验收标准**

- projection 能表达窗口前后 spacer。
- Debug 能看到 total blocks 与 rendered blocks 的区别。

**验证**

```bash
cargo test document_runtime_projects_v2_blocks_without_ui_truth
cargo check
```

---

- [x] 1.1.3 增加 projection window 单元测试骨架。

**涉及文件**

- `src/runtime/document_runtime.rs`

**怎么做**

1. 新增测试：

   ```rust
   fn projection_for_window_exposes_total_visible_count_and_spacers()
   ```

2. 先断言：

   - `projection.total_visible_blocks == runtime.visible_index.total_visible_count()`
   - `before_window_height == 0.0`
   - `after_window_height == 0.0`

3. 后续窗口化时更新断言。

**验收标准**

- 有测试保护 projection 字段语义。

**验证**

```bash
cargo test projection_for_window
```

---

## 1.2 基于 `global_scroll_top` 计算当前 page range

- [x] 1.2.1 新增 Runtime 方法：`current_page_window()`。

**涉及文件**

- `src/runtime/document_runtime.rs`
- 可能涉及：`src/editor/window/*`

**怎么做**

1. 新增：

   ```rust
   fn current_page_window(&self) -> Range<usize>
   ```

2. 使用：

   ```rust
   let current_page = self.page_layout.page_at_offset(self.scroll.global_scroll_top)
   ```

3. 初始 overscan 策略：

   ```rust
   before_pages = 1
   after_pages = 2
   ```

4. clamp 到 `0..page_count`。

**验收标准**

- scroll top 在第一页时，page window 从 0 开始。
- scroll top 到中间时，page window 包含中间页。
- 不越界。

**验证**

```bash
cargo test current_page_window
```

---

- [x] 1.2.2 增加 10w blocks runtime fixture。

**涉及文件**

- `src/runtime/document_runtime.rs`
- 或新增 `src/runtime/test_fixtures.rs`

**怎么做**

1. 新增测试辅助函数：

   ```rust
   fn runtime_with_paragraph_blocks(count: usize) -> DocumentRuntime
   ```

2. 构造 `RichTextDocument` 或 `BlockPayloadRecord`。
3. 确保不生成巨大字符串。
4. 用固定高度估算。

**验收标准**

- 测试中能快速构造 100_000 blocks runtime。
- 不 OOM。

**验证**

```bash
cargo test runtime_with_100k_blocks_fixture
```

---

- [x] 1.2.3 projection 使用 page window 生成 block range。

**涉及文件**

- `src/runtime/document_runtime.rs`

**怎么做**

1. 根据 page range 计算 block range：

   ```rust
   start = pages[start_page].block_start
   end = last_page.block_start + last_page.block_count
   ```

2. block range clamp 到 visible count。
3. projection 只遍历 `visible_block_ids[block_range]`。

**验收标准**

- 10w blocks 文档 projection blocks 数远小于 10w。
- `render_window.block_range` 等于实际 projection block range。

**验证**

```bash
cargo test projection_for_window_limits_blocks_for_100k_document
```

---

## 1.3 Spacer / placeholder 接 GUI

- [x] 1.3.1 计算窗口前后 spacer height。

**涉及文件**

- `src/runtime/document_runtime.rs`

**怎么做**

1. `before_window_height`：

   - page window start page 的 offset。

2. `after_window_height`：

   - `total_height - before - window_height`。

3. 使用 `PageLayoutIndex::offset_of_page` / page height。

**验收标准**

- before + rendered window height + after 约等于 total height。
- 滚动到中间时 before > 0。

**验证**

```bash
cargo test projection_window_spacer_heights_sum_to_total
```

---

- [x] 1.3.2 GUI 渲染 spacer。

**涉及文件**

- `src/gui/cditor_v2.rs`

**怎么做**

1. 在 block 列表前插入：

   ```rust
   div().h(px(projection.before_window_height as f32))
   ```

2. 在 block 列表后插入 after spacer。
3. Debug 显示 before/after。

**验收标准**

- 窗口化后页面总高度仍接近文档高度。
- 滚动时视觉位置不完全跳到顶部。

**验证**

```bash
cargo check
cargo run
```

手动：打开 GUI，看 debug 中 before/after 与 window range。

---

## 1.4 GPUI wheel 接入 runtime

- [x] 1.4.1 查 GPUI wheel event API 并添加 root handler。

**涉及文件**

- `src/gui/cditor_v2.rs`

**怎么做**

1. 从 GPUI examples / Zed 源码确认 API：`on_mouse_wheel` 或等价事件。
2. 在 root div 上绑定 wheel handler。
3. 先只打印/更新 debug，不做复杂滚动。

**验收标准**

- wheel event 能进入 `CditorV2View`。
- 不影响 key input。

**验证**

```bash
cargo check
cargo run
```

手动：滚轮时 debug 值变化或日志变化。

---

- [x] 1.4.2 新增 `DocumentRuntime::scroll_by_delta(delta_y)`。

**涉及文件**

- `src/runtime/document_runtime.rs`

**怎么做**

1. 新增：

   ```rust
   pub fn scroll_by_delta(&mut self, delta_y: f64) -> Result<(), String>
   ```

2. 内部调用：

   ```rust
   self.scroll.scroll_by_delta(delta_y, ScrollOrigin::UserWheel)
   ```

3. 不直接改 GUI local scroll。

**验收标准**

- `global_scroll_top` 被 clamp 在合法范围。
- delta 正负方向正确。

**验证**

```bash
cargo test document_runtime_scroll_by_delta
```

---

- [x] 1.4.3 GUI wheel 调用 runtime scroll。

**涉及文件**

- `src/gui/cditor_v2.rs`

**怎么做**

1. handler 中读取 wheel delta。
2. 调用 `runtime.scroll_by_delta(delta_y)`。
3. `cx.notify()`。
4. `cx.stop_propagation()`，避免 local ListState 双真相。

**验收标准**

- GUI 滚动后 `global_scroll_top` 变化。
- projection window 随 scroll 更新。

**验证**

```bash
cargo test gui_scroll
cargo run
```

---

# Phase 2：自定义 RichTextElement / Text Layout Cache

目标：替换当前 div-based rich text renderer，接入高性能文本布局、绘制、hit test。

## 2.1 建立 text element 模块骨架

- [x] 2.1.1 新增目录 `src/gui/text_element/`。

**涉及文件**

- `src/gui/text_element/mod.rs`
- `src/gui/text_element/input.rs`
- `src/gui/text_element/layout.rs`
- `src/gui/text_element/element.rs`
- `src/gui/mod.rs`

**怎么做**

1. 建目录。
2. 定义：

   ```rust
   pub struct RichTextLayoutInput { ... }
   pub struct RichTextElement { ... }
   ```

3. 暂时让 `RichTextElement` 委托现有 `gui::rich_text::render_inline_spans`。

**验收标准**

- 模块能编译。
- 不改变当前 GUI 视觉。

**验证**

```bash
cargo check
```

---

- [x] 2.1.2 定义 `RichTextLayoutInput`。

**涉及文件**

- `src/gui/text_element/input.rs`
- `src/runtime/view_projection.rs`

**怎么做**

字段建议：

```rust
pub struct RichTextLayoutInput {
    pub block_id: BlockId,
    pub content_version: u64,
    pub layout_version: u64,
    pub kind: RichBlockKind,
    pub spans: Vec<InlineSpan>,
    pub width_px: f64,
    pub theme_version: u64,
    pub font_version: u64,
}
```

注意：

- 可以 clone spans，第一版先接受。
- 后续优化成 Arc/Rc。

**验收标准**

- 可从 `ViewBlockSnapshot` + payload 构造 input。

**验证**

```bash
cargo test rich_text_layout_input_from_snapshot
```

---

- [x] 2.1.3 `CditorV2View` 使用 `RichTextElement` 渲染 RichText payload。

**涉及文件**

- `src/gui/cditor_v2.rs`
- `src/gui/rich_text.rs`
- `src/gui/text_element/element.rs`

**怎么做**

1. 在 `render_payload_text` 中：

   - `BlockPayload::RichText` 分支改为创建 `RichTextElement`。

2. 其他 payload 仍用当前 renderer。

**验收标准**

- Markdown bold/code/link 仍可显示。
- element-tree renderer 不再负责 RichText 主路径。

**验证**

```bash
cargo test markdown
cargo check
cargo run
```

---

## 2.2 TextLayoutKey 与 layout cache

- [x] 2.2.1 定义 `TextLayoutKey`。

**涉及文件**

- `src/gui/text_element/layout.rs`

**怎么做**

定义：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextLayoutKey {
    pub block_id: BlockId,
    pub content_version: u64,
    pub width_bucket: u16,
    pub theme_version: u64,
    pub font_version: u64,
    pub scale_factor_bits: u64,
}
```

**验收标准**

- key 可 hash。
- width bucket 变化会导致 key 变化。

**验证**

```bash
cargo test text_layout_key_changes_when_width_bucket_changes
```

---

- [x] 2.2.2 定义 `WrappedLine` / `VisualRun` 数据结构。

**涉及文件**

- `src/gui/text_element/layout.rs`

**怎么做**

最小结构：

```rust
pub struct WrappedLine {
    pub logical_range: Range<usize>,
    pub y: f64,
    pub height: f64,
    pub runs: Vec<VisualRun>,
}

pub struct VisualRun {
    pub logical_range: Range<usize>,
    pub x: f64,
    pub width: f64,
    pub mark_style: InlineStyle,
}
```

**验收标准**

- 能表达一行和多行文本。
- 能给 hit test 用。

**验证**

```bash
cargo test wrapped_line_model
```

---

- [x] 2.2.3 实现最小 line wrap 算法。

**涉及文件**

- `src/gui/text_element/layout.rs`

**怎么做**

第一版可用估算：

- monospace-ish average char width。
- 按 width 断行。
- 保证不切 UTF-8 char boundary。
- 后续替换为 GPUI text shaping。

**验收标准**

- 长文本会分多行。
- 中文/emoji 不在非法 byte boundary 截断。

**验证**

```bash
cargo test rich_text_wrap_does_not_split_utf8
cargo test rich_text_wraps_long_text
```

---

- [x] 2.2.4 增加 layout cache。

**涉及文件**

- `src/gui/text_element/layout.rs`
- `src/gui/text_element/element.rs`

**怎么做**

1. 定义：

   ```rust
   pub struct RichTextLayoutCache {
       entries: HashMap<TextLayoutKey, RichTextLayout>
   }
   ```

2. `layout(input)` 时：

   - key 命中直接返回。
   - miss 时计算并写入。

3. 限制 cache 大小，先用简单 LRU 或容量上限。

**验收标准**

- 相同 key 第二次命中。
- content_version 改变后 miss。

**验证**

```bash
cargo test text_layout_cache_hits_same_key
cargo test text_layout_cache_misses_after_content_change
```

---

## 2.3 RichTextElement 绘制与高度输出

- [x] 2.3.1 `RichTextElement` 使用 layout 结果绘制。

**涉及文件**

- `src/gui/text_element/element.rs`

**怎么做**

1. 实现 GPUI `Element` 或项目当前可用的 custom element 接口。
2. prepaint/layout 阶段生成 `RichTextLayout`。
3. paint 阶段绘制 runs。
4. 第一版不必完美 selection/caret。

**验收标准**

- 显示效果等价当前 div renderer。
- 长文本换行。
- style marks 生效。

**验证**

```bash
cargo test rich_text_element_paints_spans
cargo check
cargo run
```

---

- [x] 2.3.2 输出 measured height。

**涉及文件**

- `src/gui/text_element/layout.rs`
- `src/gui/text_element/element.rs`
- `src/runtime/document_runtime.rs`

**怎么做**

1. `RichTextLayout` 增加：

   ```rust
   pub height: f64
   ```

2. element layout 后产生 `BlockMeasuredHeight` event / callback。
3. runtime 提供：

   ```rust
   apply_measured_height(block_id, content_version, height)
   ```

**验收标准**

- 输入造成换行后 height 变化可回写 runtime。
- 过期 content_version 的测量被拒绝。

**验证**

```bash
cargo test measured_height_rejects_stale_content_version
cargo test rich_text_height_updates_after_wrap
```

---

# Phase 3：Caret / Selection / IME

目标：输入不再总在末尾；selection 和 IME 不依赖 UI entity。

## 3.1 Runtime caret offset

- [x] 3.1.1 `insert_char` 使用 caret offset。

**涉及文件**

- `src/runtime/document_runtime.rs`
- `src/runtime/input_hot_path.rs`

**怎么做**

1. 当前：

   ```rust
   let offset = model.len();
   ```

2. 改成：

   ```rust
   let offset = editing.caret_anchor.text_offset as usize;
   ```

3. 插入前校验 char boundary / grapheme boundary。
4. 插入后 caret offset += inserted len。

**验收标准**

- 设置 caret 到中间，输入发生在中间。
- 插入后 caret 前进。

**验证**

```bash
cargo test insert_char_uses_caret_offset
```

---

- [x] 3.1.2 `delete_backward` 使用 caret offset。

**涉及文件**

- `src/runtime/document_runtime.rs`

**怎么做**

1. 当前 caret = model.len()。
2. 改成 caret = editing caret offset。
3. 使用 `TextOffsetMap::backspace_range`。
4. 删除后 caret = range.start。

**验收标准**

- caret 在中间时删除中间字符。
- emoji/ZWJ 删除完整 grapheme。

**验证**

```bash
cargo test delete_backward_uses_caret_offset
cargo test text_offset_map_handles_emoji_zwj_as_single_grapheme
```

---

- [x] 3.1.3 点击 block 设置 caret offset。

**涉及文件**

- `src/gui/cditor_v2.rs`
- `src/gui/text_element/element.rs`
- `src/runtime/document_runtime.rs`

**怎么做**

1. `RichTextElement` 提供 hit test：point -> offset。
2. mouse down 时调用：

   ```rust
   runtime.focus_block_at_offset(block_id, offset)
   ```

3. fallback：没有 hit test 时仍聚焦末尾。

**验收标准**

- 点击文本开头，输入到开头附近。
- 点击文本末尾，输入到末尾。

**验证**

```bash
cargo test focus_block_at_offset
cargo test rich_text_element_hit_test
```

---

## 3.2 DocumentSelection

- [ ] 3.2.1 用正式 `DocumentSelection` 替换 `selected_block_ids`。

**涉及文件**

- `src/runtime/document_runtime.rs`
- `src/runtime/view_projection.rs`
- `src/core/edit/mod.rs`

**怎么做**

1. `DocumentRuntime` 增加：

   ```rust
   pub selection: Option<DocumentSelection>
   ```

2. `Ctrl/Cmd+A` 构造全选 selection。
3. 临时保留 `selected_block_ids`，过渡完成后删除。

**验收标准**

- selection 真相在 runtime。
- projection 根据 selection 计算 `selected` / fragments。

**验证**

```bash
cargo test document_runtime_select_all_uses_document_selection
```

---

- [ ] 3.2.2 鼠标拖选生成 selection。

**涉及文件**

- `src/gui/cditor_v2.rs`
- `src/gui/text_element/element.rs`
- `src/runtime/document_runtime.rs`

**怎么做**

1. mouse down 记录 anchor。
2. mouse move 更新 focus。
3. mouse up 结束 drag。
4. runtime 中保存 `DocumentSelection { anchor, focus }`。

**验收标准**

- 单 block 内可拖选。
- 跨 block 可拖选。
- 滚出窗口 selection 不丢。

**验证**

```bash
cargo test visible_selection_fragments
cargo test cross_page_selection_fragments_only_current_visible_window
```

---

## 3.3 IME / Composition

- [x] 3.3.1 GUI composition event 接 runtime。

**涉及文件**

- `src/gui/cditor_v2.rs`
- `src/runtime/composition.rs`
- `src/runtime/document_runtime.rs`

**怎么做**

1. 查 GPUI composition event API。
2. begin/update/cancel/commit 转成 runtime composition command。
3. composition state 写入 `EditingSession.composition`。

**验收标准**

- 中文输入法 preview 不直接污染 committed text。
- commit 后进入 undo boundary。

**验证**

```bash
cargo test composition
```

---

- [x] 3.3.2 candidate rect 使用 caret geometry cache。

**涉及文件**

- `src/gui/text_element/layout.rs`
- `src/editor/hit_test/*`
- `src/runtime/composition.rs`

**怎么做**

1. RichTextLayout 提供 offset -> caret rect。
2. composition query candidate rect 时返回 current layout rect。
3. 校验 content/layout version。

**验收标准**

- candidate window 跟随 caret。
- stale geometry 被拒绝。

**验证**

```bash
cargo test ime_candidate_rect_rejects_stale_geometry
```

---

# Phase 4：高度测量与锚点修正

目标：真实布局高度能回写，并维持滚动/编辑锚点稳定。

## 4.1 Runtime 高度更新 API

- [ ] 4.1.1 新增 `DocumentRuntime::apply_measured_height`。

**涉及文件**

- `src/runtime/document_runtime.rs`
- `src/core/layout/height_index.rs`
- `src/core/layout/page_layout.rs`

**怎么做**

签名建议：

```rust
pub fn apply_measured_height(
    &mut self,
    block_id: BlockId,
    content_version: u64,
    height: f64,
) -> Result<Option<HeightChange>, String>
```

流程：

1. 找 payload content_version。
2. 不匹配则 reject。
3. 更新 `DocumentIndex.layout_meta`。
4. 更新 `BlockHeightIndex`。
5. 更新 `PageLayoutIndex`。

**验收标准**

- stale version 不更新。
- height change 返回 delta。

**验证**

```bash
cargo test apply_measured_height_rejects_stale_version
cargo test apply_measured_height_updates_height_indices
```

---

- [ ] 4.1.2 高度变化时恢复 viewport anchor。

**涉及文件**

- `src/runtime/document_runtime.rs`
- `src/editor/scroll/anchor.rs`

**怎么做**

1. 更新前 capture anchor。
2. 更新 height index/page layout。
3. 根据 height delta restore scroll top。
4. 当前编辑 block 使用 caret anchor 优先。

**验收标准**

- 当前视口上方 block 高度变化时，当前可见内容不跳。
- 当前编辑 block 自身变高时 caret 不漂。

**验证**

```bash
cargo test caret_anchor_restore_keeps_caret_viewport_y_stable_after_reflow
cargo test random_height_correction_while_scrolling_acceptance
```

---

## 4.2 GUI 测量回写

- [ ] 4.2.1 RichTextElement layout 后发送 measured height。

**涉及文件**

- `src/gui/text_element/element.rs`
- `src/gui/cditor_v2.rs`
- `src/runtime/document_runtime.rs`

**怎么做**

1. element layout 得到 height。
2. 通过 event/callback 通知 view。
3. view 调用 runtime `apply_measured_height`。
4. 每帧限制回写数量，避免 layout storm。

**验收标准**

- 输入长文本后高度更新。
- 一帧不会处理无限高度变化。

**验证**

```bash
cargo test measured_height_apply_budget
cargo run
```

---

# Phase 5：EntityCache + Pin

目标：窗口切换时避免当前编辑 block / dirty block / composition block 被销毁。

## 5.1 Runtime pin state 暴露给 GUI

- [ ] 5.1.1 projection 输出 pin reason。

**涉及文件**

- `src/runtime/view_projection.rs`
- `src/runtime/document_runtime.rs`
- `src/runtime/entity_cache.rs` 或新增

**怎么做**

1. 定义：

   ```rust
   pub enum ViewPinReason { Focus, Dirty, Composition, SelectionEndpoint, AsyncTask }
   ```

2. `ViewBlockSnapshot` 增加：

   ```rust
   pub pin_reasons: Vec<ViewPinReason>
   ```

3. 当前先输出 Focus。

**验收标准**

- focused block projection 中有 Focus pin reason。

**验证**

```bash
cargo test projection_marks_focused_block_pinned
```

---

- [ ] 5.1.2 Debug overlay 显示 pin reason。

**涉及文件**

- `src/gui/cditor_v2.rs`
- `src/editor/debug_overlay/*`

**怎么做**

1. Debug 行展示 pinned count。
2. 当前 focused block 显示 reason。

**验收标准**

- 点击 block 后 debug 显示 pinned reason。

**验证**

```bash
cargo check
cargo run
```

---

## 5.2 GUI entity cache

- [ ] 5.2.1 新增 `GuiBlockEntityCache`。

**涉及文件**

- `src/gui/entity_cache.rs`
- `src/gui/mod.rs`
- `src/gui/cditor_v2.rs`

**怎么做**

1. 定义 cache：

   ```rust
   struct GuiBlockEntityCache {
       entries: HashMap<BlockId, CachedBlockView>
   }
   ```

2. entry 记录 `content_version` / `layout_version`。
3. clean + not pinned + outside window 可 evict。

**验收标准**

- 当前编辑 block 不因窗口切换重建。
- 非 pinned block 可被回收。

**验证**

```bash
cargo test gui_entity_cache_keeps_pinned_block
```

---

# Phase 6：Paste / Markdown import 接 GUI

目标：clipboard paste 进入 V2 runtime；Markdown paste 走批量 import。

## 6.1 Plain text paste

- [ ] 6.1.1 查 GPUI paste action API 并绑定到 root。

**涉及文件**

- `src/gui/cditor_v2.rs`

**怎么做**

1. 参考 GPUI input example。
2. bind paste action。
3. 从 clipboard 获取 text。
4. 调 runtime paste command。

**验收标准**

- Cmd+V 能进 handler。

**验证**

```bash
cargo check
cargo run
```

---

- [ ] 6.1.2 Runtime 插入 plain text paste。

**涉及文件**

- `src/runtime/document_runtime.rs`

**怎么做**

1. 新增：

   ```rust
   paste_plain_text(text: &str)
   ```

2. 小文本插入当前 block。
3. 大文本后续走 batch task；第一步可以先限制大小。
4. undo 一次恢复 paste 前状态。

**验收标准**

- paste 后文本进入当前 caret。
- undo 一次撤销 paste。

**验证**

```bash
cargo test paste_plain_text_undoes_as_one_step
```

---

## 6.2 Markdown paste

- [ ] 6.2.1 GUI paste 判断 Markdown。

**涉及文件**

- `src/gui/cditor_v2.rs`
- `src/core/rich_text/markdown.rs`

**怎么做**

1. paste text 后判断：

   ```rust
   looks_like_markdown_paste(&text)
   ```

2. 命中则调用 runtime markdown paste。
3. 未命中走 plain text paste。

**验收标准**

- 粘贴 `# Title` 生成 heading block。
- 粘贴普通句子插入文本。

**验证**

```bash
cargo test paste_markdown_heading_creates_heading_block
```

---

- [ ] 6.2.2 Runtime batch insert Markdown blocks。

**涉及文件**

- `src/runtime/document_runtime.rs`
- `src/core/rich_text/markdown.rs`

**怎么做**

1. 新增：

   ```rust
   paste_markdown_after_focused(markdown: &str)
   ```

2. 使用 `parse_markdown_document`。
3. 将 parsed blocks 转 `BlockIndexRecord + BlockPayloadRecord`。
4. batch insert 到 focused block 后。
5. 更新 index/visible/page/height。

**验收标准**

- heading/list/table/code 都生成对应 block。
- 大 paste 不创建全部 GUI entity。

**验证**

```bash
cargo test paste_10k_markdown_blocks_batches_insert_and_hydrates_visible_first
```

---

# Phase 7：复杂 Block Renderer

目标：Code/Table/Image 不拖垮整页，使用内部虚拟化或 stable box。

## 7.1 CodeBlock renderer

- [ ] 7.1.1 新增 `src/gui/block_renderers/code_block.rs`。

**怎么做**

1. 输入 `ViewBlockSnapshot` / `BlockPayload::Code`。
2. 显示 language header。
3. 初始按 visible lines 渲染。
4. 长文本只渲染部分 lines。

**验收标准**

- 10MB code block 不全量创建 line elements。

**验证**

```bash
cargo test ten_mb_code_block_scroll_uses_line_virtualization
```

---

- [ ] 7.1.2 code block 内部滚动与 document scroll 分离。

**怎么做**

1. code block 自己维护 internal scroll offset。
2. wheel 先给 code block。
3. 到边界时把剩余 delta 交给 document scroll。

**验收标准**

- code block 内滚动不反向驱动 document scroll。

**验证**

```bash
cargo test code_block_internal_scroll_consumes_wheel_before_document
```

---

## 7.2 Table renderer

- [ ] 7.2.1 新增 `src/gui/block_renderers/table.rs`。

**怎么做**

1. 输入 `TablePayload`。
2. Header row 固定显示。
3. body rows 按 viewport 内部虚拟化。
4. cell 复用 `RichTextElement`。

**验收标准**

- 50k rows table 不全量渲染。

**验证**

```bash
cargo test table_50k_rows_scroll_uses_row_virtualization
```

---

- [ ] 7.2.2 table cell hit test / focus。

**怎么做**

1. 点击 cell 得到 row/col/offset。
2. runtime 保存 inner anchor。
3. 输入只改 focused cell payload。

**验收标准**

- 点击 cell 可输入。
- 输入不会改其他 cell。

**验证**

```bash
cargo test table_hit_test_returns_inner_cell
```

---

## 7.3 Image / Media renderer

- [ ] 7.3.1 Image renderer 使用 stable box。

**怎么做**

1. 未加载图片时使用 estimated size。
2. metadata 到达后更新 stable box。
3. 不因 decode 完成造成大跳。

**验收标准**

- image dense document 打开不全量 decode。

**验证**

```bash
cargo test open_image_dense_document_does_not_decode_or_hydrate_all_media
```

---

- [ ] 7.3.2 接 media cache。

**怎么做**

1. viewport 附近 decode thumbnail。
2. 远处只保留 metadata。
3. memory pressure 下释放 decoded bytes。

**验收标准**

- memory cache 在预算内。

**验证**

```bash
cargo test media_cache
```

---

# Phase 8：性能验收与门禁

## 8.1 10w blocks 首屏打开

- [ ] 8.1.1 新增 GUI/runtime acceptance test：100k blocks first screen。

**怎么做**

1. 构造 100k blocks runtime。
2. 调 projection_for_window。
3. 断言 projection blocks 数量小于阈值，例如 500。
4. 断言 payload hydration 只发生窗口附近。

**验收标准**

- 不全量 projection。
- 不全量 render。

**验证**

```bash
cargo test gui_open_100k_acceptance
```

---

## 8.2 连续滚动

- [ ] 8.2.1 trace replay 滚动窗口。

**怎么做**

1. 构造 scroll trace。
2. 每帧调用 runtime scroll。
3. projection window 应单调/稳定变化。
4. 记录 frame cost 模拟指标。

**验收标准**

- 无反向跳动。
- window boundary 不抖动。

**验证**

```bash
cargo test gui_scroll_trace_replay
cargo test continuous_10_minute_scroll_acceptance
```

---

## 8.3 输入热路径

- [ ] 8.3.1 GUI/runtime 输入 1000 chars 验收。

**怎么做**

1. focused block 输入 1000 chars。
2. 每次输入只更新当前 block payload/model。
3. 不触发全量 projection/full layout。

**验收标准**

- 当前 block pinned。
- input latency 预算通过。

**验证**

```bash
cargo test current_block_continuous_input_1000_chars_acceptance
cargo test typing_trace_replay_1000_chars_keeps_caret_stable_and_latency_budget
```

---

# 最终完成定义

全部完成时必须满足：

- [ ] 10w blocks 不全量 hydrate/render。
- [ ] GUI 只渲染 `RenderWindow`。
- [ ] 滚轮由 `VirtualScrollState` 驱动。
- [ ] 当前编辑 block pin。
- [ ] `RichTextElement` 支持 layout cache、wrapped line、caret geometry、hit test。
- [ ] `DocumentSelection` 不依赖 UI。
- [ ] IME composition 不丢。
- [ ] 高度测量能回写并保持 anchor。
- [ ] Code/Table/Image 复杂 block 使用内部虚拟化或 stable box。
- [ ] Markdown shortcut / paste / import 接入 runtime。
- [ ] 全量测试通过。

最终验证：

```bash
cargo test
```
