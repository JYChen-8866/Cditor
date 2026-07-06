# 超大富文本编辑器可验证任务清单

本文档把 `large-document-rich-text-architecture.md` 中的方案拆成可推进、可验证、可验收的目标、任务和子任务。

原则：

```text
1. 每个目标必须有验收标准。
2. 每个任务必须有明确产物。
3. 每个子任务必须能独立验证。
4. 先跑通闭环，再追求极限性能。
5. 当前编辑体验和滚动稳定性优先于远端全局精确。
```

---

## 总体验收目标

### G0. 10w block 文档基础体验

目标：10w block 文档可以快速打开、稳定滚动、局部编辑不卡顿。

验收标准：

```text
- 打开 10w block 文档后，300ms ideal / 800ms acceptable 内显示首屏 skeleton 或首屏内容。
- 首屏可编辑不等待全量 hydrate、全量 layout、全量 FTS。
- 连续滚动 5s，p99 main-thread work < 16ms。
- 滚动过程中 global_scroll_top 符合输入方向，不出现反向大跳。
- 当前编辑 block 连续输入 1000 字，caret viewport_y 漂移 <= 1 device px。
- 拖动 scrollbar 到任意比例时，drag session 中 thumb 不跳、不反向。
- mouseup 后目标区域加载并通过 anchor restore 收敛。
```

验证方式：

```text
- perf_10w.sqlite 测试文档。
- ScrollTraceFrame replay。
- Debug Overlay 指标。
- Scroll jitter test。
- Caret anchor stability test。
```

---

## Phase 1：文档索引与高度索引闭环

### 目标

建立大文档的轻量内核索引，让滚动、高度、selection 不依赖 UI entity。

### P1-T1. DocumentIndex

产物：`DocumentIndex` 独立模块。

子任务：

- [x] 定义 `DocumentIndex` 数据结构：`block_ids`、`parent_ids`、`depths`、`kind_tags`、`flags`、`id_to_index`。
- [x] 实现 `total_count()`。
- [x] 实现 `id_at(index)`。
- [x] 实现 `index_of(block_id)`。
- [x] 实现 `compare_position(a, b)`。
- [x] 支持从 SQLite / mock store 批量构建索引。
- [x] 支持结构版本 `structure_version`。

验收标准：

```text
- 10w block 构建 DocumentIndex 不 hydrate payload。
- index_of / id_at 互逆。
- compare_position 对跨 page block 正确。
- 构建时间 < 300ms ideal / 800ms acceptable。
```

验证：

- [x] 单元测试：顺序 block。
- [x] 单元测试：嵌套 block。
- [x] 单元测试：随机 insert/delete/move 后 index 正确。
- [x] 性能测试：10w block index 构建耗时。

---

### P1-T2. VisibleDocumentIndex

产物：`VisibleDocumentIndex`。

子任务：

- [x] 定义 `visible_block_ids`。
- [x] 定义 `source_structure_version`。
- [x] 定义 `visibility_version`。
- [x] 定义 `id_to_visible_index`。
- [x] 实现从 `DocumentIndex` 构建 visible projection。
- [x] 支持 toggle / folded subtree。
- [x] 支持 `scroll_to_block(hidden_child)` 定位到 nearest visible ancestor。
- [x] 支持批量 visibility update。

验收标准：

```text
- 折叠 1w block subtree 不逐 block notify UI。
- PageLayoutIndex / BlockHeightIndex 基于 visible index，而不是原始全文 index。
- selection 仍保存 document-level block_id + TextOffset。
```

验证：

- [x] 单元测试：toggle collapse 后 visible count 正确。
- [x] 单元测试：hidden child 定位 ancestor。
- [x] 性能测试：折叠 / 展开 1w subtree 不出现 O(n²)。

---

### P1-T3. BlockHeightIndex

产物：block-level height prefix sum。

子任务：

- [x] 定义 `BlockHeightIndex`。
- [x] 存储 `heights: Vec<f64>`。
- [x] 存储 `confidence: Vec<HeightConfidence>`。
- [x] 实现 prefix sum，第一版可用 FenwickTree 或 page chunk 聚合。
- [x] 实现 `total_height()`。
- [x] 实现 `offset_of_block(index)`。
- [x] 实现 `block_at_offset(global_y)`。
- [x] 实现 `update_height(index, new_height)`。
- [x] 支持 batch insert / delete / move。

验收标准：

```text
- 10w block height index 内存 <= 2MB 级别。
- block_at_offset / offset_of_block 互逆。
- 单次 height update O(log n) 或分块近似 O(log page_count)。
- 不依赖 UI entity。
```

验证：

- [x] 单元测试：prefix sum 正确。
- [x] Property-based：随机高度、随机 update。
- [x] 性能测试：10w block 随机查找 1w 次。

---

### P1-T4. PageLayoutIndex

产物：page-level coarse mapping。

子任务：

- [x] 定义 `PageLayout`。
- [x] 定义 `PageLayoutIndex`。
- [x] 实现 `page_count()`。
- [x] 实现 `total_height()`。
- [x] 实现 `offset_of_page(page)`。
- [x] 实现 `page_at_offset(global_y)`。
- [x] 实现 `update_page_height(page, new_height)`。
- [x] 支持 `PagePolicy`：max blocks、target height、layout cost、text bytes、complex block count。

验收标准：

```text
- PageLayoutIndex 覆盖所有 visible blocks，无重叠、无空洞。
- page_at_offset 在估算高度下稳定返回目标 page。
- 高度更新后 total_height 正确。
```

验证：

- [x] 单元测试：最后一页不足 page size。
- [x] 单元测试：随机 page height update。
- [x] Property-based：page range 覆盖完整。

---

## Phase 2：虚拟滚动闭环

### 目标

所有全局滚动由 `VirtualScrollState` 驱动，UI ListState 只表达当前 render window 的局部 offset。

### P2-T1. VirtualScrollState

产物：全局滚动真相。

子任务：

- [x] 定义 `global_scroll_top: f64`。
- [x] 定义 `viewport_height: f64`。
- [x] 定义 `model_total_height: f64`。
- [x] 定义 `displayed_total_height: f64`。
- [x] 定义 `ScrollOrigin`。
- [x] 定义 `ScrollPrecision`。
- [x] 实现 clamp。
- [x] 实现 `scroll_to_global_offset(global_y, origin)`。
- [x] 实现 `scroll_to_block(block_id)`。

验收标准：

```text
- 全局坐标不使用 f32。
- 不依赖 GPUI ListState 作为全文滚动真相。
- ScrollOrigin guard 阻止 local ListState 反向驱动 global scroll。
```

验证：

- [x] 单元测试：顶部 / 底部 clamp。
- [x] 单元测试：wheel delta 正负方向。
- [x] 单元测试：origin guard。

---

### P2-T2. Global offset 映射

产物：global offset -> page/block/local offset 映射。

子任务：

- [x] 使用 BlockHeightIndex 实现 `target_for_global_offset(global_y)`。
- [x] 返回 `block_index`、`offset_in_block`、`global_scroll_top`。
- [x] 使用 PageLayoutIndex 决定目标 page。
- [x] 支持目标 page 未加载时返回 placeholder target。
- [x] 实现 window local coordinate 转换。

验收标准：

```text
- global_y 可以稳定映射到 visible block。
- 坐标提交给 UI 前转为 window local / viewport local。
- 千万级 total_height 下无 f32 精度丢失。
```

验证：

- [x] 单元测试：global_y -> block -> offset。
- [x] 单元测试：window_start_global_y 重定位。
- [x] Scroll trace：滚动中无大坐标传入 UI。

---

### P2-T3. Wheel / Trackpad 接管

产物：统一滚轮输入管线。

子任务：

- [x] 定义 `ScrollInput`。
- [x] 支持 Pixel / Line / Page delta mode。
- [x] 支持 Began / Changed / Momentum / Ended phase。
- [x] 同一 frame 合并多个 delta。
- [x] WheelActive / Momentum 状态降低远端 height correction 优先级。

验收标准：

```text
- 连续滚动中 global_scroll_top 符合输入方向。
- trackpad momentum 不误触发过度 remeasure。
- wheel event 同帧不 hydrate 大量 block。
```

验证：

- [x] Scroll jitter test。
- [x] Momentum scroll trace replay。
- [x] p99 wheel frame < 16ms。

---

### P2-T4. 自绘 / 全局 Scrollbar

产物：由 VirtualScrollState 驱动的全局 scrollbar。

子任务：

- [x] 隐藏或禁用 local ListState scrollbar。
- [x] 实现 `visual_thumb_height`。
- [x] 实现 `visual_thumb_top`。
- [x] 实现 min thumb size。
- [x] 实现 drag session。
- [x] mousedown freeze displayed_total_height。
- [x] mousemove 使用 frozen total 映射。
- [x] mouseup 后 anchor restore。

验收标准：

```text
- drag session 中 thumb 不反向、不跳变。
- displayed_total_height 与 model_total_height 可分离。
- mouseup 后再收敛 total_height。
```

验证：

- [x] Scrollbar drag precision test。
- [x] Trace 检查 thumb reverse-jump count == 0。
- [x] 高度修正期间拖动 scrollbar 不抖。

---

## Phase 3：RenderWindow 与窗口提交协议

### 目标

只渲染当前窗口，窗口切换稳定，不闪烁、不丢焦点、不丢 selection。

### P3-T1. RenderWindow

产物：当前渲染窗口模型。

子任务：

- [x] 定义 `RenderWindow`。
- [x] 包含 `page_range`。
- [x] 包含 `block_range`。
- [x] 包含 `entities`。
- [x] 包含 `local_height_index`。
- [x] 支持 placeholder window。

验收标准：

```text
- UI entity 数量控制在 window + pins + LRU 范围内。
- 目标 page 未加载时能显示稳定高度 placeholder。
- placeholder 替换为真实 window 后通过 anchor restore 保持视觉位置。
```

验证：

- [x] 跳转远端 page 时先显示 placeholder。
- [x] Placeholder height 等于 PageLayoutIndex 当前 page height。
- [x] 替换后 anchor jitter <= 1 device px。

---

### P3-T2. WindowPlanner

产物：稳定窗口规划器。

子任务：

- [x] 实现按当前 page 规划 before / after page。
- [x] 根据滚动速度调整 prefetch 方向。
- [x] 支持 hysteresis。
- [x] 支持 min stable frames before trim。
- [x] 支持 min ms between window commits。
- [x] focus / composition / selection endpoint 所在 page 永不因 planner 抖动 trim。

验收标准：

```text
- page 边界附近慢速滚动不反复 A/B window commit。
- 快速向下滚动优先预取下方 page。
- 快速向上滚动优先预取上方 page。
```

验证：

- [x] Window boundary hysteresis test。
- [x] Trace 检查 window commit count / frame。
- [x] Debug Overlay 显示当前 window page range。

---

### P3-T3. 两阶段 Window Commit

产物：atomic swap 协议。

子任务：

- [x] 定义 `WindowLoadState`。
- [x] CurrentStable 状态保持当前 window。
- [x] PreparingNext 后台准备 payload / entity / layout。
- [x] PlaceholderShown 显示稳定 placeholder。
- [x] ReadyToSwap 后 atomic swap。
- [x] swap 后只做一次 anchor restore。

验收标准：

```text
- 不出现半加载 window。
- 不出现空白闪烁。
- 不丢失 focus / composition / selection endpoint。
```

验证：

- [x] 快速滚动远端 page。
- [x] 滚动中 page load 延迟 / 失败模拟。
- [x] Swap frame trace。

---

## Phase 4：高度修正与不抖动协议

### 目标

高度变化不会导致当前 viewport、caret、scrollbar 抖动。

### P4-T1. ScrollAnchor 与 AnchorKind

产物：统一锚点系统。

子任务：

- [x] 定义 `ScrollAnchor`。
- [x] 定义 `AnchorKind`。
- [x] 支持 ViewportTop。
- [x] 支持 ExplicitScrollTarget。
- [x] 支持 SelectionFocus。
- [x] 支持 Caret。
- [x] 支持 Composition。
- [x] 实现 anchor 优先级。
- [x] 同一 frame 只允许一个 primary anchor。

验收标准：

```text
- Composition > Caret > SelectionFocus > ExplicitScrollTarget > ViewportTop。
- 低优先级 anchor 不能覆盖高优先级 anchor。
- 同一 frame anchor restore <= 1 次。
```

验证：

- [x] 输入导致换行时 caret 不跳。
- [x] IME candidate rect 不跳。
- [x] Trace 显示 primary anchor kind。

---

### P4-T2. HeightChange Queue

产物：帧内合并高度修正。

子任务：

- [x] 定义 `HeightChange`。
- [x] 收集 frame 内 height changes。
- [x] 合并同 block 多次 height change。
- [x] frame end 统一 apply。
- [x] 更新 BlockHeightIndex。
- [x] 更新 LoadedPageLayout。
- [x] 更新 PageLayoutIndex。
- [x] 更新 model_total_height。
- [x] displayed_total_height 分帧或 idle 收敛。

验收标准：

```text
- 不出现 scroll -> layout -> correction -> scroll 的同帧循环。
- 同一 frame 最多一次 anchor restore。
- viewport 下方高度变化不修改 scroll_top。
```

验证：

- [x] Height correction chaos test。
- [x] Trace 检查 correction count / frame。
- [x] Anchor jitter p95 <= 1 device px。

---

### P4-T3. HeightErrorBudget

产物：高度误差预算。

子任务：

- [x] 定义 `viewport_max_error_px`。
- [x] 定义 `page_max_error_px`。
- [x] 定义 `total_height_max_error_ratio`。
- [x] 定义 `correction_apply_threshold_px`。
- [x] 定义 `displayed_total_converge_px_per_frame`。
- [x] 小于阈值的 correction coalesce。
- [x] 防止 rounded / unrounded height 往返写入。

验收标准：

```text
- 小于 1 device px 的误差不反复触发 anchor restore。
- displayed_total_height 不因远端 refinement 突变。
- 慢速滚动无 1px 往返抖动。
```

验证：

- [x] 模拟 0.2px / 0.5px / 0.9px correction。
- [x] Offscreen measure 与 onscreen 差 1~4px 的 chaos test。
- [x] Debug Overlay 显示 last correction delta。

---

## Phase 5：输入 Hot Path 与编辑事务

### 目标

当前编辑 block 达到 ms 级输入响应，不被后台任务、持久化、FTS、远端 layout 抢占。

### P5-T1. EditingSession

产物：当前编辑会话状态。

子任务：

- [x] 定义 `EditingSession`。
- [x] 保存 `block_id`。
- [x] 保存 `content_version`。
- [x] 保存 `caret_anchor`。
- [x] 保存 `composition`。
- [x] 保存 layout cache pin。
- [x] 编辑 block 自动 pin。

验收标准：

```text
- 当前编辑 block 不因 window trim 被卸载。
- 当前编辑 block layout task 永远高优先级。
- 当前编辑 block 的 caret geometry 与 text layout 同版本。
```

验证：

- [x] 边输入边滚动远离当前 block。
- [x] 当前 block 不 evict。
- [x] 输入延迟 p95 < 8ms。

---

### P5-T2. 单字符输入 Hot Path

产物：低延迟输入路径。

子任务：

- [x] keydown 后先更新内存文本模型。
- [x] 更新 InlineRun / Rope / PieceTable。
- [x] 标记 LayoutDirtyRange。
- [x] 只 layout 当前 block / 当前 visual line 附近。
- [x] 更新 caret geometry。
- [x] 产生 EditTransaction。
- [x] 异步持久化。
- [x] 异步 FTS。
- [x] 异步 syntax highlight。

禁止项：

```text
- 同步 SQLite 写。
- 同步 FTS 更新。
- 同步全 block shaping。
- 同步 page reflow。
- 等待 async result。
```

验收标准：

```text
- 连续输入 1000 字，caret drift <= 1 device px。
- p95 input latency < 8ms。
- p99 input latency < 16ms。
```

验证：

- [x] Typing trace replay。
- [x] Caret anchor stability test。
- [x] Debug Overlay 显示 edit transaction time。

---

### P5-T3. EditTransaction 与 Undo Grouping

产物：可撤销的编辑事务系统。

子任务：

- [x] 定义 `EditOperation`。
- [x] 定义 `EditTransaction`。
- [x] 支持 inverse ops。
- [x] 支持 before / after selection。
- [x] 支持 before / after anchor。
- [x] 支持 `InsertBlocks`。
- [x] 支持 `DeleteBlockRange`。
- [x] 支持 `MoveBlockRange`。
- [x] 定义 `UndoGroupBoundary`。
- [x] 连续输入按时间和 selection 合并。
- [x] IME commit 独立 undo step。
- [x] paste / drag / format 独立 undo step。

验收标准：

```text
- height correction / syntax highlight / FTS / cache write 不进入 undo。
- 大 paste / delete 不把完整 payload 无限放内存。
- undo 后只做一次 selection restore 和 anchor restore。
```

验证：

- [x] 连续输入 undo 一次按预期回退。
- [x] IME commit undo。
- [x] paste 1w blocks undo。
- [x] delete 5w blocks undo 不 OOM。

---

## Phase 6：Selection、IME、Hit Test 与国际文本

### 目标

Selection 不依赖 UI，IME、Bidi、emoji、CJK、组合字符下编辑正确稳定。

### P6-T1. TextOffset 体系

产物：明确 offset 语义和转换层。

子任务：

- [x] 定义 `InternalTextOffset`。
- [x] 定义 `PlatformUtf16Offset`。
- [x] 定义 `GraphemeIndex`。
- [x] 实现 `TextOffsetMap`。
- [x] internal -> UTF-16。
- [x] UTF-16 -> internal。
- [x] internal -> grapheme boundary 校验。
- [x] 建立 bidi runs。

验收标准：

```text
- EditOperation range 必须落在 grapheme boundary。
- IME marked range 从 UTF-16 正确转换。
- Backspace/Delete 以 grapheme cluster 为单位。
```

验证：

- [x] emoji ZWJ。
- [x] combining mark。
- [x] CJK。
- [x] RTL/LTR 混排。
- [x] IME marked range。

---

### P6-T2. DocumentSelection

产物：文档级 selection。

子任务：

- [x] 定义 `TextPosition { block_id, offset, affinity }`。
- [x] 定义 `DocumentSelection`。
- [x] 实现 normalize。
- [x] 实现 visible selection fragments。
- [x] 支持跨 page selection。
- [x] 支持 hidden block selection 降级策略。
- [x] 支持 accessibility projection。

验收标准：

```text
- selection 滚出 window 不丢失。
- 跨 page copy 不 hydrate 中间 UI pages。
- selection fragment 只渲染当前 visible window。
```

验证：

- [x] Cross-page selection test。
- [x] Reversed anchor/focus。
- [x] Hidden subtree selection。

---

### P6-T3. VisualLineLayout 与 Hit Test

产物：视觉行级 hit test。

子任务：

- [x] 定义 `VisualLineLayout`。
- [x] 定义 `VisualRun`。
- [x] mouse x/y -> TextPosition。
- [x] TextPosition -> caret rect。
- [x] 支持 bidi visual movement。
- [x] 支持 soft wrap affinity。
- [x] 支持 double click word selection。
- [x] 支持 IME candidate rect。

验收标准：

```text
- Bidi 文本中左右方向键行为正确。
- 行首 / 行尾 / soft wrap 边界 caret 不跳。
- IME candidate rect 不使用旧 layout。
```

验证：

- [x] Bidi hit test。
- [x] Soft wrap boundary。
- [x] IME candidate rect stability。

---

### P6-T4. IME / Composition

产物：稳定 composition 管线。

子任务：

- [x] 定义 `CompositionState`。
- [x] composition block pin。
- [x] UTF-16 marked range -> internal range。
- [x] preview text layout。
- [x] Composition anchor。
- [x] commit 生成 edit transaction。
- [x] cancel 恢复 selection。
- [x] commit 作为 undo boundary。

验收标准：

```text
- composition 期间 block 不 evict。
- 输入导致换行时候选框不跳。
- cancel 后文本和 selection 恢复。
```

验证：

- [x] 中文 IME。
- [x] 日文 IME。
- [x] emoji 输入。
- [x] Composition + scroll。

---

## Phase 7：异步调度、预算和并发控制

### 目标

后台任务不能抢占输入和滚动帧，异步结果返回不会造成卡顿或旧结果覆盖新状态。

### P7-T1. MainThreadBudget

产物：主线程预算仲裁器。

子任务：

- [x] 定义 `MainThreadBudget`。
- [x] 定义 `MainThreadWorkKind`。
- [x] 实现优先级队列。
- [x] Typing / Composing 预留 input budget。
- [x] WheelScrolling 限制 window diff / measure / correction。
- [x] Async result 必须入队，不直接 apply。

验收标准：

```text
- 输入期间远端 refinement 不抢占当前帧。
- 滚动期间 async result 返回风暴不造成 dropped frames。
- 超预算任务 defer / coalesce / drop stale。
```

验证：

- [x] 模拟 1000 个 async layout result 同时返回。
- [x] Typing + background FTS。
- [x] Wheel + image decode result storm。

---

### P7-T2. LayoutScheduler

产物：layout 任务调度器。

子任务：

- [x] 定义 `InteractionMode`。
- [x] high priority queue：editing block / current viewport。
- [x] normal priority queue：overscan。
- [x] idle priority queue：远端 refinement。
- [x] max entity create / frame。
- [x] max measure apply / frame。
- [x] max height corrections / frame。
- [x] backpressure。

验收标准：

```text
- 当前编辑 block layout 永远优先。
- 滚动中不一次 measure 1000 blocks。
- idle 才做远端 convergence。
```

验证：

- [x] Scheduler priority test。
- [x] Frame budget exhaustion test。
- [x] Debug Overlay 显示 pending layout tasks。

---

### P7-T3. Async Version Control

产物：异步任务版本校验。

子任务：

- [x] 定义 `LayoutTaskRequest`。
- [x] 携带 generation。
- [x] 携带 block_id。
- [x] 携带 content_version。
- [x] 携带 layout_version。
- [x] 携带 width_bucket / exact_width。
- [x] 返回时版本不匹配 discard。
- [x] 旧结果只能作为 historical hint，不能覆盖 exact。

验收标准：

```text
- 旧 width/font/theme measure 不覆盖新高度。
- 旧 content_version shaping 不覆盖当前编辑 block。
- 快速拖 scrollbar 时旧 page request 被丢弃。
```

验证：

- [x] generation discard test。
- [x] width_bucket mismatch。
- [x] content_version mismatch。

---

### P7-T4. WorkerPoolPolicy

产物：避免后台任务优先级反转。

子任务：

- [x] interactive lanes。
- [x] background lanes。
- [x] max background queue。
- [x] background queue 满时丢弃旧 generation。
- [x] image decode / FTS / remote refinement 走 background。
- [x] editing block / current viewport 走 interactive。

验收标准：

```text
- 远端 shaping 不占满 worker pool。
- 当前编辑 block layout task 不排在大量 background task 后。
```

验证：

- [x] Worker pool saturation test。
- [x] Typing while background indexing。

---

## Phase 8：持久化、缓存和恢复

### 目标

大文档冷启动快，layout cache 可复用但可丢弃；持久化失败可恢复，不阻塞输入。

### P8-T1. Layout Cache 表

产物：`block_layout` / `page_layout`。

子任务：

- [x] 定义 `block_layout` 表。
- [x] 定义 `page_layout` 表。
- [x] 保存 measured_height。
- [x] 保存 estimated_height。
- [x] 保存 confidence。
- [x] 保存 width_bucket / exact_width。
- [x] 保存 layout_version。
- [x] 保存 structure_version。

验收标准：

```text
- 打开大文档可加载 historical height，不重新测量全文。
- layout_version 不匹配时降级，不作为 exact。
- structure_version 不匹配时 page_layout 降级为 hint。
```

验证：

- [x] 冷启动加载 layout cache。
- [x] font_version 改变。
- [x] width_bucket 改变。
- [x] structure_version 改变。

---

### P8-T2. Height Write Debounce

产物：高度写入策略。

子任务：

- [x] HeightMeasured 先写 memory cache。
- [x] 500ms debounce batch write SQLite。
- [x] close 前 flush。
- [x] 写失败进入 dirty queue。
- [x] 不阻塞 UI 线程。

验收标准：

```text
- 连续输入不会每字符同步写 SQLite。
- 高度测量风暴不会产生大量小事务。
- 关闭文档前可以 flush 或提示。
```

验证：

- [x] Typing + layout height updates。
- [x] Batch write count。
- [x] SQLite write failure simulation。

---

### P8-T3. Cache Recovery Policy

产物：缓存恢复策略。

子任务：

- [x] schema_version 检查。
- [x] cache_version 检查。
- [x] index snapshot 损坏时重建。
- [x] FTS 缺失时后台 rebuild。
- [x] thumbnail 缺失时按需生成。
- [x] layout cache 不阻塞首屏。

验收标准：

```text
- layout cache 损坏不影响正文打开。
- FTS 损坏不影响编辑器打开。
- 启动时不做全量 text shaping / page measure。
```

验证：

- [x] 删除 block_layout 后打开文档。
- [x] 删除 page_layout 后打开文档。
- [x] 损坏 FTS 后打开文档。
- [x] 损坏 index snapshot 后重建。

---

### P8-T4. Optimistic Persistence

产物：乐观持久化和失败恢复。

子任务：

- [x] 定义 `BlockPersistenceState`。
- [x] 保存 persisted_version。
- [x] 保存 memory_version。
- [x] 保存 saving_version。
- [x] 保存 SaveFailed 状态。
- [x] persisted_version == memory_version 才 Clean。
- [x] SaveFailed block pin 或进入恢复队列。

验收标准：

```text
- 保存 version 5 成功不能把已编辑到 version 6 的 block 标 Clean。
- SaveFailed 不丢编辑内容。
- 关闭文档前提示未保存内容。
```

验证：

- [x] Save v5 while edit v6。
- [x] SQLite write fail。
- [x] Close with dirty blocks。

---

## Phase 9：复杂 Block 与媒体资源

### 目标

单个复杂 block 不拖垮整个页面；媒体资源不会造成滚动卡顿或高度突变。

### P9-T1. BlockLayoutProvider

产物：统一复杂 block 高度接口。

子任务：

- [x] 定义 `estimate_height(width)`。
- [x] 定义 `intrinsic_size()`。
- [x] 定义 `layout_cost()`。
- [x] 定义 `can_measure_offscreen()`。
- [x] 段落估高。
- [x] CodeBlock 估高。
- [x] Table 估高。
- [x] Image aspect ratio 估高。
- [x] Embed / Whiteboard stable box。

验收标准：

```text
- 异步资源 block 插入时不能 height = 0。
- offscreen measure 不一致时只能作为 estimate / historical。
- layout_cost 参与 PagePolicy。
```

验证：

- [x] Image metadata missing。
- [x] Embed stable box。
- [x] Offscreen vs onscreen mismatch。

---

### P9-T2. BlockEditorModel

产物：复杂 block 内部编辑协议。

子任务：

- [x] 定义 `BlockEditorModel`。
- [x] 支持 `apply_inner_op`。
- [x] 支持 `visible_fragments`。
- [x] 支持 block 内 hit_test。
- [x] 支持 block 内 selection。
- [x] 支持 block 内 anchor。

验收标准：

```text
- 10MB code block 支持行级虚拟化。
- 5w 行 table 支持行 / 列虚拟化。
- table 插入 1000 行不触发 1000 次外层 height correction。
```

验证：

- [x] 10MB code block scroll。
- [x] 5w rows table scroll。
- [x] Table batch insert。

---

### P9-T3. 内外滚动协议

产物：复杂 block wheel handling。

子任务：

- [x] 定义 `WheelHandling`。
- [x] block 内部可滚时先消费 wheel。
- [x] block 内到边界后剩余 delta 转交 document。
- [x] whiteboard / iframe 提供退出或边界转移策略。
- [x] selection drag / IME 时不意外吞掉外层 auto-scroll。

验收标准：

```text
- 滚到大型 table / code block 时滚轮不突然失效。
- 内部滚动到边界后文档继续滚。
- 外层 scroll anchor 不被内部 scroll 覆盖。
```

验证：

- [x] Code block internal scroll。
- [x] Table internal scroll。
- [x] Embed wheel capture。

---

### P9-T4. MediaCache

产物：独立媒体资源缓存。

子任务：

- [x] 定义 `MediaCachePolicy`。
- [x] max decoded bytes。
- [x] max thumbnail bytes。
- [x] viewport distance priority。
- [x] metadata 优先加载。
- [x] viewport 附近 decode thumbnail。
- [x] 原图延迟到用户明确查看。
- [x] memory pressure 下释放 decoded resource。

验收标准：

```text
- 滚动到图片密集区不在主线程 decode 原图。
- entity pin 不等于原图资源永久 pin。
- 内存压力下不丢 payload / stable box。
```

验证：

- [x] 图片密集文档滚动。
- [x] 高分辨率图片内存压力。
- [x] Decode result storm。

---

## Phase 10：安全、Paste / Import 与全局查询

### 目标

大 paste 不阻塞 UI；外部内容安全；全文查询不依赖 UI。

### P10-T1. Paste / Import 流式管线

产物：大 paste 可取消、可进度显示。

子任务：

- [x] Clipboard/Input 接入。
- [x] parse / sanitize。
- [x] normalize blocks。
- [x] allocate block ids。
- [x] estimate layout height。
- [x] batch insert DocumentIndex。
- [x] visible blocks hydrate first。
- [x] async persist remaining payload。
- [x] async load image / embed metadata。
- [x] 支持取消。
- [x] 支持进度。

验收标准：

```text
- paste 1w blocks UI 不阻塞。
- 首屏可交互优先于全部 payload 落盘。
- 大 paste 可取消。
```

验证：

- [x] Paste 1w markdown blocks。
- [x] Paste HTML with images。
- [x] Paste cancel。

---

### P10-T2. 外部内容安全策略

产物：sanitize 和远程资源策略。

子任务：

- [x] 禁止 script。
- [x] 禁止 on* event handler。
- [x] 过滤危险 URL。
- [x] SVG 策略。
- [x] remote image 策略。
- [x] iframe / embed 策略。
- [x] file:// 策略。
- [x] data URL 策略。
- [x] 隐私模式配置。

验收标准：

```text
- 外部 HTML 不能执行脚本。
- 远程资源不会在未确认策略下泄露隐私。
- remote image 不导致滚动中网络和 decode 风暴。
```

验证：

- [x] XSS payload paste。
- [x] SVG paste。
- [x] Remote image paste。
- [x] file:// paste。

---

### P10-T3. DocumentQueryIndex

产物：全局查询不依赖 UI。

子任务：

- [x] SQLite FTS5 block_fts。
- [x] plain_text 提取。
- [x] 后台增量更新。
- [x] 查询返回 block_id。
- [x] scroll_to_block(block_id)。
- [x] 目标 page 未加载时 estimate 定位。
- [x] 加载后 anchor restore。

验收标准：

```text
- 搜索不只搜当前窗口。
- 搜索结果跳转不 hydrate 中间 pages。
- FTS 更新不阻塞输入。
```

验证：

- [x] 10w block 搜索。
- [x] 搜索结果跳转远端 page。
- [x] Typing while FTS update。

---

## Phase 11：观测、Trace Replay 和回归门禁

### 目标

所有“不抖”“不卡”“连续”都能被 trace 和指标验证，而不是只靠肉眼。

### P11-T1. Debug Overlay

产物：调试浮层。

子任务：

- [x] 显示 global_scroll_top。
- [x] 显示 model_total_height。
- [x] 显示 displayed_total_height。
- [x] 显示 ScrollPrecision。
- [x] 显示 current page / window range。
- [x] 显示 loaded / placeholder pages。
- [x] 显示 anchor kind / anchor block。
- [x] 显示 entity count / pinned count。
- [x] 显示 shape count / layout time。
- [x] 显示 SQLite query count。
- [x] 显示 height correction count。
- [x] 显示 scroll_jitter_px / caret_jitter_px。
- [x] 可视化 page 边界。
- [x] 可视化 estimated / historical / exact height 区域。

验收标准：

```text
- 出现滚动抖动时，可以从 overlay 看出 anchor、height correction、window commit 状态。
- 可以区分 model_total_height 和 displayed_total_height。
```

验证：

- [x] 手动滚动 debug overlay。
- [x] Height chaos 时 overlay 显示 correction。

---

### P11-T2. Trace Event Log

产物：结构化性能事件日志。

子任务：

- [x] PageHeightCorrected。
- [x] AnchorRestored。
- [x] WindowChanged。
- [x] EntityEvicted。
- [x] PinAdded / PinRemoved。
- [x] OldRequestDiscarded。
- [x] ScrollbarDragFrozenTotalHeight。
- [x] LayoutTaskDeferred。
- [x] AsyncResultDiscarded。

验收标准：

```text
- 每个 jitter frame 能追溯原因。
- 旧异步结果是否 discard 可观察。
- window commit 次数可统计。
```

验证：

- [x] Event log snapshot。
- [x] Old request discard simulation。

---

### P11-T3. ScrollTraceFrame Replay

产物：可离线重放的 trace。

子任务：

- [x] 记录 input。
- [x] 记录 global_scroll_top before / after。
- [x] 记录 anchor。
- [x] 记录 window range。
- [x] 记录 height changes。
- [x] 记录 correction applied。
- [x] 记录 model_total_height。
- [x] 记录 displayed_total_height。
- [x] 记录 frame_cost_ms。
- [x] 离线 replay。
- [x] 回归门禁。

验收标准：

```text
- thumb reverse-jump count == 0。
- anchor jitter p95 / p99 不超过阈值。
- caret jitter p95 / p99 不超过阈值。
- height correction per frame 不超过预算。
- window commit count 不爆炸。
```

验证：

- [x] Wheel trace replay。
- [x] Scrollbar drag replay。
- [x] Typing trace replay。
- [x] Height chaos replay。

---

## Phase 12：最终集成验收

### 目标

把前面各阶段串成完整可用的大文档富文本体验。

### P12-T1. 10w block 打开验收

子任务：

- [x] 准备 10w 个 1 行 block 文档。
- [x] 准备 10w 个高度极不均匀 block 文档。
- [x] 准备图片密集文档。
- [x] 准备单个 10MB code block。
- [x] 准备单个 5w 行 table。
- [x] 准备 emoji / CJK / bidi 文档。

验收标准：

```text
- 首屏 300ms ideal / 800ms acceptable。
- 首屏不 hydrate 全文。
- shape_count 不持续无界增长。
```

---

### P12-T2. 滚动验收

子任务：

- [x] 从顶部滚到中部。
- [x] 从中部滚回顶部。
- [x] 连续滚动 10 分钟。
- [x] 滚动中随机 height correction。
- [x] 滚动中 window load 延迟。

验收标准：

```text
- p99 frame < 16ms。
- anchor jitter p95 <= 1 device px。
- 无大于 50px 的意外视觉位移。
- local ListState 不反向驱动 global scroll。
```

---

### P12-T3. 编辑验收

子任务：

- [x] 当前 block 连续输入 1000 字。
- [x] 输入导致多次换行。
- [x] IME composition。
- [x] 边输入边滚动。
- [x] 边输入边 resize。

验收标准：

```text
- input latency p95 < 8ms。
- input latency p99 < 16ms。
- caret drift <= 1 device px。
- IME candidate 不跳。
- 当前编辑 block 不 evict。
```

---

### P12-T4. 结构编辑验收

子任务：

- [x] paste 1w blocks。
- [x] delete 5w blocks。
- [x] undo large delete。
- [x] move 1w subtree。
- [x] collapse / expand 1w subtree。

验收标准：

```text
- 批量操作不 O(n²)。
- UI 不长时间阻塞。
- 操作后只做一次 anchor restore。
- undo 不 OOM。
```

---

## 任务推进顺序建议

推荐顺序：

```text
1. P1 DocumentIndex / VisibleDocumentIndex / HeightIndex。
2. P2 VirtualScrollState / global offset mapping。
3. P3 RenderWindow / WindowPlanner / placeholder。
4. P4 Anchor / height correction / error budget。
5. P11 Debug Overlay / Trace Replay，尽早做。
6. P5 Editing hot path。
7. P6 Selection / IME / TextOffset。
8. P7 Scheduler / async budget。
9. P8 Cache / persistence recovery。
10. P9 Complex blocks / media cache。
11. P10 Paste / security / query。
12. P12 Full acceptance。
```

关键建议：

```text
Trace Replay 和 Debug Overlay 不要等最后做。
它们应该在 Phase 2~4 期间就落地，否则后续很难定位 1px 抖动和滚动条回跳。
```

---

## Definition of Done

一个任务完成必须满足：

```text
- 有代码产物或明确文档产物。
- 有单元测试或集成测试。
- 有性能 / trace / overlay 指标，适用于体验类任务。
- 不破坏已有 phase 的验收。
- 不引入 UI entity 作为全局真相。
- 不在输入 hot path 引入同步重活。
```

一个 Phase 完成必须满足：

```text
- 本 Phase 所有关键任务完成。
- 对应验收场景通过。
- Trace 中无新增严重 jitter / reverse jump。
- Debug Overlay 可以解释当前行为。
- 文档更新当前限制和已知风险。
```
