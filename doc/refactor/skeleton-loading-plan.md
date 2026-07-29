# Cditor Skeleton Loading 方案

## 背景

PostgreSQL / 网络后端加载 payload window 时存在真实延迟。当前占位体验有两个问题：

1. 单个 block payload 未加载时只显示 `Loading placeholder...` 文本，和编辑器真实块形态差异较大。
2. 整个 render window 还未覆盖 payload 时，`DocumentSurface` 只渲染一个空高度 spacer，用户看到空白页，容易误判为光标丢失或无法编辑。

参考 Liora 的 `skeleton.rs`，Cditor 需要自己的 GPUI skeleton 组件，但不能直接依赖 Liora 的 theme / motion / Config。

## 目标

- 网络加载、cold start、payload window 切换期间显示 Notion-like skeleton。
- 不把 skeleton 状态塞进 UI Entity；它只是 projection loading state 的纯渲染结果。
- 不改变 Runtime / document / index / selection / layout / scroll 的真相边界。
- 不把实现继续堆到 `cditor_v2_view.rs` 或 `block_content.rs`。
- 不引入 Liora 依赖；只借鉴组件形态和 builder API。

## 目录规划

```text
src/gui/skeleton/
  mod.rs              # 基础 skeleton 组件导出
  primitives.rs       # SkeletonItem / SkeletonVariant / SkeletonRows

src/gui/block/
  skeleton.rs         # 根据 RichBlockKind 渲染 block skeleton
  placeholder.rs      # Error/loading glue，调用 block skeleton

src/gui/document/
  skeleton_window.rs  # 整窗 placeholder skeleton，替代空 spacer
  document_surface.rs # 只接收 skeleton window element，不写具体样式
```

## Block kind 骨架策略

| Block kind | Skeleton 形态 |
| --- | --- |
| Paragraph / List / Todo | 1-2 行文本骨架，最后一行较短；列表额外预留 prefix 区域 |
| Heading | 更高、更粗的一行骨架，宽度 50%-70% |
| Quote / Callout | 左侧竖线/卡片背景 + 文本行骨架 |
| Code | 代码块背景 + 多行短横线骨架 |
| Image | 16:9 或固定最小高度矩形骨架，居中 |
| Table | 表头 + 2-3 行网格骨架 |
| Divider / Separator | 一条浅色横线 |
| Unknown / fallback | paragraph skeleton |

## Loading 层级

### 1. Block payload placeholder

当 `ViewBlockSnapshot.payload` 是：

```rust
BlockPayloadView::Placeholder { .. }
BlockPayloadView::Loading { .. }
```

由 `src/gui/block/skeleton.rs` 根据 `block.kind` 渲染对应 skeleton。

### 2. Render window placeholder

当 `EditorViewProjection.placeholder_window_height.is_some()` 且 `projection.blocks` 为空时，说明当前 payload window 没覆盖 planned render window。

此时 `DocumentSurface` 不再只插入空 `div().h(height)`，而是在该高度区域顶部渲染若干 skeleton block，数量按高度上限裁剪，避免 10w 文档生成大量 UI 节点。

## 性能约束

- Skeleton window 最多渲染固定数量，例如 12-16 个骨架块。
- 不为每个未加载 block 创建 Entity。
- 不触发 runtime mutation。
- 不在 render 中请求 payload 或更新 view。
- 所有 skeleton 都是纯 GPUI element tree。

## 注意事项

1. Skeleton 不能替代真实 block 高度：高度仍来自 layout/index/projection。
2. Skeleton 不参与 hit-test / text input；加载完成后才可编辑。
3. 对于 cold start 首屏，应保证 initial payload window 至少覆盖首屏 planned range；skeleton 是体验兜底，不是数据加载策略替代。
4. 图片骨架不能用 `cover` 裁剪语义，只是灰色占位矩形。
5. 后续如果加动画，应封装在 `gui/skeleton` 内部，不把 motion 逻辑散到 block/document 层。

## 执行清单

- [x] 写方案文档
- [x] 新增 `src/gui/skeleton/` 基础组件
- [x] 新增 `src/gui/block/skeleton.rs` 按 block kind 渲染
- [x] 新增 `src/gui/document/skeleton_window.rs` 整窗骨架
- [x] 接入 block payload placeholder/loading 分支
- [x] 接入 document surface placeholder window
- [x] 跑 `cargo fmt && cargo test gui::block --lib && cargo test gui::document --lib && cargo check`
- [ ] 提交
