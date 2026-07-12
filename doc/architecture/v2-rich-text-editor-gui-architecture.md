# Cditor 富文本 GUI 架构方案

本文档是在阅读第一版 `/Users/jychen/Desktop/Cditor/src/editor2` 后，为当前 `Cditor` 重写的一版富文本 GUI / 数据 / 渲染方案。

目标不是原封不动搬 `editor2`，而是借鉴它已经验证过的 GPUI block renderer、text element、gutter、table、image、code block 等经验，同时严格遵守 V2 大文档架构原则：

```text
UI Entity 不是真相。
DocumentIndex / VisibleDocumentIndex / HeightIndex / VirtualScrollState 才是运行时真相。
```

---

## 1. 第一版 editor2 的可借鉴点

第一版 `editor2` 主要结构：

```text
Cditor2
  -> CditorRuntimeDocument / CditorIndexedRuntimeDocument
      -> CditorDocumentTree / LightweightDocumentIndex
      -> Entity<CditorBlock>
          -> CditorBlockTextElement
          -> component/code_block
          -> component/table
          -> component/image
          -> component/gutter
          -> component/list_prefix
```

### 值得保留的能力

1. **Block renderer 经验**
   - paragraph / heading / quote / callout
   - code block toolbar / highlight
   - table cell renderer
   - image renderer
   - whiteboard / mind map embed
   - list prefix
   - gutter

2. **Text element 经验**
   - inline mark 渲染
   - cursor / selection / marked range
   - wrapped line cache
   - IME bounds
   - link hit test

3. **复杂 block 的局部 runtime**
   - table cell entity
   - code highlight cache
   - image runtime cache
   - whiteboard/mind embed state

4. **GPUI event wiring**
   - focus
   - mouse down / drag selection
   - key down
   - composition
   - clipboard
   - slash menu

5. **Indexed runtime 的经验**
   - 只 hydrate viewport 附近 block
   - placeholder page
   - height index
   - async hydrate discard
   - perf counters

### 不能照搬的问题

第一版 `editor2` 仍然有这些 V2 不应继承的问题：

```text
Entity<CditorBlock> 持有 BlockRecord，并承担太多文档状态。
ListState / Entity 生命周期容易成为滚动和数据真相。
selection / cursor 主要在 block entity 内部。
height 测量回写路径容易与全局 height truth 混在一起。
indexed runtime 与非 indexed runtime 是两套并行逻辑。
```

V2 需要把这些拆开：

```text
数据真相：V2 core/runtime/storage
UI 投影：GPUI View / Entity
局部缓存：BlockViewEntity / TextLayoutCache / TableRuntimeCache
```

---

## 2. V2 总体设计

V2 富文本 GUI 应分成五层：

```text
Storage Layer
  -> DocumentStore / BlockPayloadStore / LayoutCacheStore

Runtime Truth Layer
  -> DocumentRuntime
  -> DocumentIndex
  -> VisibleDocumentIndex
  -> BlockHeightIndex
  -> PageLayoutIndex
  -> VirtualScrollState
  -> DocumentSelection
  -> EditingSession

Projection Layer
  -> RenderWindow
  -> ViewBlockSnapshot
  -> PayloadWindow
  -> DebugOverlaySnapshot

GUI Entity Layer
  -> CditorV2View
  -> BlockViewEntity
  -> TextElement / TableElement / CodeElement / ImageElement

Cache / Async Layer
  -> EntityCache
  -> MediaCache
  -> TextLayoutCache
  -> LayoutScheduler
  -> TraceEventLog
```

核心原则：

```text
UI 只渲染当前窗口投影。
UI 不拥有文档结构真相。
UI 不拥有全局 selection 真相。
UI 不拥有全局 scroll 真相。
UI 可以拥有局部 text layout / hit test / composition geometry cache。
```

---

## 3. 数据结构设计

当前 V2 已经有：

```rust
BlockIndexRecord
DocumentIndex
VisibleDocumentIndex
BlockHeightIndex
PageLayoutIndex
VirtualScrollState
EditingSession
DocumentSelection
```

但它还缺富文本内容 DTO。需要从第一版借鉴并重写为 V2 的 `rich_text` / `payload` 模块。

建议新增：

```text
src/core/rich_text/
  mod.rs
  block_kind.rs
  inline.rs
  payload.rs
  attrs.rs
  table.rs
  asset.rs
```

### 3.1 BlockKind

第一版有 `BlockKind`，V2 应保留等价能力，但命名更贴近架构文档：

```rust
pub enum RichBlockKind {
    Paragraph,
    Heading { level: u8 },
    Quote,
    Callout { variant: CalloutVariant },
    Todo { checked: bool },
    BulletedList,
    NumberedList,
    Toggle,
    Code { language: Option<String> },
    Math,
    Mermaid,
    Html,
    Table,
    Image,
    File,
    Whiteboard,
    MindMap,
    Embed,
    Divider,
    Database,
    Custom(String),
}
```

和当前 `BlockIndexRecord.kind_tag` 的关系：

```text
BlockIndexRecord.kind_tag 负责轻量索引和快速判断类型。
RichBlockKind 负责 payload 层和 renderer。
```

需要一个注册表：

```rust
pub struct BlockKindRegistry {
    descriptors: HashMap<u16, BlockKindDescriptor>,
}
```

其中：

```rust
pub struct BlockKindDescriptor {
    pub tag: u16,
    pub name: &'static str,
    pub layout_behavior: LayoutBehavior,
    pub supports_children: bool,
    pub supports_rich_text_title: bool,
    pub can_contain_blocks: bool,
}
```

---

### 3.2 InlineSpan / InlineMark

借鉴第一版：

```rust
pub struct InlineSpan {
    pub text: String,
    pub marks: Vec<InlineMark>,
}

pub enum InlineMark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link { href: String },
    Color(String),
    Background(String),
}
```

V2 额外需要明确 offset 语义：

```text
内部编辑统一使用 UTF-8 byte offset + grapheme boundary 校验。
平台 IME / selection API 使用 UTF-16 offset 时必须走 TextOffsetMap。
```

因此 V2 的 inline 模型要和已有：

```rust
TextOffsetMap
InternalTextOffset
PlatformUtf16Offset
GraphemeIndex
```

打通。

---

### 3.3 BlockPayload

V2 应该把 block 结构索引和 block 内容拆开：

```rust
pub struct BlockPayloadRecord {
    pub block_id: BlockId,
    pub content_version: u64,
    pub kind: RichBlockKind,
    pub payload: BlockPayload,
}
```

```rust
pub enum BlockPayload {
    RichText {
        spans: Vec<InlineSpan>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Table(TablePayload),
    Image(ImagePayload),
    File(FilePayload),
    Whiteboard(WhiteboardPayload),
    MindMap(MindMapPayload),
    Embed(EmbedPayload),
    Html {
        html: String,
        sanitized: bool,
    },
    Empty,
}
```

这对应架构文档里的：

```text
blocks            -> 结构真相
block_payloads    -> 内容真相
block_attrs       -> 样式/通用属性
block_layout      -> 高度缓存
```

---

### 3.4 BlockAttrs

V2 应有独立 attrs：

```rust
pub struct BlockAttrs {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub text_align: TextAlign,
    pub indent: u16,
    pub folded: bool,
    pub locked: bool,
    pub custom: serde_json::Value,
}
```

注意：

```text
attrs 不进入 UI Entity 真相。
attrs 是 payload/store/runtime 的数据。
UI 只是读取 attrs 渲染。
```

---

## 4. Runtime 设计

建议新增：

```text
src/runtime/document_runtime.rs
src/runtime/payload_window.rs
src/runtime/view_projection.rs
```

### 4.1 DocumentRuntime

```rust
pub struct DocumentRuntime {
    pub document_id: DocumentId,
    pub index: DocumentIndex,
    pub visible_index: VisibleDocumentIndex,
    pub height_index: BlockHeightIndex,
    pub page_layout: PageLayoutIndex,
    pub selection: Option<DocumentSelection>,
    pub scroll: VirtualScrollState,
    pub editing: Option<EditingSession>,
    pub payload_window: PayloadWindow,
    pub trace: TraceEventLog,
}
```

职责：

```text
1. 接受编辑事务。
2. 更新 DocumentIndex / VisibleIndex / HeightIndex。
3. 维护当前 render window 所需 payload。
4. 维护 selection / scroll / editing session。
5. 输出 ViewProjection 给 GUI。
```

不负责：

```text
不直接绘制 UI。
不持有 GPUI Entity。
不同步 SQLite 写。
```

---

### 4.2 PayloadWindow

当前窗口 payload：

```rust
pub struct PayloadWindow {
    pub block_range: Range<usize>,
    pub payloads: HashMap<BlockId, BlockPayloadRecord>,
    pub loading: HashSet<BlockId>,
    pub failed: HashMap<BlockId, String>,
}
```

规则：

```text
只 hydrate 当前窗口 + overscan 的 payload。
跨页 selection / search / copy 不依赖 payload window，必要时走 store/query index。
离开窗口的 payload 可释放，但 Dirty / Editing / Selection endpoint 不能释放。
```

---

### 4.3 ViewProjection

GUI 不直接读 runtime 内部结构，而是读 projection：

```rust
pub struct EditorViewProjection {
    pub document_id: DocumentId,
    pub scroll: VirtualScrollState,
    pub render_window: RenderWindow,
    pub blocks: Vec<ViewBlockSnapshot>,
    pub debug: DebugOverlaySnapshot,
}
```

```rust
pub struct ViewBlockSnapshot {
    pub block_id: BlockId,
    pub visible_index: usize,
    pub depth: u16,
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayloadView,
    pub layout: BlockLayoutMeta,
    pub selected: bool,
    pub focused: bool,
    pub pinned: bool,
    pub placeholder: bool,
}
```

`BlockPayloadView` 是 UI 友好的 enum：

```rust
pub enum BlockPayloadView {
    Loaded(BlockPayloadRecord),
    Placeholder {
        estimated_height: f64,
    },
    Loading {
        stable_box: StableBox,
    },
    Error {
        message: String,
    },
}
```

---

## 5. GUI 设计

建议新增：

```text
src/gui/
  mod.rs
  app.rs
  cditor_v2.rs
  block_view.rs
  block_renderer.rs
  text_element.rs
  input.rs
  scroll.rs
  gutter.rs
  theme.rs
```

### 5.1 CditorV2View

```rust
pub struct CditorV2View {
    runtime: Entity<DocumentRuntimeEntity>,
    block_entities: EntityCache<BlockId, BlockViewEntity>,
    show_debug_overlay: bool,
}
```

这里 `DocumentRuntimeEntity` 可以是 GPUI entity 包装层，但它内部持有的是 V2 runtime：

```rust
pub struct DocumentRuntimeEntity {
    runtime: DocumentRuntime,
}
```

注意：

```text
GPUI Entity 只是为了响应/notify/render。
DocumentRuntime 仍是数据真相。
```

---

### 5.2 BlockViewEntity

第一版 `CditorBlock` 太重，它持有 `BlockRecord` 并修改内容。

V2 应改成：

```rust
pub struct BlockViewEntity {
    pub block_id: BlockId,
    pub focus: FocusHandle,
    pub local_cache: BlockLocalCache,
}
```

其中：

```rust
pub struct BlockLocalCache {
    pub text_layout: TextLayoutCache,
    pub caret_geometry: Option<CaretGeometryCache>,
    pub hit_test_geometry: Option<CaretGeometryCache>,
    pub table_runtime: Option<TableRuntimeCache>,
    pub image_runtime: Option<ImageRuntimeCache>,
    pub code_highlight: Option<CodeHighlightCache>,
}
```

关键区别：

```text
BlockViewEntity 不保存 BlockPayload 真相。
每次 render 接收 ViewBlockSnapshot。
本地 cache 必须带 content_version/layout_version 校验。
```

---

### 5.3 block renderer

从第一版 `editor2/block/render.rs` 借鉴布局结构，但输入改为：

```rust
pub fn render_block(
    entity: Entity<BlockViewEntity>,
    block: &ViewBlockSnapshot,
    runtime: Entity<DocumentRuntimeEntity>,
    window: &mut Window,
    cx: &mut Context<BlockViewEntity>,
) -> impl IntoElement
```

渲染分发：

```rust
match block.kind {
    RichBlockKind::Paragraph => render_rich_text_block(...),
    RichBlockKind::Heading { level } => render_heading_block(...),
    RichBlockKind::Quote => render_quote_block(...),
    RichBlockKind::Callout { .. } => render_callout_block(...),
    RichBlockKind::Todo { .. } => render_todo_block(...),
    RichBlockKind::BulletedList | RichBlockKind::NumberedList => render_list_block(...),
    RichBlockKind::Code { .. } => render_code_block(...),
    RichBlockKind::Table => render_table_block(...),
    RichBlockKind::Image => render_image_block(...),
    RichBlockKind::Whiteboard => render_whiteboard_block(...),
    RichBlockKind::Embed => render_embed_block(...),
    RichBlockKind::Divider => render_divider(...),
    _ => render_placeholder_block(...),
}
```

第一版可复用部分：

```text
component/plain_text   -> 改为 render_rich_text_block
component/code_block   -> 改为接 BlockPayload::Code
component/table        -> 改为接 BlockPayload::Table
component/image        -> 改为接 BlockPayload::Image + MediaCache
component/list_prefix  -> 可直接改造
component/gutter       -> 可直接改造
text/element.rs        -> 改成 V2 TextElement
```

---

## 6. TextElement 重写方案

第一版 `CditorBlockTextElement` 很有价值，但它写 layout 结果回 `CditorBlock` entity。

V2 要改为：

```text
TextElement 负责 shape / paint / hit test。
layout result 写入 BlockViewEntity.local_cache。
高度变化通过 Runtime event 上报给 DocumentRuntime。
DocumentRuntime 再更新 HeightIndex / PageLayoutIndex。
```

建议：

```rust
pub struct RichTextElement {
    pub block_id: BlockId,
    pub content_version: u64,
    pub layout_version: u64,
    pub spans: Vec<InlineSpan>,
    pub selection: Option<SelectionRange>,
    pub marked_range: Option<Range<usize>>,
    pub caret: Option<TextPosition>,
    pub editable: bool,
}
```

prepaint 后生成：

```rust
pub struct RichTextPrepaintState {
    pub lines: Rc<[WrappedLine]>,
    pub cursor: Option<PaintQuad>,
    pub selection: Vec<PaintQuad>,
    pub inline_code_backgrounds: Vec<PaintQuad>,
    pub line_height: Pixels,
    pub measured_height: Pixels,
}
```

然后发送：

```rust
BlockViewEvent::Measured {
    block_id,
    content_version,
    layout_version,
    height,
}
```

Runtime 收到后：

```text
1. 校验 content_version/layout_version。
2. 更新 BlockHeightIndex。
3. 更新 PageLayoutIndex。
4. 进入 HeightCorrectionPipeline。
5. 需要时 restore anchor。
```

---

## 7. 输入事件设计

第一版是 block entity 直接处理 `on_key_down`。

V2 应改成：

```text
GPUI event
  -> GuiInputAdapter
  -> DocumentRuntime command
  -> EditTransaction
  -> Runtime update
  -> Projection update
  -> UI notify
```

### 7.1 Key input

```rust
pub enum GuiEditorCommand {
    InsertText { text: String },
    DeleteBackward,
    DeleteForward,
    SplitBlock,
    MergeBlockBackward,
    MoveCaretLeft,
    MoveCaretRight,
    MoveCaretUp,
    MoveCaretDown,
    ToggleMark(InlineMarkKind),
    SetBlockKind(RichBlockKind),
}
```

执行：

```text
InsertText
  -> EditingSession must exist / create focus session
  -> SingleCharInputHotPath for single char
  -> BatchTextInputHotPath for paste/composition commit
  -> update payload memory first
  -> schedule async persist / FTS / highlight
```

### 7.2 IME

```text
composition preview 不写 payload truth。
composition state 存在 EditingSession。
candidate rect 来自 CaretGeometryCache。
composition block 必须 pin。
```

### 7.3 Mouse selection

```text
mouse down/move/up
  -> hit test current window geometry
  -> update DocumentSelection
  -> selection endpoint block pin
```

注意：

```text
selection 不能只存在 UI entity 内。
跨页 selection 必须由 DocumentSelection + DocumentIndex 解析。
```

---

## 8. 滚动设计

第一版 `CditorIndexedRuntimeDocument` 使用 `ListState + RuntimeHeightIndex` 实现估算滚动。

V2 应使用已经实现的：

```rust
VirtualScrollState
GlobalOffsetMapper
WindowPlanner
RenderWindow
HeightCorrectionPipeline
```

GUI 只做：

```text
wheel event
  -> ScrollInput
  -> VirtualScrollState.scroll_by_delta
  -> target_for_global_offset
  -> WindowPlanner.plan
  -> RenderWindow commit
```

禁止：

```text
local ListState 反向驱动 global scroll。
```

如果 GPUI 仍需要局部 list：

```text
ListState 只能显示当前 window 内的小列表。
ListState scroll offset 是 window-local。
global_scroll_top 只能来自 VirtualScrollState。
```

---

## 9. Entity 生命周期

V2 的 entity cache：

```rust
pub struct GuiEntityCache {
    blocks: HashMap<BlockId, Entity<BlockViewEntity>>,
    pins: HashMap<BlockId, HashSet<PinReason>>,
    lru: VecDeque<BlockId>,
}
```

pin 来源：

```text
Focus
Composition
SelectionEndpoint
Dirty
AsyncTask
RecentEdit
DragSource
SlashMenu
```

evict 规则：

```text
不在 render window
不 pinned
不 dirty
无 composition
无 async task
```

释放 entity 不释放：

```text
payload truth
layout truth
media resource truth
```

---

## 10. 迁移路径

### Step 1：V2 payload DTO

新增：

```text
src/core/rich_text
```

从第一版迁移并重命名：

```text
BlockKind       -> RichBlockKind
InlineSpan      -> InlineSpan
InlineMark      -> InlineMark
TableData       -> TablePayload
AssetData       -> ImagePayload/FilePayload
WhiteboardData  -> WhiteboardPayload
```

同时实现：

```rust
kind_tag <-> RichBlockKind
BlockPayloadRecord
plain_text extraction
height estimate by payload
```

---

### Step 2：V2 DocumentRuntime

新增：

```text
src/runtime/document_runtime.rs
src/runtime/payload_window.rs
src/runtime/view_projection.rs
```

把现有各模块串起来：

```text
DocumentIndex
VisibleDocumentIndex
BlockHeightIndex
PageLayoutIndex
VirtualScrollState
EditingSession
PayloadWindow
TraceEventLog
```

---

### Step 3：最小 GPUI View

新增：

```text
src/gui
```

先只支持：

```text
Paragraph
Heading
Quote
Code placeholder
Table placeholder
Image placeholder
```

输入先支持：

```text
focus block
insert text
backspace
enter split block
```

滚动先支持：

```text
VirtualScrollState + RenderWindow
```

---

### Step 4：迁移 editor2 renderer

按模块迁移：

```text
list_prefix
plain_text/text_element
gutter
code_block
table
image
whiteboard/mind_map
slash_menu
clipboard
```

每迁移一个模块都要改造成：

```text
输入 ViewBlockSnapshot
输出 GPUI element
事件发 GuiEditorCommand
不直接修改 payload truth
```

---

### Step 5：SQLite / 大文档接入

真实大文档路径：

```text
DocumentStore.load_document_index
DocumentStore.load_block_payloads(window)
DocumentStore.load_block_layouts
DocumentRuntime.open
CditorV2View render projection
```

---

## 11. 文件结构建议

最终建议结构：

```text
src/
  core/
    rich_text/
      mod.rs
      block_kind.rs
      inline.rs
      payload.rs
      attrs.rs
      table.rs
      asset.rs

  runtime/
    document_runtime.rs
    payload_window.rs
    view_projection.rs

  gui/
    mod.rs
    app.rs
    cditor_v2.rs
    theme.rs
    block_view.rs
    block_renderer.rs
    gutter.rs
    list_prefix.rs
    text_element.rs
    code_block.rs
    table.rs
    image.rs
    input.rs
    scroll.rs
    debug_overlay.rs
```

---

## 12. 最小可运行目标

第一阶段 GUI 验收目标：

```text
cargo run
  -> 打开 GPUI 窗口
  -> 使用 V2 DocumentRuntime
  -> 渲染 1000 个 block 的当前窗口
  -> 可编辑当前 paragraph
  -> 输入走 SingleCharInputHotPath
  -> 当前编辑 block pin
  -> Debug overlay 显示 global_scroll_top / window / height / shape_count
```

非目标：

```text
暂不接完整 table 编辑。
暂不接 whiteboard/mind map 编辑。
暂不接真实 SQLite 大文档。
暂不接完整 slash menu。
```

---

## 13. 和第一版 editor2 的对应关系

| 第一版 editor2 | V2 新位置 | 处理方式 |
|---|---|---|
| `NativeDocument` | `DocumentStore + DocumentRuntime` | 不作为 GUI 真相 |
| `BlockRecord` | `BlockIndexRecord + BlockPayloadRecord` | 拆分结构和内容 |
| `BlockKind` | `RichBlockKind` | 迁移并扩展 |
| `InlineSpan/InlineMark` | `core/rich_text/inline.rs` | 迁移 |
| `Cditor2` | `gui::CditorV2View` | 重写 |
| `CditorRuntimeDocument` | `runtime::DocumentRuntime` | 重写 |
| `CditorIndexedRuntimeDocument` | `DocumentRuntime + PayloadWindow + WindowPlanner` | 合并重写 |
| `CditorBlock` | `gui::BlockViewEntity` | 只保留 UI cache |
| `CditorBlockTextElement` | `gui::RichTextElement` | 改成版本化 cache，上报测量 |
| `RuntimeHeightIndex` | `BlockHeightIndex + PageLayoutIndex` | 使用 V2 已有实现 |
| `ListState` | window-local list only | 不能成为全局 scroll truth |
| `ScrollbarThumb` | V2 `ScrollbarDragSession` + visual | 借鉴 UI，逻辑用 V2 |

---

## 14. 关键约束

迁移时必须遵守：

```text
1. 不让 Entity<CditorBlock> 持有文档真相。
2. 不让 ListState 持有全局 scroll 真相。
3. selection 必须在 DocumentSelection。
4. payload 修改必须生成 EditTransaction。
5. 输入 hot path 不做同步 SQLite / FTS / full shaping。
6. height measure 结果必须版本校验。
7. async hydrate 结果必须 generation 校验。
8. 当前编辑 block / composition block / selection endpoint 必须 pin。
9. debug overlay 和 trace event 必须能解释 jitter。
10. 所有复杂 block 必须有 stable height / internal virtualization 策略。
```

---

## 15. 推荐下一步实现

下一步不要继续依赖第一版 `cditor::Cditor2` 启动，而是开始实现 V2 自己的 GUI：

1. 新增 `core/rich_text` DTO。
2. 新增 `runtime/document_runtime.rs` 串联 V2 index/scroll/editing。
3. 新增 `gui::CditorV2View`，先渲染 paragraph/heading。
4. `main.rs` 改为启动 `gui::CditorV2View`。
5. 再逐步迁移第一版 renderer 的 code/table/image/gutter/text element。

这样最终 GUI 才是真正接当前 V2 后端，而不是 path dependency 到第一版。
