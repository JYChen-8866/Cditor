# Windows 编辑器输入闪烁与骨架屏问题分析

## 现象

- Windows 桌面端在编辑器中按键时，界面短暂闪烁。
- 闪烁后，除了鼠标点击的当前 block，其余 block 显示为骨架屏。
- macOS 普通输入基本正常；但在软换行 block 中按 Enter 仍会闪烁。
- 因此问题同时覆盖两条路径：
  - Windows 上的普通文本输入触发了过于频繁的布局/窗口重新规划。
  - 软换行 Enter 在任何桌面平台都会改变 block 高度，触发同一套窗口切换路径。

## 已确认的渲染链路

一次输入后的 GPUI 帧会依次经过：

1. `CditorV2View::render` 调用 `EditorSessionHandle::render_frame`。
2. `render_frame` 先刷新待处理的 block 高度修正，再调用 `DocumentRuntime::projection`。
3. `projection_for_window_planned` 根据新的滚动位置、block 高度和 payload residency 计算目标窗口。
4. `ProjectionState::reconcile` 决定展示已提交窗口、准备中的旧窗口，还是 placeholder window。
5. `DocumentSurface` 根据 `placeholder_window_height` 渲染整段 skeleton；加载窗口内 payload 缺失时，`BlockView` 再渲染单个 block skeleton。
6. 当前 block 会被 editing session / selection pin 保护，所以出现“点击的 block 仍然正常，其他 block 变成骨架”的视觉结果。

关键代码位置：

- `crates/cditor-editor-gpui/src/editor_view/render.rs`
- `crates/cditor-editor-gpui/src/editor_view/render/payload_scheduling.rs`
- `crates/cditor-runtime/src/document_runtime/projection.rs`
- `crates/cditor-runtime/src/document_runtime/layout_state.rs`
- `crates/cditor-editor-gpui/src/document/document_surface.rs`
- `crates/cditor-editor-gpui/src/block/block_content.rs`

## 根因判断

### 1. 输入帧把几何收敛和 payload 窗口切换放在同一条同步路径

文本输入不仅更新当前 block 的内容和 caret，还会让当前 block 的平台文本测量结果重新进入 `pending_measured_heights`。下一帧 `render_frame` 会同步刷新这些高度修正，然后立刻用新的高度索引规划可见 block 窗口。

普通字符在软换行附近也可能改变行数。Windows 的文本 shaping / 字体度量与 macOS 不同，宽度和行高更容易在输入后产生新的测量结果，因此 Windows 更容易在每个按键后重新规划窗口；软换行 Enter 则在 macOS 也必然改变 block 高度。

### 2. 目标窗口变化时，当前渲染协议允许回退到 placeholder

`projection_for_window_planned` 会把新的可见范围作为 `desired`。当目标范围尚未 resident，`ProjectionState::reconcile` 进入 `PreparingNext` 或 cold placeholder 路径；payload 调度器随后发起新的窗口读取。

只要稳定发布快照因为结构/窗口身份检查失效，或者当前会话还没有可复用的 stable snapshot，`DocumentSurface` 就会收到 `placeholder_window_height`，从而绘制整段骨架屏。若旧窗口仍被复用但只剩当前编辑 block resident，则 `BlockPayloadView::Placeholder` 会让其它 block 单独显示骨架。

### 3. cache eviction 放大了问题，但不是唯一触发源

持久化文档会在输入、payload 读取和保存回调后安排延迟 cache maintenance。维护逻辑会保护 editing session 和当前窗口，但在窗口重新规划期间，尚未被新窗口确认 resident 的 block 可能被当作可回收对象。Windows 上更频繁的测量/窗口变化会使这个竞态更容易出现，最终表现为“当前 block 被 pin，邻近 block 暂时没有 payload”。

### 4. 这不是单纯的 GPUI 重绘闪烁

GPUI 的 `cx.notify()` 会触发重绘，但正常重绘不会把已加载内容变成 skeleton。真正造成视觉闪烁的是：输入帧中内容测量、窗口身份变化、payload readiness 和 skeleton fallback 同时发生，导致一次不完整的窗口投影被展示出来。

## 解决方案

### 阶段 A：先保证输入期间的视觉连续性

1. 为编辑中的 block 建立“输入期间稳定投影”策略：普通字符、IME composition、软换行 Enter 在 payload 新窗口准备完成前，继续展示上一份已提交的 block projection。
2. 只有当前可见 payload 完整准备好后，才原子切换到新窗口；禁止输入帧直接进入 full placeholder。
3. 对部分 resident 的窗口，复用上一帧的 payload，而不是把缺失 block 立即降级为 skeleton。

### 阶段 B：拆开几何测量与窗口读取

1. 输入帧只提交当前 block 的文本和 layout correction，不在同一帧发布新的 payload window。
2. 将由高度变化产生的窗口规划标记为 `pending target`，通过下一帧/idle frame 读取并提交。
3. 输入期间禁止由单个字符造成的 prefetch eviction；至少保留上一份 stable render window、当前 visible range 和 editing pins。

### 阶段 C：修正跨平台文本测量抖动

1. 检查 Windows 与 macOS 的 body font、scale factor、wrap width 和 line-height 是否使用同一套归一化规则。
2. 对平台测量结果使用稳定的像素量化和小幅变化阈值，避免 1px 以内的 DirectWrite/CoreText 差异重新规划窗口。
3. 对软换行 block 在 Enter 后只更新受影响 block 及其后续高度索引，避免整页重新进入 payload readiness 流程。

### 阶段 D：增加回归测试

需要新增以下测试：

- 普通文本输入后，上一份 stable projection 不得降级为 skeleton。
- 输入期间目标窗口变化时，旧窗口中的已加载 block 不得变成 placeholder。
- Windows 风格的每次输入高度变化序列不会触发 full placeholder。
- 软换行 block 按 Enter 后，当前 block 高度更新，但其它 block 保持已提交内容。
- cache maintenance 在 editing session 和 stable window 切换期间不得回收当前可视 block。

## 实施顺序

1. 先在 runtime projection 层增加稳定窗口回退测试，复现“只剩当前 block resident”的状态。
2. 再调整 GPUI render 层：输入期间保留 stable projection，延迟 payload window 切换。
3. 调整 cache maintenance 的保护范围，覆盖 stable projection 与 pending target 的并集。
4. 最后处理 Windows 文本测量量化和软换行高度增量更新。
5. 运行 Cditor runtime/editor-gpui 全量测试，再在 Windows 和 macOS 分别验证普通输入、中文 IME、软换行 Enter 和跨 block 滚动。

## 验收标准

- Windows 连续输入 50 个字符，页面不出现整页或邻近 block 骨架屏。
- Windows 中文输入法组合态期间，未编辑 block 不闪烁、不变成 skeleton。
- macOS 软换行 block 按 Enter，只发生局部高度变化，不出现页面闪烁。
- 只有真正冷启动或目标 payload 读取失败时，才允许显示 skeleton/error placeholder。
