# V1 Block Gutter / List 层级 / 背景逻辑迁移方案

> 目标：把 `/Users/jychen/Desktop/Cditor` V1/editor2 里已经验证过的 block gutter、列表层级、有序列表编号、任务列表 checkbox、不同 block 背景逻辑，体系化迁移到 CDitor-V2。不能破坏 10w block 大文档虚拟滚动性能，必须符合 V2 架构：runtime 是真相，UI 不是数据真相，global scroll 是唯一滚动真相，block 高度由统一 layout/metrics/provider 管理。

---

## 1. V1 实现在哪里

V1 源码路径：

```txt
/Users/jychen/Desktop/Cditor
```

与本方案相关的核心文件：

```txt
/Users/jychen/Desktop/Cditor/src/editor2/block/render.rs
/Users/jychen/Desktop/Cditor/src/editor2/block/entity.rs
/Users/jychen/Desktop/Cditor/src/editor2/component/gutter/mod.rs
/Users/jychen/Desktop/Cditor/src/editor2/component/list_prefix/mod.rs
/Users/jychen/Desktop/Cditor/src/editor2/component/plain_text/mod.rs
/Users/jychen/Desktop/Cditor/src/editor2/runtime/indexed_document.rs
/Users/jychen/Desktop/Cditor/src/editor/mod.rs
/Users/jychen/Desktop/Cditor/src/document/state.rs
/Users/jychen/Desktop/Cditor/src/theme.rs
```

---

## 2. V1 是怎么实现的

### 2.1 Block 渲染结构

V1/editor2 的 block 渲染入口在：

```txt
src/editor2/block/render.rs
```

核心结构不是把缩进、gutter、prefix、content 混在一起，而是分层：

```txt
CditorBlock root
  rounded / border / bg / mouse / keyboard / focus
  └─ indent wrapper
      padding-left = list_info.depth * 24px
      └─ row
          ├─ gutter slot: 24px x 22px
          │   └─ render_block_gutter(...)  // hover/action 时显示
          └─ content surface
              rounded / kind background / kind border / quote bar / padding
              ├─ render_block_prefix(...)  // list marker / number / checkbox / callout icon
              └─ actual block content
                  ├─ text element
                  ├─ code block
                  ├─ table
                  ├─ image
                  ├─ whiteboard
                  └─ mind map
```

V1 关键代码特征：

```rust
let visual = self.kind().block_style(theme);
let gutter_visible = (action_active && action_root) || (!action_active && self.hovered);
let outer_depth = self.list_info.depth;

.child(
    div().pl(px(outer_depth as f32 * 24.0)).child(
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .on_hover(...)
            .child(gutter slot)
            .child(content surface),
    ),
)
```

注意：V1 缩进是由 `list_info.depth` 控制的，不是直接读 block tree depth 随便 margin。gutter 有固定 slot，即使 gutter 不显示，也保留占位，使正文对齐稳定。

### 2.2 Gutter 实现

文件：

```txt
src/editor2/component/gutter/mod.rs
```

V1 gutter 是独立组件：

```rust
render_block_gutter(action_active, cx)
```

样式：

```rust
.w(px(24.0))
.h(px(22.0))
.rounded(px(7.0))
.flex()
.items_center()
.justify_center()
.bg(theme.gutter_background 或 theme.action_background)
.hover(theme.action_hover_background)
.cursor_pointer()
.child(svg(GUTTER_ICON))
```

事件：

```rust
.on_mouse_down(MouseButton::Left, ... {
    window.focus(&this.focus, cx);
    cx.emit(CditorBlockEvent::GutterAction { block_id });
    cx.emit(CditorBlockEvent::GutterDragStart { block_id, position });
    cx.stop_propagation();
})
```

也就是说 V1 gutter 同时承担：

- block action 激活入口
- drag start 入口
- hover/action 视觉反馈
- 独立点击区域

### 2.3 List / Todo / Callout prefix 实现

文件：

```txt
src/editor2/component/list_prefix/mod.rs
```

V1 prefix 宽度固定：

```rust
const LIST_PREFIX_WIDTH: f32 = 38.0;
```

不同 block kind 的 prefix：

#### Bulleted list

```rust
BlockKind::BulletedListItem => div()
    .w(px(LIST_PREFIX_WIDTH))
    .flex_shrink_0()
    .flex()
    .justify_center()
    .text_color(theme.prefix_text)
    .child(bullet_marker_for_depth(list_info.depth))
```

marker 按 depth 轮换：

```rust
match depth % 3 {
    0 => "•",
    1 => "◦",
    _ => "▪",
}
```

#### Numbered list

```rust
BlockKind::NumberedListItem => div()
    .w(px(LIST_PREFIX_WIDTH))
    .flex_shrink_0()
    .flex()
    .justify_center()
    .text_color(theme.prefix_text)
    .child(format!("{}.", list_info.numbered_ordinal.unwrap_or(1)))
```

编号来自 `BlockListInfo.numbered_ordinal`，不是 visible index。

#### Task list

```rust
BlockKind::TaskListItem { checked } => div()
    .w(px(LIST_PREFIX_WIDTH))
    .flex_shrink_0()
    .flex()
    .items_center()
    .justify_center()
    .when(editable, |this| this.cursor_pointer().on_mouse_down(... toggle checked ...))
    .child(render_task_checkbox(*checked))
```

checkbox 样式：

```rust
.size(px(16.0))
.rounded(px(4.0))
.border_1()
.border_color(checked ? action_accent : checkbox_border)
.bg(checked ? checkbox_checked_background : surface)
.child(if checked { "✓" } else { "" })
```

#### Callout

Callout 也走 prefix，但宽度和样式不同：

```rust
.w(px(34.0))
.pt(px(1.0))
.child(
    div()
      .size(px(24.0))
      .rounded(px(6.0))
      .bg(theme.callout_icon_background)
      .child(callout_icon(variant))
)
```

### 2.4 Block 背景逻辑

文件：

```txt
src/editor2/component/plain_text/mod.rs
```

V1 把 block style 作为 kind 的派生视觉模型：

```rust
trait BlockKindStyle {
    fn block_style(&self, theme: Editor2Theme) -> BlockVisualStyle;
}
```

`BlockVisualStyle`：

```rust
pub(crate) struct BlockVisualStyle {
    pub(crate) background: u32,
    pub(crate) border: u32,
    pub(crate) text: u32,
    pub(crate) padding_y: f32,
    pub(crate) min_height: f32,
    pub(crate) quote_bar: Option<u32>,
}
```

映射逻辑：

```rust
Heading => heading(level)
Quote => quote()
Callout => callout()
_ => paragraph()
```

具体效果：

- Paragraph/List/Todo 默认：`surface` 背景，`surface` border。
- Heading：更大的 padding/min_height。
- Quote：`quote_text`，左侧 quote bar，背景仍 surface。
- Callout：`callout_background` + `callout_border`，更大 padding/min_height。
- Code block 在 block render 中识别 `is_code` 后走专门 renderer 和 theme code background。

V1 block content surface 渲染重点：

```rust
.relative()
.min_w(px(0.0))
.w_full()
.min_h(px(visual.min_height))
.rounded(...)
.bg(action_active ? theme.action_background : visual.background)
.border_color(visual.quote_bar.unwrap_or(visual.border))
.border_l(if quote_bar { 4px } else { 1px })
.pl(if quote_bar { 8px } else if callout { 10px } else { 0px })
.pr(if callout { 10px } else { 0px })
.py(visual.padding_y)
.flex()
.items_center()
.when(is_quote, |this| this.items_start())
.child(render_block_prefix(...))
.child(content)
```

### 2.5 ListInfo / depth / numbered ordinal

V1 的 `BlockListInfo` 定义在：

```txt
src/editor/mod.rs
```

```rust
pub struct BlockListInfo {
    pub depth: usize,
    pub numbered_ordinal: Option<usize>,
}
```

在 editor2 runtime 里，每个 block 的 list info 来自：

```rust
base_list_info_for_index(index)
  -> list_info_overrides_by_id
  -> index.meta_at(index).list_info
  -> default depth 0
```

然后如果是 numbered list：

```rust
list_info.numbered_ordinal = Some(numbered_ordinal_for_index(index, list_info.depth))
```

### 2.6 有序列表编号算法

文件：

```txt
src/editor2/runtime/indexed_document.rs
```

核心函数：

```rust
fn numbered_ordinal_for_index(&self, index: usize, depth: usize) -> usize {
    let mut ordinal = 1usize;
    let mut previous = index;
    while let Some(candidate) = previous.checked_sub(1) {
        let candidate_info = self.base_list_info_for_index(candidate);
        if candidate_info.depth < depth {
            break;
        }
        if candidate_info.depth == depth {
            if self.is_numbered_list_item_at(candidate) {
                ordinal += 1;
            } else {
                break;
            }
        }
        previous = candidate;
    }
    ordinal
}
```

语义：

- 从当前 block 向前找。
- 如果遇到更浅 depth，停止。
- 同 depth 且是 numbered item，ordinal + 1。
- 同 depth 但不是 numbered item，停止并重启。
- 更深 depth 会跳过，因为那是前一个 sibling 的子树。

这正好解决：

- 同级连续 numbered list 递增。
- 遇到 bullet/task/paragraph 重启。
- nested list 不污染父级编号。

### 2.7 缩进 / 反缩进

V1 缩进逻辑在：

```txt
src/editor2/runtime/indexed_document.rs
```

#### Indent

```rust
fn indent_block(&mut self, block_id, cx) -> Option<RuntimeSaveEvent> {
    let index = self.index.index_of(block_id)?;
    let previous_index = index.checked_sub(1)?;
    let previous_id = self.index.id_at(previous_index)?;

    if previous block 不支持 children {
        return None;
    }

    let previous_info = self.list_info_for_index(previous_index);
    let current_info = self.list_info_for_index(index);
    let new_info = BlockListInfo {
        depth: previous_info.depth + 1,
        numbered_ordinal: current_info.numbered_ordinal,
    };

    list_info_overrides_by_id.insert(block_id, new_info);
    entity.set_list_info(new_info);
    list_state.remeasure_items(list_index..list_index + 1);

    emit EditOperation::MoveBlock {
        block_id,
        new_parent_id: Some(previous_id),
        new_position: 0,
    }
}
```

要点：

- 只有当前一个 block 支持 children 时才能 indent。
- 缩进不是只改 UI margin，而是结构事务 `MoveBlock`。
- UI 先乐观更新 `list_info_overrides_by_id`，同时发保存事务。
- 只 remeasure 当前 item，不全量刷新。

#### Outdent

```rust
fn outdent_block(...) {
    if current_info.depth == 0 { return None; }

    new_info.depth = current_info.depth - 1;
    list_info_overrides_by_id.insert(block_id, new_info);
    entity.set_list_info(new_info);
    list_state.remeasure_items(current_item);

    emit MoveBlock { new_parent_id: None, new_position: index }
}
```

### 2.8 空列表 Enter 行为

V1 对空 list/task enter 有特殊行为：

```rust
if trailing empty && current title empty && exits_on_empty_enter(kind) {
    let depth = list_info_for_index(after_index).depth;
    if depth > 0 && is_list_item_kind(kind) {
        return outdent_block(after_id, cx);
    }
    set_kind(Paragraph);
    remeasure current item;
    emit SetBlockKind Paragraph;
}
```

语义：

- 空的根 list item 按 Enter 退出为 Paragraph。
- 空的嵌套 list item 按 Enter 先 outdent。

---

## 3. V2 当前差距

当前 V2 文件：

```txt
src/gui/block/block_shell.rs
src/gui/block/list.rs
src/gui/block/block_view.rs
src/gui/theme.rs
src/runtime/view_projection.rs
src/runtime/document_runtime.rs
```

主要差距：

- [ ] `GuiTheme` 缺少 V1 的 gutter/list/callout/checkbox/action tokens。
- [ ] `ViewBlockSnapshot` 只有 `depth`，没有 `BlockListInfo` / `numbered_ordinal` / chrome snapshot。
- [ ] `block_shell.rs` 直接 `ml(depth * 24)`，没有 V1 的 gutter slot + content surface。
- [ ] `list.rs` 用 text prefix，且 numbered list 使用 `visible_index + 1`，语义错误。
- [x] Todo checkbox 已有独立 hit area，点击可走 runtime toggle。
- [x] Callout icon/prefix 已迁移。
- [ ] Quote/callout/code/list 背景逻辑没有统一 `BlockVisualStyle`。
- [ ] Indent/outdent 还没有按 V1 的结构事务语义接入 V2。
- [ ] 空 list enter 退出/反缩进语义未完整恢复。

---

## 4. 工程目录设计

### 4.1 目标目录结构

迁移 V1 block chrome 不能继续把逻辑堆在 `block_shell.rs`、`list.rs`、`block_view.rs`。V2 需要先把目录边界定清楚：

```txt
src/
  core/
    block/
      mod.rs
      list_info.rs          # BlockListInfo、编号/层级纯算法，可被 runtime 测试直接使用
      chrome.rs             # 与 GUI 无关的 BlockChromeSnapshot / BlockPrefixSnapshot
    layout/
      block_metrics.rs      # 继续作为高度估算唯一入口，接收 chrome/min-height 规则

  runtime/
    document_runtime.rs     # 只负责把 index/payload/selection 投影成 ViewBlockSnapshot
    view_projection.rs      # ViewBlockSnapshot / EditorViewProjection 数据结构
    list_projection.rs      # projection 阶段 list_info/numbered ordinal/cache 计算

  gui/
    theme.rs                # 主题 token，只放颜色/尺寸 token，不放逻辑
    block/
      mod.rs
      block_view.rs         # block 总分发，保持薄
      block_shell.rs        # block 外层结构，消费 chrome style，不写 kind 规则
      chrome.rs             # GUI BlockChromeStyle：把 snapshot + theme 转成样式
      gutter.rs             # gutter slot / icon / hover / action / drag start
      prefix.rs             # bullet / numbered / todo checkbox / callout icon / toggle prefix
      paragraph.rs
      heading.rs
      quote.rs
      code.rs
      list.rs               # 可删除或仅 re-export prefix/list helpers，不能再放 marker 真相
      placeholder.rs
    input/
      command.rs            # IndentBlock / OutdentBlock / ToggleTodo 等命令枚举
      keyboard.rs           # Tab / Shift+Tab / checkbox 快捷键映射
      mouse.rs              # block focus、文本拖选、gutter drag controller
    overlay/
      selection_overlay.rs  # 只画 projection/selection overlay，不算 selection 真相
```

### 4.2 目录职责边界

| 目录/文件 | 可以放什么 | 禁止放什么 |
|---|---|---|
| `core/block/list_info.rs` | list depth/ordinal 纯算法、`BlockListInfo` | GPUI、theme、payload loading |
| `core/block/chrome.rs` | 与 UI 无关的 `BlockChromeSnapshot`、`BlockPrefixSnapshot` | 颜色、px、GPUI element |
| `runtime/list_projection.rs` | 根据 `DocumentIndex` 计算 window 内 list info、ordinal cache | 任何 GPUI 代码、文本绘制 |
| `runtime/view_projection.rs` | projection 数据结构 | 具体 UI 样式计算 |
| `gui/block/chrome.rs` | `BlockChromeStyle::from_snapshot(block, theme)` | 修改 runtime、计算全局编号 |
| `gui/block/block_shell.rs` | V1 那种 root/indent/gutter/content surface 结构 | kind-specific 大量 match、编号算法 |
| `gui/block/gutter.rs` | gutter UI 和 gutter 鼠标事件入口 | 文本 selection、payload 修改 |
| `gui/block/prefix.rs` | bullet/number/todo/callout/toggle prefix UI | block 正文内容渲染、编号计算 |
| `gui/block/block_view.rs` | 按 kind 组合 shell/content/prefix | 直接写样式细节 |
| `gui/theme.rs` | token | 行为逻辑 |
| `core/layout/block_metrics.rs` | 高度规则唯一入口 | GPUI 测量细节、hover 状态 |

### 4.3 数据流设计

```txt
DocumentIndex / PayloadWindow / Selection
  -> DocumentRuntime::projection_for_window
  -> runtime/list_projection.rs
      computes BlockListInfo / numbered ordinal / prefix snapshot
  -> ViewBlockSnapshot
      kind, depth, list_info/chrome, selection_range, focused, payload, layout
  -> BlockView
  -> BlockShell
      uses gui/block/chrome.rs for style
      renders gui/block/gutter.rs
      renders gui/block/prefix.rs
      renders content renderer
```

关键原则：

- list ordinal 真相在 runtime projection，不在 GUI。
- GUI 只消费 `ViewBlockSnapshot`，不向前扫描文档。
- gutter/prefix/背景都是 chrome，不属于 payload text。
- hover/action 状态可以在 GUI，但不能影响 runtime 高度真相。
- content surface 的 padding/min height 如果影响外高，必须同步到 `block_metrics.rs`。

### 4.4 建议新增数据结构

#### `core/block/list_info.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockListInfo {
    pub depth: usize,
    pub numbered_ordinal: Option<usize>,
}
```

#### `core/block/chrome.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockChromeSnapshot {
    pub list_info: BlockListInfo,
    pub prefix: BlockPrefixSnapshot,
    pub has_children: bool,
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockPrefixSnapshot {
    None,
    Bullet { depth: usize },
    Number { ordinal: usize },
    Todo { checked: bool },
    Callout { variant: CalloutVariant },
    Toggle { collapsed: bool },
}
```

#### `gui/block/chrome.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockChromeStyle {
    pub indent_px: f32,
    pub gutter_width_px: f32,
    pub prefix_width_px: f32,
    pub content_min_height_px: f32,
    pub content_padding_y_px: f32,
    pub content_padding_left_px: f32,
    pub content_padding_right_px: f32,
    pub content_radius_px: f32,
    pub outer_background: u32,
    pub content_background: u32,
    pub content_border: u32,
    pub text_color: u32,
    pub quote_bar: Option<u32>,
}
```

### 4.5 文件迁移策略

不要一步把所有文件重写。按下面顺序安全迁移：

1. 先新增 `core/block/*`，不接 GUI。
2. 再新增 `runtime/list_projection.rs`，只改 projection 数据。
3. 再新增 `gui/block/chrome.rs`，保持旧 shell 不变，仅测试 style 输出。
4. 再新增 `gutter.rs` / `prefix.rs`，但先不删除旧 `list.rs`。
5. 最后重写 `block_shell.rs` 内部结构。
6. 通过测试后，删除或瘦身旧 `list.rs` 的 prefix 逻辑。

这样每一步都可以单独测试，不会一次性破坏滚动/输入/高度。

---

## 5. V2 迁移设计

### 5.1 新增 V2 block chrome model

建议新增：

```txt
src/gui/block/chrome.rs
```

定义：

```rust
pub struct BlockChromeStyle {
    pub outer_padding_y: f32,
    pub indent_px: f32,
    pub gutter_width_px: f32,
    pub prefix_width_px: f32,
    pub content_min_height_px: f32,
    pub content_padding_y_px: f32,
    pub content_padding_left_px: f32,
    pub content_padding_right_px: f32,
    pub content_radius_px: f32,
    pub outer_background: u32,
    pub content_background: u32,
    pub content_border: u32,
    pub text_color: u32,
    pub quote_bar: Option<u32>,
}
```

定义：

```rust
pub enum BlockPrefixKind {
    None,
    Bullet { depth: usize },
    Number { ordinal: usize },
    Todo { checked: bool },
    Callout { variant: CalloutVariant },
    Toggle { collapsed: bool },
}
```

### 5.2 新增 runtime projection chrome 字段

在 `ViewBlockSnapshot` 增加：

```rust
pub list_info: BlockListInfo,
pub prefix: BlockPrefixKind,
```

或者更少耦合：

```rust
pub chrome: BlockChromeSnapshot,
```

其中：

```rust
pub struct BlockChromeSnapshot {
    pub depth: usize,
    pub numbered_ordinal: Option<usize>,
    pub has_children: bool,
    pub collapsed: bool,
}
```

### 5.3 Numbered ordinal 性能策略

V1 是向前扫描，窗口内小规模够用，但 V2 是 10w block，需要约束：

Phase 1 可接受：

- projection window 约 100 block。
- numbered ordinal 在 projection 期间只对 visible blocks 计算。
- 每个 visible numbered block 向前扫描到同 depth 起点。
- 这在极端 10w 连续 numbered list 时可能退化。

Phase 2 必须优化：

- `DocumentRuntime` 维护 `list_ordinal_cache: Vec<Option<usize>>`。
- cache keyed by `structure_version`。
- 构建/结构变更时 O(n) 重算；不在单字符输入 hot path 中做。
- 后续做 page checkpoint / sibling range incremental rebuild。

### 5.4 BlockShell 渲染结构

V2 目标结构：

```txt
block_shell root absolute height fixed by layout_meta
  rounded / full row mouse area / focus event
  background full row only for selected/focused/hover if needed
  └─ indent row
      padding-left = chrome.indent_px
      └─ flex row
          ├─ gutter slot width 24
          │   └─ render_gutter if hover/action/focused
          └─ content surface
              ├─ prefix slot width 38
              └─ content renderer
```

注意：不能让 `indent` 改变 absolute block outer height，也不能让 hover/action 影响高度。

### 5.5 背景和高度关系

所有背景、border、gutter、prefix 必须满足：

- 不改变 `BlockLayoutMeta.effective_height` 的语义。
- content surface padding/min height 需要纳入 `block_metrics`。
- 迁移 V1 `BlockVisualStyle.min_height/padding_y` 时，同步更新 `src/core/layout/block_metrics.rs`。
- 不能在 GUI 中临时撑高 block 但 runtime height 不变。

---

## 6. 任务清单

### Phase 0：工程目录骨架

- [x] DIR-001 新增 `src/core/block/mod.rs`。
- [x] DIR-002 新增 `src/core/block/list_info.rs`。
- [x] DIR-003 新增 `src/core/block/chrome.rs`。
- [x] DIR-004 在 `src/core/mod.rs` 导出 `block` 模块。
- [x] DIR-005 新增 `src/runtime/list_projection.rs`。
- [x] DIR-006 在 `src/runtime/mod.rs` 导出/声明 `list_projection`。
- [x] DIR-007 新增 `src/gui/block/chrome.rs`。
- [x] DIR-008 新增 `src/gui/block/gutter.rs`。
- [x] DIR-009 新增 `src/gui/block/prefix.rs`。
- [x] DIR-010 更新 `src/gui/block/mod.rs` 模块导出。
- [ ] DIR-011 明确 `src/gui/block/list.rs` 后续只保留兼容 re-export 或删除。
- [x] DIR-012 给每个新增模块写单元测试，确保目录骨架可编译。

### Phase A：V1 行为建模与文档对齐

- [x] A-001 定位 V1 gutter 实现：`editor2/component/gutter/mod.rs`。
- [x] A-002 定位 V1 list prefix 实现：`editor2/component/list_prefix/mod.rs`。
- [x] A-003 定位 V1 block shell/render 实现：`editor2/block/render.rs`。
- [x] A-004 定位 V1 block style 实现：`editor2/component/plain_text/mod.rs`。
- [x] A-005 定位 V1 list info / numbered ordinal 实现：`editor2/runtime/indexed_document.rs`。
- [x] A-006 写出 V1 -> V2 迁移方案文档。

### Phase B：Theme token 迁移

- [x] B-001 在 `src/gui/theme.rs` 扩展 V2 `GuiTheme`。
  - [x] 增加 `hover_surface`。
  - [x] 增加 `action_background`。
  - [x] 增加 `action_hover_background`。
  - [x] 增加 `action_accent`。
  - [x] 增加 `gutter_background`。
  - [x] 增加 `gutter_foreground`。
  - [x] 增加 `prefix_text`。
  - [x] 增加 `quote_text`。
  - [x] 增加 `quote_bar`。
  - [x] 增加 `callout_background`。
  - [x] 增加 `callout_border`。
  - [x] 增加 `callout_icon_background`。
  - [x] 增加 `checkbox_border`。
  - [x] 增加 `checkbox_checked_background`。
  - [x] 增加 `checkbox_checked_text`。
- [x] B-002 为 light theme 填入 V1 token 值。
- [x] B-003 更新 theme 相关测试，保证 token 稳定。

### Phase C：Runtime list info / ordinal projection

- [x] C-001 在 V2 core/runtime 定义 `BlockListInfo`。
  - [x] 字段 `depth: usize`。
  - [x] 字段 `numbered_ordinal: Option<usize>`。
- [x] C-002 在 `ViewBlockSnapshot` 增加 `chrome` 字段。
- [x] C-003 实现 runtime list projection cache。
  - [x] 读取 `DocumentIndex.depths[index]`。
  - [x] 判断当前 kind 是否 `NumberedList`。
  - [x] 为 numbered list 填入 ordinal。
- [x] C-004 实现 numbered ordinal cache 算法。
  - [ ] 同 depth 连续 numbered 累加。
  - [ ] 更浅 depth 停止。
  - [ ] 同 depth 非 numbered 停止。
  - [ ] 更深 depth 跳过。
- [ ] C-005 添加 runtime 测试。
  - [ ] 连续 numbered list 产生 1、2、3。
  - [ ] paragraph 后 numbered 重启为 1。
  - [ ] bullet/task 后 numbered 重启为 1。
  - [ ] nested child 不污染 parent ordinal。
  - [ ] parent numbered ordinal 不受 child numbered 影响。
- [ ] C-006 性能保护。
  - [ ] projection 只对 visible window 计算。
  - [ ] 添加 10w mixed demo projection 性能测试或复用现有 window 限制测试。

### Phase D：Block chrome style provider

- [x] D-001 新增 `src/gui/block/chrome.rs`。
- [x] D-002 实现 `BlockChromeStyle::from_snapshot(block, theme)`。
- [x] D-003 迁移 V1 paragraph style。
- [x] D-004 迁移 V1 heading style。
- [x] D-005 迁移 V1 quote style。
- [x] D-006 迁移 V1 callout style。
- [x] D-007 迁移 V1 action/focused/selected 外层背景规则。
- [x] D-008 写 chrome style 单元测试。
  - [ ] depth 变成 indent px。
  - [ ] quote 有 quote bar。
  - [ ] callout 有 callout background。
  - [ ] heading min height/padding 正确。

### Phase E：Gutter 组件迁移

- [x] E-001 新增 `src/gui/block/gutter.rs`。
- [x] E-002 迁移 V1 gutter 样式。
  - [x] 固定 `24x22`。
  - [x] rounded 7。
  - [x] hover background。
  - [x] action active background。
- [x] E-003 使用 V2 现有 icon 资源或迁移 `assets/icons/gutter.svg`。
- [x] E-004 接入 block hover/focus 显示逻辑。
  - [x] 未 hover 时保留 slot 但不显示 icon。
  - [x] hover 时显示。
  - [x] action active 时显示。
- [x] E-005 gutter mouse down 事件接入。
  - [x] focus block。
  - [x] stop propagation。
  - [x] 后续 action/drag 事件预留。
- [x] E-006 添加 GUI 测试。

### Phase F：List prefix / Todo checkbox / Callout icon 迁移

- [x] F-001 新增 `src/gui/block/prefix.rs` 为 prefix 组件。
- [x] F-002 实现 `render_block_prefix(prefix, theme, editable)`。
- [x] F-003 bullet marker 按 depth `% 3` 显示 `• / ◦ / ▪`。
- [x] F-004 numbered marker 使用 `list_info.numbered_ordinal`。
- [x] F-005 todo checkbox 迁移。
  - [x] 固定 16px。
  - [x] checked background。
  - [x] checked text `✓`。
  - [x] unchecked border。
  - [x] click toggle 事件。
- [x] F-006 callout icon prefix 迁移。
- [x] F-007 prefix slot 固定宽度 38px。
- [x] F-008 删除旧 `render_numbered(block.visible_index + 1)` 逻辑。
- [ ] F-009 添加测试。
  - [x] bullet depth 0/1/2 marker。
  - [x] numbered 使用 ordinal。
  - [x] todo checked 状态从 payload/runtime projection 进入 prefix。
  - [ ] todo checkbox checked/unchecked style。

### Phase G：BlockShell 结构重写

- [x] G-001 改 `block_shell.rs`，不再对 root 使用 `ml(depth * 24)`。
- [x] G-002 root 保持 full width，用于 full-block selection/focus/hover。
- [x] G-003 内部增加 indent wrapper：`pl(depth * 24)`。
- [x] G-004 增加 gutter slot，固定宽度。
- [x] G-005 增加 content surface，应用 `BlockChromeStyle`。
- [x] G-006 prefix 渲染移入 content surface 开头。
- [x] G-007 content renderer 只负责 payload 内容，不再负责 list/todo marker。
- [x] G-008 保证 shell 结构不改变 absolute block outer height。
- [x] G-009 更新 block shell tests。

### Phase H：高度 estimator 对齐

- [x] H-001 审核 V1 `BlockVisualStyle.min_height/padding_y` 与 V2 `block_metrics` 差异。
- [x] H-002 将 shell/content surface/code label chrome 纳入统一 metrics；gutter/prefix 只占横向 slot，不增加纵向高度。
- [x] H-003 list/todo/callout/quote 初始高度使用同一 chrome metrics。
- [x] H-004 measured height normalize 继续使用 `normalize_text_inner_measured_height`，且 chrome_y 匹配新 shell。
- [x] H-005 添加回归测试。
  - [x] list/todo/quote 显式多行高度增长。
  - [x] todo 多行不覆盖下一 block（通过 outer height >= 行高 + shell chrome 契约覆盖）。
  - [x] quote/callout/code 不重叠。

### Phase I：Indent / Outdent / Empty Enter 行为

- [x] I-001 增加 V2 command：`IndentBlock` / `OutdentBlock`。
- [x] I-002 keyboard 接 Tab / Shift+Tab。
- [x] I-003 runtime 实现 focused block 结构缩进。
  - [x] previous block 必须 supports_children。
  - [x] 更新 parent/depth/index。
  - [x] 更新 affected range projection/list cache；layout height 保持既有 meta，不触发全量重测。
  - [x] 不进入单字符 hot path。
- [x] I-004 runtime 实现 focused block outdent。
- [x] I-005 空 list enter 行为。
  - [x] root empty list -> Paragraph。
  - [x] nested empty list -> outdent。
- [x] I-006 添加 focused block 结构事务与空 list Enter 测试。

### Phase J：性能验收

- [x] J-001 `cargo test gui --lib` 通过。
- [x] J-002 `cargo test runtime::document_runtime --lib` 通过。
- [x] J-003 `cargo check` 通过。
- [x] J-004 `large_mixed_demo_keeps_payloads_windowed` 通过。
- [ ] J-005 10w demo projection blocks 仍约 100~120。
- [ ] J-006 render total 不因 gutter/prefix 引入 O(total_blocks) 行为。
- [ ] J-007 滚动不因 list ordinal 计算出现卡顿。
- [ ] J-008 输入 hot path 不触发全量 ordinal/cache rebuild。

---

## 6. 迁移顺序建议

推荐顺序：

1. Theme tokens。
2. Runtime `BlockListInfo` + numbered ordinal projection。
3. Chrome style provider。
4. Gutter component。
5. Prefix component。
6. 重写 BlockShell 内部布局。
7. 对齐 block_metrics 高度。
8. Indent/outdent/empty enter。
9. 性能验收。

原因：

- 先做 projection/list_info，GUI 才有稳定输入。
- 先做 style provider，避免 `block_shell.rs` 堆 if。
- shell 重写前先有 gutter/prefix 组件，避免中间状态混乱。
- 高度最后统一校准，避免边做边补丁。

---

## 7. 和 V2 大文档方案的契合点

必须遵守：

- Runtime 是真相，GUI 不保存 list ordinal 真相。
- GUI 只渲染 projection window。
- Gutter/prefix 不能导致全量 entity 创建。
- Numbered ordinal 不能每帧全量扫描 10w。
- Block outer height 仍由 `BlockLayoutMeta` / `block_metrics` 管理。
- 背景/hover/gutter 不应影响高度，除非同步更新 metrics。
- 输入/IME hot path 不做结构 cache 全量重算。

本迁移方案将 V1 的视觉/交互模型迁移为 V2 projection/chrome model，而不是恢复 V1 的 ListState 全局真相或 per-block entity 真相。
