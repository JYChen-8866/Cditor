# Cditor 当前问题深度分析与可推进任务清单

> 审计日期：2026-07-12
> 审计范围：白板、gutter、block 选区、表格布局、inline Markdown、Mermaid、复杂 block 与总体架构的符合度。
> 架构基线：`doc/large-document-rich-text-architecture.md` 与 `cditor 开发助手`。
> 本文只做分析和实施规划，不包含问题修复。

## 0. 结论与建议顺序

当前问题不是彼此独立的样式瑕疵，背后集中在多个边界没有完全建立：

1. **文本 block 与复杂 block 没有严格区分输入能力。** 白板被放进通用 `BlockText` focus/Enter 链路，最终发生 payload 降级。
2. **gutter 的 click、drag、menu、block selection 共用一个临时 `action_block_id`。** 因而既无法稳定打开菜单，也无法表达“只选 block 内容、不选 gutter”。
3. **布局真相仍由 core/app 多组魔数共同推导。** 表格默认宽度和可用宽度相差约 2 px，复杂 block 高度还可能漏算边框占位。
4. **inline Markdown 的静态解析和在线增量转换混成了同一条“整行重解析”路径。** 新一轮转换会覆盖前一轮已经写入 spans、但源码 delimiter 已被删除的 marks。
5. **剪贴板与 selection 仍有临时双真相。** 富格式只存在当前 View 的内存缓存里；文本选区、整 block 选区和平台输入选区走不同命令分支，导致复制、选区绘制和删除行为不对称。

建议按以下顺序推进：

- **P0：先修数据破坏**：白板 Enter；补复杂 block 输入能力边界。
- **P0：再修格式丢失**：inline Markdown 增量 span splice。
- **P1：建立 gutter 交互状态机**：click/drag 分流、menu、block-only selection 一次完成。
- **P0：补齐 selection 删除语义**：文本 range、跨 block 文本、整 block 多选分别走明确 transaction。
- **P1：重做剪贴板 envelope**：系统只暴露纯文本，Cditor 通过 GPUI metadata 恢复富格式与 block 结构。
- **P1：统一表格布局契约**：同时修默认宽度与最后一行边框。
- **P1：对白板做 trace 驱动性能治理**：先量化 host 集成开销，再拆更新/持久化频率。
- **P2：接入 Mermaid 原生渲染**：先完成许可证决策和独立 compile spike，再接异步缓存管线。
- **P2：处理横向架构债务**：复杂 block model 接入、selection 单真相、persistence 边界、超大文件拆分。

## 1. 审计证据与当前验证结果

### 1.1 已执行的只读验证

- [x] 阅读总体大文档架构、模块拆分计划、白板集成架构。
- [x] 检查两张截图的 gutter 与选区边界。
- [x] 跟踪白板点击、focus、Enter、payload split 的完整调用链。
- [x] 跟踪 gutter mouse-down、drag threshold、mouse-up commit 与 action projection。
- [x] 核对表格默认列宽、block shell 几何、overlay outline 几何。
- [x] 核对 inline Markdown 静态 parser 与在线 shortcut 的两条路径。
- [x] 扫描所有 Rust 文件行数和关键架构依赖。

本轮通过的相关测试：

```text
cditor-core rich_text::markdown                  24 passed
cditor-runtime rich_text_edit                    15 passed
cditor-runtime table::layout                      4 passed
cditor-app gui::block::table                     40 passed
cditor-app gui::block::whiteboard                 3 passed
```

针对本轮新增的 clipboard/selection/delete 审计，补充验证了：

```text
cditor-runtime selection_scroll                  15 passed
cditor-runtime delete_navigation_height          18 passed
cditor-runtime conversion_clipboard_media        11 passed
cditor-app gui::input::clipboard                  1 passed
cditor-app gui::app::input::keyboard              4 passed
```

测试通过并不表示现象不存在：现有测试主要验证纯函数和静态 parser，缺少用户实际触发的“连续输入/GUI 组合链路”和像素级渲染验收。当前还有一个 `flush_plain_span` dead-code warning。

### 1.2 风险分级

| ID | 问题 | 等级 | 主要风险 |
| --- | --- | --- | --- |
| WB-ENTER | 白板预览点击后回车导致 payload 被破坏 | P0 | 数据破坏、持久化后难恢复 |
| MD-INLINE | 相邻/嵌套 inline Markdown 丢 mark | P0 | 用户内容语义被静默改写 |
| MULTI-DELETE | 单行/跨行/多 block 选区无法一致删除 | P0 | 核心编辑命令失效或误删结构 |
| GUTTER-MENU | gutter 单击没有 Notion 菜单 | P1 | 核心 block 操作入口缺失 |
| GUTTER-SELECT | 选区包含 gutter/子树，语义不清 | P1 | 视觉与 selection truth 不一致 |
| CLIPBOARD | 应用内粘贴丢样式或误用旧样式 | P1 | 富文本/结构复制不可靠 |
| MULTI-SELECT-GEOMETRY | 跨 block 中间 fragment 覆盖 gutter | P1 | 视觉选区范围错误 |
| TABLE-BORDER | 最后一行选框下边框被覆盖 | P1 | 布局高度/overlay 几何不一致 |
| TABLE-WIDTH | 默认 3×3 表格略超编辑器 | P1 | 默认内容即产生横向溢出 |
| WB-PERF | 集成白板显著卡于 standalone | P1 | 复杂 block 无法产品化 |
| MERMAID | 当前 Mermaid 只显示代码，Zed 新 renderer 不能原样无审计搬入 | P2 | GPL、GPUI 版本和同步 CPU 渲染风险 |
| ARCH | 多个模块未完全符合架构/技能 | P2 | 后续功能重复制造同类缺陷 |

## 2. WB-ENTER：白板回车后只剩骨架和 `whiteboard`

### 2.1 已确认的根因

这是确定的类型边界错误，不是白板渲染失败。

当前调用链如下：

```text
单击白板预览
  -> block_shell 的通用 on_mouse_down
  -> focus_block_from_gui_at_position
  -> DocumentRuntime::focus_block_at_offset
  -> focus_block
  -> 无条件创建 InputTarget::BlockText

按 Enter
  -> GuiInputCommand::HandleEnter
  -> DocumentRuntime::handle_enter
  -> 只对 Table 做了特殊分支
  -> Whiteboard 落入 split_focused_block_at_caret
  -> split_payload_for_enter 的 other 分支
  -> WhiteboardPayload::plain_text() == "whiteboard"
  -> 原白板 payload 被 RichText payload 替换，新建 paragraph 获得 "whiteboard"
```

关键证据：

- `crates/core/src/rich_text/payload.rs:60` 把白板的通用纯文本投影写成固定字符串 `whiteboard`。
- `crates/runtime/src/document_runtime/focus.rs:21-35` 对任意 block 都创建 `InputTarget::BlockText`。
- `crates/runtime/src/document_runtime/structure_edit.rs:75-142` 的 Enter 逻辑只保护了 Table。
- `crates/runtime/src/document_runtime/text_payload.rs:377-385` 会把所有未显式支持的 payload 转成 plain text 后拆分。
- 拆分时原 record 的 `kind` 仍可能是 `Whiteboard`，但 payload 已变成 `RichText`，形成 kind/payload 不变量破坏；这解释了“空白板骨架 + 另一个 whiteboard 文本 block”。

### 2.2 正确设计

不要继续给 `handle_enter` 增加零散的 `if Whiteboard`。应先建立 block 输入能力：

```rust
enum BlockInputCapability {
    Text(TextInputCapability),
    TableCell,
    ComplexBlock,
    Atomic,
    None,
}

enum InputTarget {
    BlockText { block_id: BlockId },
    TableCell { block_id: BlockId, row: usize, col: usize },
    ComplexBlock { block_id: BlockId },
    BlockChrome { block_id: BlockId },
}
```

白板预览单击应聚焦 block chrome/complex block，而不是伪造文本 caret。其 Enter 默认语义建议为“在白板后插入 paragraph 并聚焦”，双击才进入白板编辑器。

此外要建立不可破坏不变量：`RichBlockKind::Whiteboard` 只能搭配 `BlockPayload::Whiteboard`。通用 split/merge/replace 不得把未知复杂 payload 静默转成 RichText。

### 2.3 实施任务

- [ ] **WB-ENTER-01** 在 core/runtime 增加 block input capability 映射，并覆盖所有 `RichBlockKind`。
- [ ] **WB-ENTER-02** 扩展 `InputTarget`，使非文本复杂 block 不再进入 `BlockText`。
- [ ] **WB-ENTER-03** 修改预览 block 的 focus：单击只产生 block/complex focus，不建立文本 caret。
- [ ] **WB-ENTER-04** 定义 atomic/complex block 的 Enter、Shift+Enter、Backspace、Delete、方向键语义。
  - [ ] Enter：在当前 block 后插入 paragraph。
  - [ ] Shift+Enter：不修改复杂 payload；可选择无操作。
  - [ ] Backspace/Delete：只在 block-level selection 明确时删除，避免“空文本模型”推断删除。
  - [ ] 上/下方向键：导航到相邻可编辑 block 或 block chrome。
- [ ] **WB-ENTER-05** 给 `split_payload_for_enter` 增加 fallible/typed API；复杂 payload 返回 `UnsupportedSplit`，禁止 `other.plain_text()` 降级。
- [ ] **WB-ENTER-06** 在 payload 写入入口增加 kind/payload 配对校验，加载旧数据时提供显式 recovery，而不是继续运行。
- [ ] **WB-ENTER-07** 确保该操作形成正确结构事务、selection 和 scroll anchor，不直接只改 UI/payload window。

### 2.4 测试与验收

- [ ] runtime 单测：白板 focus 后 Enter，原 `scene_json`、kind、content version 保持，后方新增空 paragraph。
- [ ] runtime 参数化单测：Image/File/Embed/Divider/Database/Whiteboard 均不会被 Enter 转成 RichText。
- [ ] property test：任何合法 kind/payload 经过 focus + 非法文本命令后仍保持 kind/payload 配对不变量。
- [ ] GUI 集成测试：单击预览 + Enter，不打开白板 editor，不破坏缩略图，caret 落到新 paragraph。
- [ ] persistence 回归：保存、重载后白板 scene 完整。
- [ ] 手工验收：空白板、有元素白板、只读模式、白板位于文档首/尾四种情况。

## 3. GUTTER-MENU：单击 gutter 打开 Notion 风格菜单

### 3.1 当前缺口

目前 gutter handle 只有 mouse-down 回调。`gutter_mouse_down_from_gui` 同时：

- 设置 `action_block_id`；
- 创建 `GutterBlockDragState`；
- 把 block 放进通用文本 focus；
- 等待全局 mouse-up 提交 drag。

如果没有超过 4 px drag threshold，`commit_gutter_block_drag` 直接清掉 action 并返回。代码中没有 block menu state、anchor rect、click release 分支或 menu overlay，所以单击不可能稳定打开菜单。

### 3.2 推荐状态机

```text
Idle
  -- mouse-down --> Pressed { block_id, start, anchor_rect }

Pressed
  -- move < threshold --> Pressed
  -- move >= threshold --> Dragging
  -- mouse-up < threshold --> MenuOpen

Dragging
  -- move --> update target / auto-scroll
  -- mouse-up --> commit move -> Idle

MenuOpen
  -- menu action --> execute command -> Idle
  -- Escape / outside click / scroll-window change --> Idle
```

不要在 mouse-down 当场打开菜单，否则正常拖拽会闪出菜单；单击语义应在 mouse-up 且未越过 threshold 时确认。

菜单数据建议与渲染解耦：

```rust
struct BlockMenuState {
    block_id: BlockId,
    anchor: OverlayAnchor,
    query: String,
    highlighted: usize,
}

enum BlockCommand {
    ConvertTo(RichBlockKind),
    SetColor(BlockColor),
    CopyLink,
    Duplicate,
    MoveTo,
    Delete,
    Comment,
    SuggestEdit,
}
```

第一阶段应实现截图中的核心项，但 command enablement 必须基于 block kind、readonly、selection、加载状态统一计算，不能在菜单 view 中直接改 runtime。

### 3.3 实施任务

- [ ] **GUTTER-MENU-01** 把 `gutter_block_drag` 重构成上述明确状态机。
- [ ] **GUTTER-MENU-02** 在 geometry 层记录 gutter handle 的实际 window bounds，作为 overlay anchor。
- [ ] **GUTTER-MENU-03** 新增 `BlockMenuState` 与独立 overlay 模块，支持 viewport clamp/上下翻转。
- [ ] **GUTTER-MENU-04** 建立 `BlockCommand` registry：label、icon、shortcut、enabled、danger、keywords 与执行函数分离。
- [ ] **GUTTER-MENU-05** 实现搜索、键盘上下选择、Enter、Escape、外部点击关闭。
- [ ] **GUTTER-MENU-06** 菜单打开期间 pin 对应 block/page；window planner 不得把 menu anchor block evict。
- [ ] **GUTTER-MENU-07** 菜单 action 全部经过 runtime transaction；Delete/Duplicate/Move/Convert 支持 undo/redo 与持久化。
- [ ] **GUTTER-MENU-08** readonly 和 payload loading/error 状态有明确降级，不显示不可执行的假按钮。

### 3.4 测试与验收

- [ ] 状态机单测：0/3.9/4.0/10 px 移动分别进入正确状态。
- [ ] GUI 测试：单击打开一次，拖拽从不打开，双击不重复创建 overlay。
- [ ] menu 定位测试：文档顶部/底部、窗口左右边缘、缩放/DPI、缩进 list block。
- [ ] 命令矩阵测试：每种 block kind、readonly、loading、根/子 block。
- [ ] 虚拟化测试：菜单打开后滚动窗口，anchor pin 或显式关闭，不悬浮到错误 block。
- [ ] 手工验收：菜单视觉、hover、危险操作、快捷键与截图目标一致。

## 4. GUTTER-SELECT：只选 block，不把 gutter 算入选区

### 4.1 当前根因

当前 `BlockActionState::action_active` 同时驱动：

- 整个 shell 的 background/border；
- content container 的 background/border；
- source block 的整个 subtree 高亮。

`block_shell` 的 outer shell 包含 gutter，因此 outer background 变色时 gutter 自然处于选区中。`block_action_state_for_projection` 又把 action source 到 subtree end 全部设为 active，所以“菜单 action root”“拖拽 subtree preview”“block-level selection”三个概念被压成一个布尔值。

### 4.2 正确设计

拆开三种视觉状态：

```rust
struct BlockVisualState {
    selected: bool,          // 文档级 block selection，只画 content region
    menu_anchor: bool,       // gutter/menu 保持可见，不代表选区范围
    drag_source: bool,       // 拖拽源，可包含 subtree preview
    drag_descendant: bool,
}
```

block selection layer 应从 `content origin x` 开始，覆盖 prefix + payload 内容，但 gutter handle 保持透明并绘制在 selection layer 上方。选区几何应共享 `BlockChromeMetrics`，不能再在 CSS 和 hit-test 中各算一遍。

### 4.3 实施任务

- [ ] **GUTTER-SELECT-01** 将 menu anchor、block selection、drag subtree projection 拆成独立状态。
- [ ] **GUTTER-SELECT-02** gutter 单击在 runtime 建立明确的 block selection，而不是文本 caret。
- [ ] **GUTTER-SELECT-03** 选区背景只挂在 content container/独立 paint-only overlay，outer shell 与 gutter 保持 page background。
- [ ] **GUTTER-SELECT-04** 只在拖拽 source subtree 时展示 descendant preview；普通单击只选一个 block。
- [ ] **GUTTER-SELECT-05** 定义 Shift-click/Cmd-click 多 block selection 与 menu 对多选的作用范围。
- [ ] **GUTTER-SELECT-06** block selection 进入统一 selection truth，copy/cut/delete 不依赖当前 UI entity。

### 4.4 测试与验收

- [ ] geometry 单测：selection rect 的 left 等于 content origin，不包含 gutter 24 px 与 row gap。
- [ ] screenshot/golden：普通 paragraph、list child、quote、table、whiteboard 五种 block。
- [ ] 虚拟化测试：跨 window block selection 只由 runtime/index 计算。
- [ ] 验收：截图红框区域内为 block selection，左侧六点 gutter 明确在区域外。

## 5. WB-PERF：白板集成后卡，standalone 流畅

### 5.1 当前最可能的差异链路

`ding-board` standalone 和 Cditor overlay 使用同一个 `WhiteboardView`，但 host 回调完全不同。集成模式每次 board `flush` 都会：

```text
Scene::to_json（全 scene 序列化）
  -> host CditorV2View.update
  -> runtime.update_whiteboard_scene_json（复制/比较/替换大 String）
  -> content_version + 1
  -> mark_dirty
  -> 调度 Postgres debounce
  -> host cx.notify（重绘整个 Cditor view/projection/overlays）
```

`crates/ding-board/src/lib.rs:3233-3240` 会在变更 flush 时做全量 `scene.to_json()`；`crates/app/src/gui/app/cditor_v2_view/whiteboard.rs:40-53` 又在每次回调更新 host runtime 并 mark dirty。standalone 没有 Cditor 的 projection、虚拟窗口、缩略图、保存状态和根 view 重绘成本。因此“单独流畅、集成卡”首先应怀疑 host invalidation 和全量 snapshot 频率，而不是先改 canvas 绘制。

白板自身仍有第二层成本：render 时会扫描 elements、建立 visible id set、retain caches、再构造 layers；文件过大也让优化边界不清晰。但这部分在 standalone 同样存在，应通过对比 trace 决定优先级。

### 5.2 必须先加的性能证据

- `board_pointer_to_paint_ms`：指针事件到 board paint。
- `board_render_ms`：白板自身 render/paint。
- `scene_serialize_ms/bytes`：JSON 序列化耗时和大小。
- `host_callback_ms`：runtime update + dirty scheduling。
- `host_render_ms`：Cditor 根 view render/projection。
- `save_batch_ms/bytes`：后台 snapshot 与 Postgres。
- 帧指标：p50/p95/p99、dropped frame ratio；至少按 100/1k/10k elements 分组。

### 5.3 推荐实现方向

白板交互期间采用 session-local truth，文档 runtime 只接收节流后的 snapshot 或 commit：

```text
pointer move
  -> board scene in-memory mutation
  -> board-only notify/paint
  -> 不更新 Cditor root，不持久化

idle 100~250ms / pointer-up / explicit close
  -> 生成一次 scene snapshot
  -> runtime complex-block transaction
  -> 异步持久化
```

长期可把 `scene_json: String` 升级为 opaque scene snapshot + optional delta：Cditor core 不理解 scene 类型，但 runtime 能以 `ComplexBlockPatch` 管理 version、undo boundary 和 persistence coalescing。

### 5.4 实施任务

- [ ] **WB-PERF-01** 建立 standalone/Cditor 同场景 benchmark fixture，固定元素数量与交互 trace。
- [ ] **WB-PERF-02** 加上述分段 telemetry，先输出对比报告再改代码。
- [ ] **WB-PERF-03** board entity 的局部 repaint 与 host root notify 解耦；普通 pointer move 不触发 Cditor render。
- [ ] **WB-PERF-04** 将 `on_change` 改为 `on_dirty` + debounced snapshot/`on_commit`，pointer-up/close 强制 flush。
- [ ] **WB-PERF-05** scene JSON 序列化移出交互帧；用 generation 防止旧 snapshot 覆盖新 scene。
- [ ] **WB-PERF-06** runtime whiteboard 更新形成合并 transaction，不为每个 move 增 content version。
- [ ] **WB-PERF-07** autosave 只保存 dirty block，避免每次 `loaded_payload_records_snapshot()` 抓整个 payload window。
- [ ] **WB-PERF-08** 基于 trace 再优化 board render：空间索引、stable visible set、layer cache、dirty-region paint。
- [ ] **WB-PERF-09** 关闭 editor 时同步保证最终内存 snapshot 已提交；数据库保存仍异步且可恢复。

### 5.5 性能验收

- [ ] 1k elements 连续拖拽：p95 main-thread work < 8 ms，p99 < 16 ms，掉帧率 < 1%。
- [ ] pointer move 热路径不出现 `Scene::to_json`、Postgres snapshot 或 Cditor 全文 projection。
- [ ] 关闭 editor、应用退出、保存失败重试均不丢最后一次修改。
- [ ] 同一 trace 下，集成模式 p95 不超过 standalone 的 1.25 倍；若超过，trace 必须能指明 host 成本。

## 6. TABLE-BORDER：最后一行选框下边框被覆盖

### 6.1 根因判断

这是高度 truth 和 overlay box 几何没有使用同一份 chrome metrics 的高概率问题。

当前 table payload 高度为：

```text
table rows height
+ COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX (16)
+ TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX (14)
```

但 GUI shell 还存在：outer `py(4)`、outer border、content `py(4)`、content border。16 px 只明显覆盖了两组上下 padding（8 + 8），边框是否计入 layout box 依赖 GPUI box sizing；当前 core 与 GUI 没有共享可验证的完整公式。少算的 2~4 px 会让下一 block 的绝对层覆盖 table 最底部 overlay。

另外 `table_axis_selection_outline_rect`/`table_range_selection_outline_rect` 先扩 `half = 1px`，随后又把 bottom clamp 到 `table_view.height_px`，但 render 使用 2 px border box。该算法没有明确 border 是内描边、居中还是外扩，最后一行是唯一会触发 bottom clamp 的位置。

### 6.2 正确实现

定义一份可序列化、可测试的 `BlockChromeMetrics` / `TableLayoutBox`：

```rust
struct TableLayoutBox {
    grid: Rect,
    selection_clip: Rect,
    scrollbar: Rect,
    outer_height: f64,
}
```

runtime height index、GUI block absolute height、table overlay origin、selection clipping 都消费这份结果。选框应明确使用 inner stroke 或为外描边预留 overlay padding，不能靠 `+half` 后再 clamp 猜测。

### 6.3 实施任务

- [ ] **TABLE-BORDER-01** 在 debug overlay 输出 predicted block height、measured shell height、grid/selection rect。
- [ ] **TABLE-BORDER-02** 记录最后一行选中时四个 bottom 值，确认是 block height 截断还是 stroke clamp。
- [ ] **TABLE-BORDER-03** 把 shell padding/border/table scrollbar 统一进共享 metrics，删除 `16` 这类无组成信息的魔数。
- [ ] **TABLE-BORDER-04** 明确 selection stroke policy（推荐 inner stroke），重写 outline rect 计算。
- [ ] **TABLE-BORDER-05** measured height 回写后走一次 height correction + anchor restore，禁止 UI 靠 overflow 掩盖差值。

### 6.4 测试与验收

- [ ] 纯几何测试：首/中/末行，单行、多行、range、axis selection 的四边均可见。
- [ ] DPI 测试：1x/1.5x/2x 下 round 到 device pixel 后无 1 px 缺边。
- [ ] screenshot/golden：选最后一行时蓝色下边框完整且不被下一 block 覆盖。
- [ ] 高度不变量：`measured_outer_height == layout_box.outer_height`，误差 <= 0.5 device px。

## 7. TABLE-WIDTH：默认 3×3 表格略超编辑器宽度

### 7.1 已确认的根因

默认列宽在 runtime 中由 `DEFAULT_LAYOUT_WIDTH_PX = 812` 平分成 3 个固定 `Px` track；而 GUI 根 block 的实际文本/复杂内容宽度按当前 shell 公式约为：

```text
860 page width
- 8 left shell padding
- 24 gutter
- 8 row gap
- 1 content left border
- 8 right shell padding
- 1 content right border
= 810 px
```

因此默认表格约比 content box 多 2 px，和“超出一点”的现象一致。更深层问题是：

- core 的 `DEFAULT_LAYOUT_WIDTH_PX`；
- app 的 860/8/24/8/1 等 chrome 常量；
- table 的固定列宽；
- list depth 的缩进；

没有通过统一 `LayoutContext.available_width` 汇合。当前 `TableLayoutInput.available_width_px` 已存在，但 projection 使用 `table_layout_from_payload`，实际总是 `available_width_px = None`；已有“分配额外宽度”的能力没有接入在线路径。

### 7.2 推荐实现

- 新表格列使用 `Auto`，而不是把某个历史默认宽度固化成三份 `Px`。
- projection/layout 必须拿到当前 block 的 available content width，考虑 page width、shell、depth、prefix 与 scrollbar policy。
- 用户手动 resize 后该列才转成 `Px`；Auto 列分配剩余宽度。
- 窗口更窄时允许内部横向滚动，但默认新建表格在正常 page width 下应无滚动条。

### 7.3 实施任务

- [ ] **TABLE-WIDTH-01** 建立共享 `LayoutContext { viewport_width, content_width, depth, scale, layout_version }`。
- [ ] **TABLE-WIDTH-02** 将 block available width 从 runtime/window plan 传入 table layout/projection。
- [ ] **TABLE-WIDTH-03** 新建表格默认 3 个 `Auto` 列；迁移旧的 812/3 默认值时保持用户已 resize 的 Px 列。
- [ ] **TABLE-WIDTH-04** 接入 `TableLayoutInput.available_width_px`，同时支持 Auto 填满、Px 溢出、混合 track。
- [ ] **TABLE-WIDTH-05** depth/页面 resize/字体或 DPI 改变时提升 `layout_version` 并重算 table geometry。
- [ ] **TABLE-WIDTH-06** 默认 fit 与横向滚动使用同一 viewport measurement，避免 scrollbar 自己成为第二宽度 truth。

### 7.4 测试与验收

- [ ] root 3×3 默认表格：width 精确等于 available width，无横向 scrollbar。
- [ ] depth 1/2/3 的表格：宽度随缩进收缩，不越界。
- [ ] Auto/Px 混合列、全部 Px 列、超窄窗口、resize 后重载。
- [ ] property test：不溢出策略下 `sum(columns) <= available_width`；显式 Px 溢出时 scrollbar max 与差值一致。

## 8. MD-INLINE：相邻/嵌套 Markdown marks 丢失

### 8.1 已确认的根因

静态 parser 对 `**asd**~~ad~~` 已有测试并通过，所以单纯再加 delimiter pairing case 不能修用户现象。

真正问题是在线输入过程：

```text
输入 **asd**
  -> parser 解析整行
  -> replace_block_kind_and_spans
  -> text model 变为 "asd"，payload span 保留 Bold

继续输入 ~~dasd~~
  -> parser 只看到纯文本 "asd~~dasd~~"
  -> 它无法知道 "asd" 之前已有 Bold（delimiter 已在上次转换时删除）
  -> 新 spans 整体替换旧 spans
  -> Bold 丢失
```

嵌套 italic 同理：一旦外层或内层先被增量消费，下一轮整行源码已经不完整。当前 parser 还有 delimiter claimed-byte 的自制算法，没有完整实现 CommonMark flanking、rule-of-three、escape 等规则；但用户这个具体问题首先是 transaction/splice 错误。

### 8.2 正确设计

在线 shortcut 应只转换“刚刚闭合的 delimiter 区域”，然后把结果 splice 回现有 rich spans：

```rust
struct InlineShortcutEdit {
    source_range: Range<usize>,
    replacement_spans: Vec<InlineSpan>,
    caret_after: usize,
}
```

算法要点：

1. 根据 caret 向左寻找当前刚闭合的 delimiter pair，而不是重 parse 整个 block 后整体替换。
2. 从现有 spans 切出 `source_range`，解析该范围。
3. 新 mark 与该范围原有 marks 做有序 union；范围外 spans 原样保留。
4. 一次 transaction 同时更新 text model、payload spans、caret、selection 和 undo。
5. IME composition 期间不触发 shortcut；commit 后再做一次局部检测。

若要达到完整 Markdown 兼容，建议采用事件/token 栈或经过验证的 CommonMark delimiter algorithm，不继续堆最长优先的 substring 搜索。

### 8.3 实施任务

- [ ] **MD-INLINE-01** 先加连续逐字符输入回归测试，复现 mark 丢失，禁止只调用静态 parser。
- [ ] **MD-INLINE-02** 定义 `InlineShortcutEdit` 与 span range splice 基础设施。
- [ ] **MD-INLINE-03** 在线 shortcut 改为 caret-local delimiter detection，不再 `replace_block_kind_and_spans` 整行覆盖。
- [ ] **MD-INLINE-04** 明确定义已有 mark 与新 mark 的合并/切换语义，保证 marks 顺序稳定、可 merge。
- [ ] **MD-INLINE-05** 增加 escape、标点 flanking、Unicode/CJK、相邻 delimiter、三连 delimiter 规则。
- [ ] **MD-INLINE-06** IME、selection replace、paste、undo/redo 共用同一个 rich span transaction。
- [ ] **MD-INLINE-07** 删除 dead helper/warning，并把 parser 按 tokenizer、delimiter resolver、span builder、shortcut splice 拆分，避免 `inline.rs` 继续逼近 700 行。

### 8.4 必测矩阵

- [ ] 逐字符输入：`**asd**~~dasd~~`，结果为 Bold(asd) + Strike(dasd)。
- [ ] 逐字符输入：`**bold *italic* bold**`，嵌套区同时含 Bold/Italic。
- [ ] `***both***`、`**a****b**`、`~~a~~**b**`、`**a**~~b~~*c*`。
- [ ] CJK：`**中文**~~删除~~`；emoji ZWJ；组合音标；RTL/LTR。
- [ ] escaped delimiter、inline code 内 delimiter、link label 与已有 marks。
- [ ] caret 在中间输入、selection replace、Backspace 删除闭合符、undo/redo 恢复 marks 与 caret。
- [ ] IME composition 内输入星号不抢跑，commit 后结果稳定。

## 8A. CLIPBOARD：应用内保留样式，其他应用只收到纯文本

### 8A.1 当前实现与确定缺口

当前复制会把纯文本写入系统剪贴板，同时把富文本快照存在当前 `CditorV2View.internal_clipboard`：

```text
Copy
  -> selected_focused_text() -> ClipboardItem::new_string(text)
  -> selected_focused_rich_text() -> self.internal_clipboard

Paste
  -> 读取系统纯文本
  -> 如果纯文本与 internal_clipboard.plain_text 相同
  -> 猜测它仍是同一次 Cditor 复制，恢复 spans/table
```

关键证据：

- `crates/app/src/gui/input/clipboard.rs:35-40` 只用字符串相等判断内部富快照是否仍有效。
- `crates/app/src/gui/app/input/keyboard.rs:158-171` 系统剪贴板只写 `ClipboardItem::new_string`，富内容仅留在 View 内存。
- `crates/app/src/gui/app/input/keyboard.rs:369-400` 粘贴时依赖内存快照和纯文本匹配。
- `crates/runtime/src/document_runtime/selection.rs:205-224` 对跨 block 文本选区直接返回 `None`，所以跨 block 复制只可能得到纯文本。
- 整 block selection 使用 `selected_block_ids`，但 Copy/Cut 只查询 `selected_focused_text`，不会生成 block snapshot。

因此当前实现存在四个确定问题：

1. **跨窗口/跨 Cditor 实例失败**：富快照属于单个 View，不在系统 clipboard item 中。
2. **同文本误判**：用户在外部应用重新复制相同文字，Cditor 仍可能粘贴旧的 Bold/Link/Table 样式。
3. **跨 block 丢结构/样式**：`selected_focused_rich_text` 明确拒绝跨 block selection。
4. **整 block 多选无法复制/剪切**：命令没有读取 `selected_block_ids` 的路径。

GPUI 当前版本已经提供所需机制：`ClipboardItem::new_string_with_json_metadata(text, metadata)`。它仍以普通 String 向系统提供纯文本，同时在 GPUI clipboard metadata 中携带 Cditor 私有 JSON。其他应用读取到的是纯文本；Cditor 再粘贴时可以校验 metadata 并恢复富格式，无需字符串猜测。

### 8A.2 推荐 clipboard envelope

```rust
#[derive(Serialize, Deserialize)]
struct CditorClipboardEnvelope {
    schema: String,          // "application/x-cditor-clipboard"
    version: u16,
    source_document: Option<DocumentId>,
    selection: ClipboardSelection,
    checksum: u64,
}

enum ClipboardSelection {
    Inline {
        spans: Vec<InlineSpan>,
    },
    TextFragments {
        fragments: Vec<RichTextBlockFragment>,
    },
    Blocks {
        roots: Vec<ClipboardBlock>,
    },
    Table(TableClipboardSnapshot),
}

struct RichTextBlockFragment {
    kind: RichBlockKind,
    attrs: BlockAttrs,
    spans: Vec<InlineSpan>,
    boundary: FragmentBoundary,
}
```

系统可见文本始终单独生成：

```text
Inline/TextFragments -> plain text，用 \n 连接 block
Blocks               -> 可读的纯文本/Markdown 降级，但不把 HTML/Rich Text 暴露给外部应用
Table                -> TSV 或当前约定的纯文本表格
```

Cditor paste 优先级建议：

```text
可信且版本兼容的 Cditor metadata
  -> 按目标上下文粘贴 Inline / Blocks / Table
否则
  -> image / external paths
  -> 系统纯文本
  -> 明确的 Markdown paste（仅在产品规则允许时）
```

metadata 只表示结构化数据，不应信任其中的本地路径、URL scheme、超大 payload 或未知 block kind；必须走已有 external content security policy 和大小限制。

### 8A.3 实施任务

- [ ] **CLIPBOARD-01** 在 core/runtime 定义带 schema/version 的 `CditorClipboardEnvelope`，补 serde 与兼容性策略。
- [ ] **CLIPBOARD-02** 把单 block inline、跨 block text fragments、整 block roots、table 四类 selection 序列化统一到 envelope。
- [ ] **CLIPBOARD-03** Copy/Cut 使用 `ClipboardItem::new_string_with_json_metadata`，系统 text 字段只放纯文本。
- [ ] **CLIPBOARD-04** Paste 从 `ClipboardString.metadata_json` 解码；删除 `matches_system_text` 及 View-local 猜测逻辑。
- [ ] **CLIPBOARD-05** 跨 block text copy 保留首尾 partial spans、中间 full block spans、kind、attrs 和 block boundary。
- [ ] **CLIPBOARD-06** 整 block copy 保留 subtree、顺序和复杂 block opaque payload；粘贴时重新分配 block id/parent id。
- [ ] **CLIPBOARD-07** 明确目标上下文适配：文本 caret、block selection、table cell、whiteboard editor、readonly。
- [ ] **CLIPBOARD-08** Cut 必须采用“成功写 clipboard 后再提交 delete transaction”；写入失败不得删除原内容。
- [ ] **CLIPBOARD-09** 加 metadata 大小、版本、checksum、未知 kind、恶意 URL/path 校验；失败安全降级为纯文本。
- [ ] **CLIPBOARD-10** 如平台会剥离 metadata，保持纯文本 fallback；跨窗口/跨实例是否保留富格式纳入平台验收。

### 8A.4 测试与验收

- [ ] 单 block Bold/Italic/Strike/Link/Code 混合复制，在 Cditor 内粘贴完整保留 marks。
- [ ] 同一段内容复制到 TextEdit/终端/浏览器，只出现纯文本和换行，不出现 JSON/HTML/Markdown delimiter。
- [ ] 外部应用重新复制相同文字后回 Cditor，不能误用旧富样式。
- [ ] Cditor 两窗口、两个 document、应用重启边界按平台能力验证。
- [ ] 跨 block partial/full/partial 复制，Cditor 内恢复 block boundary 与每段 marks。
- [ ] 整 block 多选包含 list subtree、table、whiteboard 时可复制、粘贴、undo/redo。
- [ ] metadata 损坏、超限、未知版本、系统只返回 text 时全部安全降级。
- [ ] Cut 的 clipboard write 失败测试：文档内容保持不变。

## 8B. MULTI-SELECT-GEOMETRY：跨 block 中间 fragment 把 gutter 一起选中

### 8B.1 已确认的根因

截图中的蓝色区域来自 document-level text selection 的中间 full fragment，而不是普通文字排版选区：

```text
DocumentSelection
  -> visible selection fragments
  -> 中间 block = SelectionRange::Full
  -> ViewBlockSnapshot.selected = true
  -> selection_overlay_fragments(full block)
  -> overlay left = 0, right = 0
```

`crates/app/src/gui/overlay/selection_overlay.rs:47-53` 对所有 full block fragment 从 editor 内容列的 `x=0` 画到 `right=0`。这个范围包含 shell 左 padding、缩进槽、gutter 和 row gap，因此中间 block 的 gutter 被一起染色。端点 block 使用 text element 的 platform range bounds，只画文字行，所以截图中只有中间 full fragment 特别宽。

这和 GUTTER-SELECT 的“整 block 选择不要包含 gutter”共享同一几何原则，但数据来源不同：

- gutter 单击：`Blocks` selection；
- 鼠标跨 block 拖选：`Text` selection，中间 block 产生 full text fragment。

两者不能继续共用只有 `y/height/full_block` 的 overlay fragment。

### 8B.2 正确 fragment 模型

```rust
enum SelectionOverlayGeometry {
    TextSegments(Vec<Rect>),
    FullTextContent {
        content_left: f32,
        content_right: f32,
        top: f32,
        bottom: f32,
    },
    WholeBlockContent(Rect),
}

struct SelectionOverlayFragment {
    block_id: BlockId,
    selection_kind: SelectionKind,
    geometry: SelectionOverlayGeometry,
}
```

跨 block 文本选区的中间 fragment 应覆盖“可复制的文本内容区域”，从 text/content origin 开始；不覆盖 gutter、菜单 handle、外层缩进空白。list bullet/todo prefix 是否着色需产品明确，建议：文本选择只覆盖实际文本与行内 prefix，不覆盖 gutter；整 block selection 覆盖 prefix + content，但仍不覆盖 gutter。

### 8B.3 实施任务

- [ ] **MULTI-SELECT-GEO-01** 给 projection fragment 增加 selection kind，不再用 `block.selected` 混合 Text Full 与 Blocks selection。
- [ ] **MULTI-SELECT-GEO-02** overlay fragment 携带 x/width 或共享 `BlockContentRect`，删除 `left(0)/right(0)`。
- [ ] **MULTI-SELECT-GEO-03** 中间 text fragment 使用 text/content bounds；gutter、row gap、indent blank 永远排除。
- [ ] **MULTI-SELECT-GEO-04** 明确 list prefix、quote bar、table/whiteboard 等 complex block 穿越时的视觉与复制语义。
- [ ] **MULTI-SELECT-GEO-05** fragment 只从 DocumentSelection + DocumentIndex 推导，UI entity 不作为 selection truth。
- [ ] **MULTI-SELECT-GEO-06** 与 GUTTER-SELECT 共用几何 contract，但保持 Text/Blocks 两种 selection 的不同宽度策略。

### 8B.4 测试与验收

- [ ] screenshot：同一行 selection、同 block 跨行、跨 3 个 paragraph、反向跨 block。
- [ ] 中间 fragment 的 left 必须大于 gutter + gap 右边界；gutter 六点区域保持 page background。
- [ ] list depth 0/1/2、todo、quote、callout、code、table、whiteboard 混合选区。
- [ ] 1x/1.5x/2x DPI 和滚动后的 virtual window 坐标测试。
- [ ] selection 视觉覆盖的文本与 Copy 产生的 plain text/rich fragments 一致。

## 8C. MULTI-DELETE：单行、跨行和多 block 选择后的删除逻辑

### 8C.1 三条删除路径的现状

#### A. 同一 text block 内的单行/跨行 selection

模型层已有删除能力：`focused_text_selection_range` 会被 `replace_text_in_focused_range(None, "")` 消费。现有单测覆盖了同 block range replace，但没有覆盖真实 GUI 的 mouse-down → drag → mouse-up → Backspace/Delete 序列。

因此如果同一 block 的 GUI 选择仍无法删除，优先检查：

- `document_selection`；
- `focused_text_selection`；
- `EditingSession.selected_range`；
- platform UTF-16 selection；

是否在 mouse-up、focus 切换或 platform input 回调后发生分叉。不能再通过增加第四份 selection 状态修补。

#### B. 跨 text block selection

Backspace 路径会先检查 non-caret `document_selection` 并调用 `delete_document_selection`；正向 Delete 路径没有相同检查，它只看同 block `focused_text_selection_range`。所以跨 block selection 对 Backspace/Delete 的行为确定不对称。

当前 `delete_document_selection` 又复用了名为 `collapse_cross_block_selection_for_paste` 的 helper：

- 要求 start/end 都有已加载 `text_model`；
- 将 start+1 到 end 的 index records 整段 drain；
- 用 start prefix + end suffix 合并成一个 plain text payload；
- 对 complex block、unloaded endpoint、list subtree、不同 kind 的合并策略没有独立删除语义。

这条 helper 既承担 paste 又承担 delete，且会把 rich spans 合并为 plain text，存在样式丢失和结构误删风险。

#### C. 整 block 多选（`selected_block_ids`）

这是确定无法删除的路径：`select_visible_block_range` 完成后把 `editing = None`，但 `delete_backward`/`delete_forward` 都不检查 `selected_block_ids`，也没有 `delete_selected_blocks`。随后删除命令找不到 focused block，直接返回 false。Copy/Cut 同样不识别这类 selection。

### 8C.2 正确命令分派

删除不应由 Backspace/Forward 各自猜当前状态，先统一解析 selection command：

```rust
enum ResolvedDeleteTarget {
    TextRange(NormalizedSelection),
    Blocks(NormalizedBlockSelection),
    TableCells(TableRangeSelection),
    ComplexInner(ComplexSelection),
    Caret { position: TextPosition, direction: DeleteDirection },
}

fn delete_selection_or_caret(direction: DeleteDirection)
    -> Result<EditTransaction, DeleteError>;
```

分派优先级：

```text
IME composition selection
  > table/complex inner selection
  > document text selection
  > whole block selection
  > focused text selection
  > caret directional delete
```

跨 block text delete 的语义应为：保留 start block 前缀和 end block 后缀及各自 rich spans；删除完全覆盖的中间 blocks；根据 kind compatibility 决定合并或保留 end block。整 block delete 则删除选中的 root/subtree，至少保留一个空 paragraph，并把 caret 放到前一个相邻 block 末尾或后一个 block 开头。

### 8C.3 实施任务

- [ ] **MULTI-DELETE-01** 写 GUI event-sequence 回归，分别复现同一行、同 block 跨行、跨 block、整 block drag selection。
- [ ] **MULTI-DELETE-02** 引入 `ResolvedDeleteTarget`，Backspace/Delete 共享 selection-first 分派，只在无 selection 时区分方向。
- [ ] **MULTI-DELETE-03** 新增 runtime `delete_selected_blocks` transaction，按 document order 处理，不迭代 HashSet 直接删除。
- [ ] **MULTI-DELETE-04** 定义 subtree policy：选中 parent 时是否自动包含 descendants；重叠 roots 规范化，禁止 orphan child。
- [ ] **MULTI-DELETE-05** 将 `collapse_cross_block_selection_for_paste` 拆成纯 selection plan + Paste/Delete 两个执行器。
- [ ] **MULTI-DELETE-06** 跨 block text delete 用 rich span slice/concat 保留首尾 marks，不调用 plain-text append/replace。
- [ ] **MULTI-DELETE-07** complex/unloaded block 使用 DocumentStore/PayloadWindow 计划；禁止因为 UI/entity/text_model 不存在而无法删除。
- [ ] **MULTI-DELETE-08** 正向 Delete 与 Backspace 对 non-caret selection 产生相同 transaction；方向只影响 caret/no-selection 情况。
- [ ] **MULTI-DELETE-09** transaction 完整记录 deleted records/payloads、before/after selection、scroll anchor、height changes、undo/redo。
- [ ] **MULTI-DELETE-10** 删除后清理所有 selection 暂态、table/complex focus、composition、menu pin，并把 caret 安置到确定位置。
- [ ] **MULTI-DELETE-11** 最后一个可见 block 被全部删除时原子地重置为空 paragraph，不允许空文档或悬空 focus。
- [ ] **MULTI-DELETE-12** Cut 复用同一 delete transaction，且仅在 clipboard 写入成功后提交。

### 8C.4 测试矩阵

- [ ] 同行正向/反向选区 + Backspace/Delete；selection 恰好位于 grapheme/emoji/组合字符边界。
- [ ] 同 block 跨软换行/硬换行 + Backspace/Delete；caret 落在 start offset。
- [ ] 跨 2/3/100 blocks 的 partial/full/partial selection，正向和反向选择结果一致。
- [ ] 首尾 Bold/Italic/Link spans 删除后剩余 marks 不丢失、不串到 suffix。
- [ ] 中间包含 list subtree、table、whiteboard、image、placeholder/unloaded page。
- [ ] 整 block 单选、多选、非连续选择、parent+child 重叠选择、全选。
- [ ] undo/redo 完整恢复 block ids、parent/depth、payload、selection、caret 和 viewport anchor。
- [ ] persistence transaction 与重载结果一致；大范围删除分批持久化但 UI 只提交一次原子结果。
- [ ] GUI mouse selection 后立即 Delete、切换输入法后 Delete、滚动虚拟窗口后 Delete。

## 9. 其他不符合 `cditor 开发助手` / 总体架构的模块

### 9.1 ARCH-SIZE：超大文件未拆分

明确超过技能建议的 700 行阈值：

| 文件 | 当前行数 | 问题 |
| --- | ---: | --- |
| `crates/ding-board/src/lib.rs` | 10902 | model、camera、input、render、toolbar、menu、thumbnail、IME、tests 全部混合 |
| `crates/ding-board/src/font.rs` | 922 | shaping、layout、wrap、caret/selection geometry 与 tests 混合 |

`crates/core/src/rich_text/markdown/inline.rs` 为 677 行，虽然尚未超过阈值，但已经同时承担 atomics、delimiter pairing、unclosed 检查、span build 与 tests，本次修复后必然越界，应提前拆。

- [ ] **ARCH-SIZE-01** 保持 `ding-board` public API 稳定，按 `model/camera/geometry/render/input/tools/thumbnail/embed/persistence` 分阶段拆分。
- [ ] **ARCH-SIZE-02** `font.rs` 拆为 `font_face`、`shaping`、`line_break`、`caret_geometry`、`decoration`。
- [ ] **ARCH-SIZE-03** 每次只做行为不变迁移，保留/迁移原单测，增加 standalone screenshot smoke test。
- [ ] **ARCH-SIZE-04** inline Markdown 按 8.3 的职责拆分。

### 9.2 ARCH-COMPLEX：复杂 block 抽象只存在于模型测试，在线路径未接入

`core/layout/block_editor_model.rs` 已定义 `BlockEditorModel`、`TableEditorModel`、inner selection 和 wheel transfer，但生产代码没有使用 `TableEditorModel`。在线表格使用另一套 `DocumentRuntime::TableRuntime` + GUI `GuiTableInteractionMode`；白板又完全由 `ding-board` 自己管理。

这意味着“复杂 block 有统一内部编辑模型”目前是文档/测试层能力，不是运行时不变量。

- [ ] **ARCH-COMPLEX-01** 决策：让 live TableRuntime/Whiteboard adapter 实现统一 trait，或删除没有生产用途的平行模型；禁止长期双轨。
- [ ] **ARCH-COMPLEX-02** 统一 complex selection、hit-test、inner scroll、height change、transaction、pin 生命周期。
- [ ] **ARCH-COMPLEX-03** 表格/白板交互通过 adapter 投影给 GUI，GUI 不保存语义 truth。

### 9.3 ARCH-SELECTION：selection 仍有多份可变真相

runtime 同时维护 `document_selection`、`focused_text_selection`、`selected_block_ids`；GUI 还维护 `table_interaction_mode`、drag controllers 与 `action_block_id`。这些状态之间依靠事件函数手工 clear/sync，容易出现一个状态更新而另一个残留。

- [ ] **ARCH-SELECTION-01** 设计统一 `EditorSelection`：Text / Blocks / TableCells / ComplexInner。
- [ ] **ARCH-SELECTION-02** `focused_text_selection`、`selected_block_ids` 尽量改为统一 truth 的索引/缓存投影，不作为独立可写状态。
- [ ] **ARCH-SELECTION-03** GUI 仅保留 pointer gesture 暂态，完成 gesture 后提交 runtime selection transaction。
- [ ] **ARCH-SELECTION-04** selection fragments、copy/cut/delete、toolbar、gutter highlight 全从统一 selection 派生。

### 9.4 ARCH-PERSIST：GUI 层直接持有 Postgres 实现

`cditor-app` 直接依赖 `sqlx` 和 `cditor-storage-postgres`；`gui/persistence/postgres_saver.rs` 持有 `PgPool`、构造具体 store、从 runtime drain transaction 并组装 save batch。这不符合“UI 只消费 projection / editor -> runtime -> storage”的干净边界，也让白板每次 host dirty 更容易牵动根 view 状态。

- [ ] **ARCH-PERSIST-01** 在 application/runtime service 层定义 `PersistenceCoordinator`，只依赖 storage traits。
- [ ] **ARCH-PERSIST-02** Postgres adapter 留在 store-postgres；GUI 只发送 SaveRequested/Dirty 事件并消费 SaveStatus projection。
- [ ] **ARCH-PERSIST-03** 保存队列按 dirty block/transaction 增量抓取，禁止 GUI 读取整个 loaded payload window。
- [ ] **ARCH-PERSIST-04** 保存失败、重试、dirty pin、close flush 做 service 级集成测试。

### 9.5 ARCH-LAYOUT：core/app 重复布局魔数

当前 `DEFAULT_LAYOUT_WIDTH_PX=812`、document width 860、shell padding/gutter/gap/border 等分散在 core 与 app。表格两个现象已经证明这些常量会漂移。

- [ ] **ARCH-LAYOUT-01** 建立共享语义 metrics，不让 core 依赖 GPUI，但让 GUI 从同一数据结构渲染。
- [ ] **ARCH-LAYOUT-02** layout cache key 加入 width/theme/font/scale/version。
- [ ] **ARCH-LAYOUT-03** 所有 complex block 提供 exact/predictive height 置信度，并用 onscreen measurement 校正。
- [ ] **ARCH-LAYOUT-04** 删除无法说明组成的 812、16 等派生魔数，改成命名组件求和。

### 9.6 ARCH-TRANSACTION：白板 scene 更新绕过完整编辑事务

`update_whiteboard_scene_json` 目前只替换字符串并增加 content version，没有 document undo step、before/after selection、persistence transaction 或显式 dirty range。虽然白板内部有 undo，这不能替代文档层对“关闭 editor、跨 block undo、崩溃恢复”的一致管理。

- [ ] **ARCH-TRANSACTION-01** 定义 `ComplexBlockEditSession` 与 coalesced document transaction。
- [ ] **ARCH-TRANSACTION-02** session 内部 undo 由 board 负责，session commit 在 document undo 中形成一个可配置边界。
- [ ] **ARCH-TRANSACTION-03** transaction 携带 scene version/hash，不把大 JSON 重复塞入每个 pointer event。
- [ ] **ARCH-TRANSACTION-04** 崩溃恢复验证最后 committed snapshot，未 commit session 有 recovery journal。

### 9.7 ARCH-TEST：测试未覆盖真实组合链路

技能要求每项功能完整单测。当前缺少：

- 白板预览 focus + Enter 的 runtime/GUI 回归；
- gutter click/drag/menu 完整状态机；
- block-only selection screenshot；
- table 最后一行像素边框与实际 measured height；
- inline Markdown 逐字符多轮转换；
- Cditor clipboard metadata、跨 block rich fragments 与外部应用纯文本降级；
- 同行/跨行/跨 block/整 block selection 的统一删除命令；
- 白板 standalone/integrated 对比 benchmark。

- [ ] **ARCH-TEST-01** 每个修复 PR 必须先增加能失败的回归测试，再实现。
- [ ] **ARCH-TEST-02** 纯模型测试之外增加 GUI event-sequence 测试和 screenshot/golden。
- [ ] **ARCH-TEST-03** 增加 10 分钟真实 editor soak：滚动、输入、table、whiteboard、autosave 并行。
- [ ] **ARCH-TEST-04** CI 门禁至少包含 `cargo fmt --check`、workspace test、clippy、关键 performance trace replay。

### 9.8 ARCH-DOC：实施状态文档已漂移

`large-document-rich-text-implementation-status.md` 仍写“尚未接入真实 GPUI”“242 passed”，与当前仓库已有完整 GPUI app、Postgres 和更多测试不符。过期状态会让后续架构决策基于错误事实。

- [ ] **ARCH-DOC-01** 更新实施状态为当前 crate/GUI/DB 事实，不再记录一次性的旧测试数字作为长期真相。
- [ ] **ARCH-DOC-02** 对每个架构不变量增加“生产调用点”和“验证测试”链接；只有模型 mock 不算已产品化。

## 10. MERMAID：评估并接入 Zed 新原生渲染器

### 10.1 结论

Zed 最近更新的 Mermaid 功能确实已经拆成独立 crate：`crates/mermaid_render`。2026-05-27 合并的 [PR #57644](https://github.com/zed-industries/zed/pull/57644) 用 `merman` 替换旧的 `mermaid-rs`；2026-06-11 的 [PR #59140](https://github.com/zed-industries/zed/pull/59140) 又修正了长标签与 `foreignObject`/resvg 不兼容造成的重叠。因此，如采用 Zed 方案，基线至少应包含后一个修复，不能复制最初发布快照。

技术结论是：**可以复用，不能不加改造地直接拷贝**。

- Zed crate 的公开入口很小：`render_to_svg(source, theme) -> Result<String>`。
- 它是接近叶子的 Rust crate，不依赖 Node、浏览器或 WebView；主要依赖 `merman`、`quick-xml`、`serde_json`、`anyhow`，GPUI 仅用于 `Hsla/Rgba` 颜色类型。
- Zed 的 [Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/mermaid_render/Cargo.toml) 继承 workspace 配置且整个 Zed workspace `publish = false`，因此它不是一个可以直接 `cargo add mermaid_render` 的 crates.io 包。
- 直接 git 依赖整个 Zed 仓库也不合适：本项目 GPUI 固定在提交 `1d217ee39d381ac101b7cf49d3d22451ac1093fe`，而 Zed main 的 `mermaid_render` 依赖其当前 workspace GPUI，容易引入第二套 GPUI 类型与更大的依赖图。
- `mermaid_render` 明确采用 `GPL-3.0-or-later`。本仓库根目录目前没有统一 LICENSE，虽然 `ding-board` 已声明 GPL，但仍需先明确整个 Cditor 的发布许可证。复制 Zed 源码必须保留许可证与 attribution；若产品需闭源或采用宽松许可证，就不应复制这部分 GPL 代码。

当前项目已经具备 Mermaid block 的数据骨架：`RichBlockKind::Mermaid`、slash menu、持久化 kind 和稳定高度规则都存在，但 `crates/app/src/gui/block/block_view.rs` 仍把 Mermaid 与 RawMarkdown 一起按代码框显示。因此不需要改数据格式，缺的是 renderer、异步任务、缓存与 GUI image adapter。

### 10.1A 2026-07-12 实施状态

第一阶段已接入。由于 Cditor 的 GPUI 本身已经固定在 Zed commit `1d217ee39d381ac101b7cf49d3d22451ac1093fe`，且该提交同时包含新版 `mermaid_render` 与长标签修复，当前采用“同仓库、同 commit 固定 git dependency”，而不是依赖 Zed main 或复制一份会漂移的源码。这样 Cargo 只解析一套 GPUI 类型；`Cargo.lock` 同时固定 patched `merman` revision。

已完成：

- [x] `cditor-app` 接入 Zed `mermaid_render`，app 明确声明 `GPL-3.0-or-later`，补充 GPL/Apache 文本和 third-party notice。
- [x] 新建 `gui/block/mermaid/{cache,render,theme}`，没有把渲染逻辑塞回 `block_view.rs`。
- [x] 可见窗口内按 block/content version/source hash/theme 建缓存；离开窗口或换文档时释放 task/cache。
- [x] `merman -> raster-safe SVG -> GPUI RenderImage` 全部在 background task 执行，完成后只 notify 当前 editor。
- [x] 连续编辑保留上一次成功图作为 fallback；过期 task 随旧 entry drop，不覆盖新 generation。
- [x] 默认预览、源码切换、loading、首行错误回退；预览态拦截隐藏源码修改，Enter 在 block 后创建 paragraph。
- [x] Mermaid 被定义为 Markdown source input capability；源码态 Enter 插入换行而不拆 block。
- [x] 增加 256 KiB source 上限、真实 SVG 渲染测试和 `foreignObject` 断言。
- [x] `cargo check -p cditor-app` 通过；`cditor-app --lib` 217 passed、1 ignored；10w block demo 使用 runtime shader 模式可启动且无 renderer panic。

仍未完成：按 SVG intrinsic ratio 动态更新 exact block height、DPI/resize golden、100 Mermaid block 性能 trace、更多 diagram 类型 golden 和导出 SVG/图片。这些继续保留在下方任务中。

### 10.2 两条可选路线

#### 路线 A：vendor Zed `mermaid_render`（Cditor 接受 GPL 时推荐）

将 Zed crate 固定到一个包含 PR #59140 的明确 commit，复制为 `crates/mermaid-render`，保留 Zed 的 GPL 文件、copyright 和 upstream commit 记录。修改 manifest 为本仓库可独立解析的依赖；优先让 crate 自己定义颜色 DTO，或由 app adapter 做 GPUI 颜色转换，避免 renderer 与特定 GPUI revision 耦合。

优点：最快获得 Zed 已验证的主题注入、accent color 后处理和 resvg 兼容修复。缺点：GPL 传播要求和后续 upstream 同步成本。

#### 路线 B：直接使用 `merman`，自行实现 Cditor adapter（非 GPL 产品推荐）

[merman](https://github.com/zed-industries/merman) 本身采用 MIT/Apache-2.0，已经提供 headless Rust 渲染和 resvg-safe SVG pipeline。Cditor 可只依赖 Zed 使用的 patched tag 或兼容 release，自己实现主题映射和必要的 SVG 清理，不复制 `mermaid_render` 的 GPL 源码。

优点：许可证边界清晰、可去掉 GPUI 类型依赖。缺点：需要重新验证 class/mindmap/sequence accent color、fallback、`foreignObject` 与 CSS 清理；视觉结果不一定与 Zed 完全一致。

推荐决策：若 Cditor 确认整体 GPL，走 A；否则走 B。不要用“先复制以后再处理许可证”的方式做生产合并。此处仅是工程许可证识别，不替代正式法律意见。

### 10.3 推荐架构

```text
core / engine
  Mermaid block 只保存 source + block/version
            |
            v
app MermaidRenderCoordinator
  key = source_hash + theme_revision + width_bucket + scale_factor
  后台 CPU task + generation/cancellation
            |
            v
mermaid-render crate
  source + neutral theme DTO -> SVG String
            |
            v
app GPUI adapter
  SVG -> RenderImage -> measured intrinsic size -> repaint block
```

边界要求：

- core/engine 不依赖 renderer、GPUI 或 SVG；只保存可编辑源码，SVG/bitmap 全是可重建缓存。
- `render_to_svg` 是同步 CPU 工作，禁止在 GPUI render/paint 或每次按键主线程中直接调用。
- coordinator 按 block generation 丢弃过期结果；快速连续输入只显示上一个成功图或 skeleton，debounce 后渲染最新版。
- 两级缓存建议为 SVG cache 与当前 scale 的 `RenderImage` cache。key 至少包含源码 hash、主题版本、布局宽度桶、scale factor 和 renderer version。
- Mermaid 源码改变时只 invalid 当前 block；主题变化可复用源码解析结果的能力取决于 merman API，否则按 theme key 重渲染。
- SVG 成功后根据 `viewBox`/intrinsic aspect ratio 更新 exact height；完成前继续使用稳定预测高度，并走现有 scroll anchor correction，避免图完成时文档跳动。
- 错误不能覆盖源码或 payload。预览区显示简短错误与“查看源码/重试”，保留编辑、复制和导出能力。
- 对输入长度、节点数、渲染时间和 SVG 尺寸设上限；禁止外部资源加载，并对最终 SVG 做安全策略验证。

### 10.4 可推进任务

- [ ] **MERMAID-00 许可证决策门**
  - [ ] 明确 Cditor 根项目的发布许可证和分发方式。
  - [ ] 若选路线 A，记录 Zed upstream commit、复制文件清单、GPL/NOTICE 保留方式。
  - [ ] 若选路线 B，记录只使用 MIT/Apache `merman`、不复制 Zed GPL 后处理代码的边界。
- [ ] **MERMAID-01 独立 compile spike**
  - [ ] 新建隔离实验 crate，不先接生产 block。
  - [ ] 用 flowchart、sequence、class、state、mindmap 各渲染一张 SVG。
  - [ ] 验证当前 Rust 1.95 满足依赖 MSRV；核对 macOS 构建时间、二进制增量和依赖重复。
  - [ ] 验证当前固定 GPUI 提交是否存在 SVG 单帧 raster API；若 API 不兼容，只在 app adapter 适配，不升级整个 GPUI 作为本任务的隐含副作用。
- [ ] **MERMAID-02 建 renderer crate**
  - [ ] 输入使用与 GUI 无关的 `MermaidThemeDto`，输出 `Result<RenderedSvg, MermaidRenderError>`。
  - [ ] 将 parse/render、SVG postprocess、theme、error、tests 拆分，避免形成单个超大模块。
  - [ ] 错误包含阶段、可读摘要和可诊断 source location，不把完整敏感文档写日志。
- [ ] **MERMAID-03 主题适配**
  - [ ] 从 `GuiTheme` 映射 background/foreground/border/accent/line/text 等颜色。
  - [ ] 覆盖浅色、深色和 accent 变化；主题 revision 纳入 cache key。
  - [ ] 检查 class/mindmap/sequence diagram 的特殊配色和对比度。
- [ ] **MERMAID-04 后台协调器**
  - [ ] 每个 block 保存 source version、requested generation、last successful artifact 和 error state。
  - [ ] debounce 连续编辑，后台执行同步 renderer，完成时校验 generation 后再提交。
  - [ ] block 离开 virtual window、文档关闭或源码更新时取消/忽略过期任务。
  - [ ] 限制并发数，避免多个复杂图同时抢占编辑器 CPU。
- [ ] **MERMAID-05 GPUI 图像接入**
  - [ ] SVG 在 app 层 rasterize 为 `RenderImage`，不得在每次 paint 重做解析/渲染。
  - [ ] 使用 block id + generation 定点 notify/repaint，不让根 editor 整体重建。
  - [ ] 处理 DPI、缩放、宽度变化和 image cache 失效。
- [ ] **MERMAID-06 block 交互**
  - [ ] 默认显示图，提供图/源码 toggle；编辑源码时不破坏 Mermaid kind。
  - [ ] loading 使用稳定 skeleton；失败显示错误但保留最后一次成功图和源码入口。
  - [ ] copy/export 明确区分“复制源码”“复制 SVG/图片”。
- [ ] **MERMAID-07 布局与虚拟化**
  - [ ] 从 SVG intrinsic size 计算高度并写回测量缓存，不持久化 bitmap。
  - [ ] 宽度变化按 bucket 重渲染，exact height 到达时执行 anchor correction。
  - [ ] offscreen block 不抢占前台任务；重新进入窗口优先命中缓存。
- [ ] **MERMAID-08 安全与资源预算**
  - [ ] 设置源码长度、节点数、SVG byte size、最大像素面积和并发预算。
  - [ ] 禁止网络/文件资源、脚本、事件属性及不支持的 `foreignObject`。
  - [ ] 超时/超限返回可恢复错误，不能卡住输入线程或导致 payload 丢失。
- [ ] **MERMAID-09 upstream 维护**
  - [ ] 固定版本和 commit，不跟随 Zed main 浮动构建。
  - [ ] 在 vendor README 记录本地 patch、升级步骤和 golden 差异。
  - [ ] 升级时先同步 merman/Zed 修复，再跑全套 golden 与性能基准。

### 10.5 测试与验收

- [ ] 单元测试：有效/无效源码、空图、Unicode、长标签、特殊字符和主题色映射。
- [ ] golden：flowchart、sequence、class、state、ER、mindmap 在浅/深主题输出稳定且无文字重叠。
- [ ] GUI：源码连续输入 30 次只提交最新版；旧 task 完成不能覆盖新图。
- [ ] 虚拟化：含 100 个 Mermaid block 的文档只渲染窗口内高优先级图，滚动无主线程长帧。
- [ ] 性能门槛：cache hit 不重新调用 merman；渲染任务不在 UI thread；记录 p50/p95/p99 与最大内存。
- [ ] 布局：首次完成、主题切换、窗口 resize、DPI 切换不发生明显 scroll jump，误差满足统一 layout DoD。
- [ ] 安全：外部 URL、恶意 SVG/CSS、超大图和超深图均被拒绝或安全降级。
- [ ] 许可证：产物与源码分发包含所选路线要求的 LICENSE/NOTICE，依赖审计通过。

### 10.6 Go/No-Go 判定

只有同时满足以下条件才进入生产接入：许可证路线已确认；compile spike 不要求全局 GPUI 升级；五类图能生成并被当前 GPUI 显示；渲染完全移出 UI thread；长标签修复与资源上限通过。否则保持 Mermaid 源码模式，不为赶功能引入第二套 GPUI 或未经确认的 GPL 代码。

## 11. 推荐里程碑与依赖

### Milestone A：阻止数据/格式破坏（P0）

- [ ] 完成 WB-ENTER-01 ~ 07。
- [ ] 完成 MD-INLINE-01 ~ 07。
- [ ] 完成 MULTI-DELETE-01 ~ 12。
- [ ] 相关 runtime、GUI、persistence 回归全部通过。
- [ ] 对已可能损坏的 Whiteboard kind/RichText payload 增加加载检测与恢复提示。

**完成定义**：白板不会被任何文本命令静默降级；多轮 inline Markdown 输入不丢已有 marks；任何非空 selection 都能通过统一 transaction 安全删除并 undo。

### Milestone B：完成 gutter 交互闭环（P1）

- [ ] 完成 GUTTER-MENU 全部任务。
- [ ] 完成 GUTTER-SELECT 全部任务。
- [ ] 完成 MULTI-SELECT-GEO 全部任务。
- [ ] 同时落地统一 selection 的最小生产切片，避免新菜单继续依赖 `action_block_id`。

**完成定义**：单击、拖拽、菜单、整 block 选区和跨 block 文本 fragment 互不冲突，任何选区都不包含 gutter。

### Milestone C：统一表格 layout box（P1）

- [ ] 完成 TABLE-BORDER 与 TABLE-WIDTH 全部任务。
- [ ] 根 block 和多级缩进 table 通过 golden/DPI 测试。

**完成定义**：默认表格不溢出，最后一行选框完整，runtime 与 measured height 误差在 0.5 device px 内。

### Milestone D：白板性能达标（P1）

- [ ] 先交 WB-PERF trace 报告。
- [ ] 再完成局部 repaint、snapshot debounce、transaction coalescing。
- [ ] 达到 5.5 的性能门槛和不丢数据验收。

### Milestone D2：剪贴板格式闭环（P1）

- [ ] 完成 CLIPBOARD-01 ~ 10。
- [ ] 应用内 inline/blocks/table 粘贴保留样式和结构。
- [ ] 外部应用只读取纯文本，metadata 缺失或损坏时安全降级。
- [ ] Cut 与 MULTI-DELETE 共用原子 transaction，clipboard 失败不删除内容。

### Milestone E：架构收口（P2）

- [ ] 复杂 block live adapter 接入。
- [ ] selection 单真相。
- [ ] persistence 移出 GUI 具体实现。
- [ ] layout metrics 单来源。
- [ ] 拆分 ding-board 超大文件。
- [ ] 更新架构状态文档与 CI 门禁。

### Milestone F：Mermaid 原生预览（P2）

- [ ] 先完成 MERMAID-00 许可证决策与 MERMAID-01 compile spike。
- [ ] 按许可证结论选择 vendor Zed renderer 或直接使用 merman，禁止两条路线混用。
- [ ] 完成异步协调器、缓存、GPUI adapter、源码 toggle 和安全预算。
- [ ] 通过 10.5 的 golden、性能、虚拟化、布局与许可证验收。

## 12. 每个任务/PR 的统一完成定义

每个勾选项只有同时满足以下条件才算完成：

- [ ] 实现遵循 `core -> runtime -> storage` 与 `app -> runtime` 的单向边界。
- [ ] UI 只保存 gesture/overlay 暂态，文档、selection、layout、scroll truth 在内核。
- [ ] 有先失败后通过的单元测试；GUI 问题有 event-sequence 或 screenshot 验证。
- [ ] 涉及复杂 block 时覆盖 focus、selection、IME、undo、persistence、virtual window pin。
- [ ] 涉及高度/宽度时覆盖 DPI、缩进、resize、anchor correction。
- [ ] 涉及 hot path 时提供 p95/p99 trace，不用主观“感觉流畅”验收。
- [ ] `cargo fmt --check`、相关 crate test、workspace check 通过，无新增 warning。
- [ ] 更新对应架构/实施状态文档，写清未完成边界。
