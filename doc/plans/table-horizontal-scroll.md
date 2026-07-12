# 表格横向滚动功能

## 问题描述

当表格的列数较多或列宽较大时，表格的总宽度可能超出文档的可用宽度（812px，即文档页面宽度 860px 减去左右 padding 48px）。此时表格会超出文档边界，导致内容不可见或布局错乱。

## 解决方案

在表格渲染时，检测表格宽度是否超过文档可用宽度。如果超过，则将表格包裹在一个横向可滚动的容器中，使用户可以通过滚动查看完整的表格内容。

## 实现细节

### 文件修改

**`crates/app/src/gui/block/table/render.rs`**

始终把表格包裹在一个「填满可用内容宽度」的横向滚动视口里：

```rust
// 表格内容：固定为自身的自然宽度，不被 flex 父容器压缩
let table_content = div()
    .relative()
    .flex_none()
    .w(px(table_view.width_px))
    .h(px(table_view.height_px))
    // ... 边框 / 背景 / 单元格 ...
    ;

// 滚动视口：w_full + min_w(0) 从父容器拿到确定宽度，
// 内容更宽时溢出并可横向滚动。
div()
    .id(("table_scroll_container", block_id))
    .w_full()
    .min_w(px(0.0))
    .flex()
    .overflow_x_scroll()
    .child(table_content)
    .into_any_element()
```

导入必要的 GPUI traits：
- `InteractiveElement` — 提供 `.id()` 方法
- `StatefulInteractiveElement` — 提供 `.overflow_x_scroll()` 方法

### 关键点：为什么之前滚动条不出现

之前的实现只给滚动容器设置了 `max_w(812)`，没有确定的宽度。在 GPUI 的
flex / taffy 布局里，只有 `max_w` 的块级子元素会按「内容的自然宽度」来排布
（即表格的完整宽度），因此**永远不会溢出**，只会把整个页面撑宽。

要让 `overflow_x_scroll` 真正裁剪并滚动，容器必须从父级拿到一个**确定的宽度**
（`w_full()` + `min_w(0)`），这样更宽的子元素才会溢出这个视口。同时子元素要用
`flex_none()` 保持自己的固定宽度，不被 flex 父容器压缩。

### 行为说明

- **表格宽度 ≤ 可用宽度**：表格按自然宽度显示，无溢出、无滚动
- **表格宽度 > 可用宽度**：表格溢出视口，可横向滚动查看隐藏列
  - 不再依赖固定阈值判断，直接由布局的实际可用宽度决定是否溢出
  - 通过触控板 / 鼠标滚轮横向滚动

### 可见滚动条：自绘实现

GPUI 的 `scrollbar_width` 默认为 `0`，即 `Overflow::Scroll` **不会自动绘制**
原生滚动条。因此仅靠 `overflow_x_scroll()` 只能用触控板 / 滚轮滚动，看不到滚动条。

为了让滚动条**可见且跟手**，采用「持久化 `ScrollHandle` + 自绘 thumb」方案：

1. **持久化 ScrollHandle**：每个表格 block 在 `CditorV2View.table_scroll_handles:
   HashMap<BlockId, ScrollHandle>` 里持有一个跨帧存活的滚动句柄。渲染顶层
   （`app/render.rs`）为当前窗口内的每个 table block 惰性创建句柄，并把只读
   快照沿渲染链传入（`DocumentEditorView` → `BlockView` → `block_content`
   → `render_table_block`）。

2. **绑定视口**：滚动视口调用 `.track_scroll(&handle)`，GPUI 在滚轮滚动后会
   `cx.notify(view)` 触发 view 重绘，因此自绘 thumb 会自动跟随最新偏移更新。

3. **自绘 thumb**：在滚动视口的外层 wrapper（不随内容滚动）里叠加一个
   `absolute` 定位到底部的横条。thumb 的宽度和位置由纯函数
   `table_hscroll_thumb(viewport_width, content_width, max_offset_x, offset_x)`
   计算：
   - `thumb_width = viewport_width * (viewport_width / content_width)`，
     并有最小宽度 `32px`；
   - `thumb_left = (viewport_width - thumb_width) * progress`，
     其中 `progress = -offset_x / max_offset_x`；
   - 当 `max_offset_x <= 0.5`（无溢出）或视口尚未布局（宽度为 0）时返回
     `None`，即不显示滚动条。

thumb 使用半透明灰（`0x8c959faa`）配合表面色描边，视觉上与文档主竖向滚动条
（`app/interaction/scrollbar.rs`）保持一致。

### 测试

新增纯函数单测（`table/mod.rs`）：
- `table_hscrollbar_hidden_when_content_fits_viewport` — 内容不溢出 / 未布局时不显示
- `table_hscrollbar_thumb_tracks_scroll_progress` — thumb 宽度与位置随滚动进度变化
- `table_hscrollbar_thumb_respects_minimum_width` — 超宽内容时 thumb 有最小宽度

所有 app 层表格测试（39 个）通过，`cargo check --workspace` 通过。

## 用户体验

1. **表格宽度 ≤ 可用宽度**：无滚动条，按自然宽度显示。
2. **表格宽度 > 可用宽度**：
   - 底部显示可见的横向滚动条，thumb 长度反映可见比例；
   - 触控板 / 滚轮横向滚动时 thumb 实时跟随；
   - 表格高度正常增长，不受横向滚动影响。

## 技术说明

### GPUI 滚动 API

- `overflow_x_scroll()` — 设置横向滚动（需元素有 `.id()`，来自 `StatefulInteractiveElement`）
- `.track_scroll(&ScrollHandle)` — 把视口绑定到持久化句柄
- `ScrollHandle::offset()` / `max_offset()` / `bounds()` — 读取实时滚动状态用于自绘 thumb

### 替代方案考虑

1. ❌ 仅 `max_w` + `overflow_x_scroll`：容器无确定宽度，不会溢出，滚动条不出现
2. ❌ 压缩列宽以适应文档宽度：会破坏用户设置的列宽
3. ❌ 依赖 GPUI 原生滚动条：默认不绘制，用户看不到
4. ✅ 持久化 `ScrollHandle` + 自绘 thumb：可见、跟手、可单测

## 未来改进

可选的未来增强：
1. 滚动条可拖拽 / 点击轨道跳转（当前为只读显示，滚动靠触控板/滚轮）
2. 滚动到活动单元格（编辑越界列时自动横向滚动）
3. 滚动位置持久化（切换文档后记住位置）
4. hover 时才显示滚动条（更接近 Notion 的隐藏式风格）

## 相关文件

- `crates/app/src/gui/block/table/render.rs` — 渲染 + 自绘滚动条 + 几何纯函数
- `crates/app/src/gui/block/table/mod.rs` — 单元测试
- `crates/app/src/gui/app/cditor_v2_view.rs` — `table_scroll_handles` 字段
- `crates/app/src/gui/app/lifecycle.rs` — 句柄的创建 / 清理
- `crates/app/src/gui/app/render.rs` — 顶层预建句柄并传入渲染链
- `crates/app/src/gui/document/document_surface.rs` — 文档宽度定义

## 完成时间

2026-07-09
