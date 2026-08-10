# 编辑器输入闪烁与骨架屏：根因定位与解决方案（代码级证据）

> 本文基于对键盘输入、Enter 拆分、渲染帧三条完整链路的逐行追踪，
> 修正并取代 `windows-editor-skeleton-flash.md` 中的初步分析。
> 所有结论均有 file:line 证据，未经验证的部分会明确标注。

## 现象回顾

1. **Windows**：编辑器内按任意键，页面闪一下；随后除了鼠标点击（编辑中）的
   block，其余 block 全部变成骨架屏。
2. **macOS**：普通输入正常；但在软换行（一段文字折成多视觉行）的 block 里按
   Enter 会闪一下。

## 两种骨架屏的准确区分（先澄清一个误判）

渲染层有两套互不相同的骨架机制，旧文档把它们混在一起了：

| 机制 | 触发条件 | 视觉表现 |
|---|---|---|
| **整窗骨架** | `projection.placeholder_window_height = Some(_)`，此时 `projection.blocks` 为空（`projection/placeholder.rs:70,73`） | 整页骨架条，**连焦点 block 也不渲染**，没有任何真实内容 |
| **单 block 骨架** | 已加载窗口帧中，某 block 的 payload 不在 `payload_window.payloads` 里 → `BlockPayloadView::Placeholder`（`projection.rs:317-319`） | 该 block 单独显示骨架，其余 block 正常 |

**推论**：用户看到的"只有点击的 block 正常、其余全是骨架屏"**不可能是整窗骨架**
（整窗骨架帧里焦点 block 也会消失）。它一定是**已加载窗口 + 邻近 payload 缺失**
的混合态：编辑中 block 的 payload 被 editing pin 保护不被淘汰
（`payload_cache.rs:94-96`），而邻近 block 的 payload 已不在驻留窗口里。

"闪一下"则是另一回事：或者是一帧整窗骨架（stable 快照被丢弃且目标窗口未就绪），
或者是高度索引跳变导致的内容位移。两个现象的成因见下文。

## 根因

### 根因 0：绘制层的 deferred placeholder 骨架条（日志实锤的直接根因）

前四个根因修复后闪烁仍在，最终通过五层递进式日志（`CDITOR_TRACE_FLASH=1`）
定位：投影、单块渲染状态、跳帧路径全部稳定，但闪烁瞬间稳定出现

```
[cditor][flash][gui][text.deferred-placeholder] surface=Block(33) kind=Code …
```

机制：文档文本元素以 `require_prewarmed_layout` 模式渲染
（`block_content.rs`），measure 阶段**禁止同步排版**，只能使用排版缓存中的
现成快照（`text/element/layout_resolution.rs`）。当精确 key 与兼容快照都
miss 时，排版任务进入主线程预算队列，本帧 `layout = None`，paint 直接绘制
`deferred_placeholder_quads`——1~3 条灰色骨架条（`text/element/fallback.rs`）。
这完全绕过投影协议，之前所有 runtime 侧结论（"投影稳定"）都无法覆盖它。

触发条件（都会让 key 变化或快照消失）：

- Enter 拆分后被拆/新建 block 的 `content_version` 变化，重排在 Typing 模式
  的预算挤压下推迟 1~2 帧；
- 代码块异步高亮结果落地，替换同步 fallback spans，`marks_fingerprint`
  变化 → 新旧 key 双 miss；
- **（最终实锤的主因）宽度反馈环**：文本元素把排版后的自然宽度上报为盒子
  尺寸，代码卡片的 flex 测量将其缓存后在下一帧作为 `known.width` 喂回
  `measured_wrap_width`，于是 paint 以"内容自然宽度"（如 374px）排版，而
  prewarm 始终以投影宽度（766px）排版——两个宽度的快照在"每 surface 只保留
  一个几何"的缓存里逐帧互踢，**每个代码块每帧都在重排**；输入帧预算被挤压
  时重排推迟，即为骨架条闪烁。

**修复**（均已实施）：

1. **宽度权威归 runtime（根治）**：文档 block 的排版宽度以投影几何
   `DocumentTextGeometry`（经 `RichTextLayoutInput.width_px` 传入）为唯一
   权威，GPUI/taffy 的测量约束不再影响 shaping，盒子尺寸也上报为全宽、不再
   把内容宽度喂回下一帧（`text/element.rs` measure 闭包，仅限
   `SurfaceId::Block`；表格单元格、图注等真正由容器决定宽度的 surface 保持
   原逻辑）。UI 只是投影，宽度的真相在 runtime。此修复同时消除了每帧双重
   排版的性能浪费；`MAX_LAYOUT_GEOMETRIES_PER_SURFACE = 1` 的既有设计保持
   不变（曾试过放宽到 3 的补丁方案，与"resize 不留历史"的设计意图冲突，
   已回滚）。
2. **旧快照兜底（安全网）**：`resolve_measured_layout` 在 exact 与
   compatible 都 miss 时，取同 surface 最新的旧快照（`cditor-text` 新增
   `try_stale_text_layout_for_surface`）画一帧，下一帧真实排版落地后收敛；
   骨架条只保留给该 surface 从未有过快照的真正冷启动。安全约束：旧快照必须
   对齐一致且 wrap 宽度差 ≤1px，否则宁可骨架条也不画错位文本；旧快照标记
   `stale`，禁止发布为交互几何（hit test / IME）、无障碍投影或测量高度回流
   （`RichTextGpuiPrepaintState::stale_layout` 门控 `publish_text_layout`）。

### 根因 1：Enter 拆分走全量结构重建，且丢弃被拆 block 的实测高度（macOS/Windows 共同）

Enter 的完整链路：

```
Newline action (editor_view/render.rs:190)
→ GuiInputCommand::HandleEnter (input/routing.rs:567)
→ DocumentRuntime::handle_enter (structure_edit.rs:80)
→ split_focused_block_at_caret (structure_insert.rs:173-307)
→ apply_transaction → structure_changed=true (transaction_apply_structure.rs:111)
→ rebuild_structure_index (transaction_apply.rs:201-206)
```

`rebuild_structure_index`（`structure_index.rs:97-140`）一次做了所有重活：

- `structure_version + 1`（`structure_index.rs:104`）；
- 全量重建 `visible_index`、`list_projection_cache`；
- **把 `payload_window.block_range` 重置为 `0..total_visible_count`**（`:110-111`）；
- `rebuild_height_indexes_from_layout_meta`：O(n) 全量重建高度索引和分页索引，
  清空 `page_local_cache`。

同时，拆分对被拆 block 执行 `ReplacePayload`，其 staging 逻辑
（`transaction_apply_domain.rs:88-97`）：

```rust
layout.estimated_height = height_estimate.height;
layout.measured_height = None;   // ← 实测高度被销毁
layout.dirty = true;
```

新高度用 `estimate_block_height(kind, payload, layout_width_for_kind(kind))` 估算，
**用的是合成宽度而非实际渲染宽度**。对软换行 block 来说，估算行数与真实行数
几乎必然不一致 → 高度索引里该 block 从 `Exact` 掉到 `Default`
（`height_index.rs:97-107`），前缀和用错误估值计算 → **caret 之后的所有内容
在 1~2 帧内发生位移，直到 GPUI 重新测量并经 `flush_pending_height_corrections`
写回真实高度**。这就是 macOS 上"软换行 block 按 Enter 闪一下"的直接原因。

结构版本 bump 还有第二重后果：下一帧投影时

```rust
// projection.rs:91-95
stable.target.structure_version == self.document.visible_index.source_structure_version
```

不再成立 → `reconcile` 无条件丢弃 stable 快照（`layout_state.rs:116-118`）并
bump window generation。此后若目标窗口恰好未完全驻留（见根因 3 的原子就绪
判定），`reconcile` 落入 `ColdPlaceholder` → 整窗骨架一帧 → 闪烁。
Enter 场景下新 block 的 payload 是同步插入 `payload_window` 的
（`transaction_apply.rs:236`），所以多数情况 `desired_ready` 为真、不出整窗
骨架，闪烁主要来自高度跳变；但高度跳变本身会改变 desired 窗口范围，把未驻留
的 overscan block 拉进就绪判定，此时就会叠加一帧整窗骨架。

**普通打字对比**：单字符输入走预应用路径（`platform_text_edit.rs:8-53`），只
bump `content_version` / `layout_version`，不动 `structure_version`、不重建索
引、不清 `measured_height`。所以 macOS 普通打字不闪。

### 根因 2：Windows 每键测量高度抖动 → desired 窗口逐键变化 → 载入被反复作废（Windows 独有的放大器）

关键事实：**整条输入链路没有任何 `cfg(target_os)` 平台分支**，Windows 和 macOS
执行完全相同的代码。分歧全部来自平台文本测量结果：

1. Windows `WM_CHAR` → `replace_text_in_range(None, text)`（GPUI
   `gpui_windows/src/events.rs:104,407-411`），与 macOS `insertText` 语义一致，
   IME 组合路径不会被普通打字误入 —— 排除了输入协议差异。
2. 每次按键后编辑 block 重新测量。高度回流有 0.5px 容差
   （`layout_heights.rs:56-67,146-150`），**但没有任何 DPI/物理像素量化**
   （测量路径中 `scale_factor` 只用于字形光栅化）。DirectWrite 与 CoreText 的
   字体度量、行高、换行位置本就不同，Windows 常见的 1.25x/1.5x 分数缩放使
   f32 高度更容易逐帧产生 ≥0.5px 的差值。
3. 另一个抖动源：`TextLayoutApplyKey` 直接用 f32 位模式做缓存键
   （`cache/layout_update.rs:22-55` 的 `bounds_bits`），亚像素 bounds 抖动都会
   重新进入 `accept_text_layout`，且每次接受布局都调度一个后续帧
   （`layout_update.rs:98-110`）——放大了重绘频率（用户感知的"闪"）。

高度一旦变化 ≥0.5px，链式反应开始：

```
flush 高度 → height_index 增量更新（layout_heights.rs:167-183）
→ viewport_window_ranges 用新前缀和算出不同的 block 边界（window_planning.rs:54-100）
→ desired 窗口范围变化 → same_window_as 为假（layout_state.rs:162-167）
→ window.generation + 1
```

generation bump 的破坏力在载入侧：

- 每次重新规划都会 bump `payload_window_generation`（`payload_window.rs:88-90`），
  在途的载入结果按 `DiscardedStaleGeneration` 被整批丢弃
  （`payload_window.rs:225-231`）；
- 载入结果的主线程应用（`MainThreadWorkKind::WindowSwap`）在
  `InteractionMode::Typing/Composing` 期间被 `should_protect_input_frame` 整体
  推迟（`main_thread_budget.rs:396-399`），且滞后任务可能被 `is_stale` 判定
  丢弃（`main_thread_budget.rs:385-394`）；
- 可见载入是单车道的：车道忙时新请求只能存入 `pending_range`
  （`payload_window_port.rs:143-146`）。

于是在连续打字期间：**desired 窗口逐键漂移 → 在途载入不断作废 → 载入结果即使
返回也被输入帧保护推迟 → 邻近 block 的 payload 始终无法转为驻留**。编辑中
block 因 editing pin（`payload_cache.rs:94-96`）payload 始终在，其余 block 落
入 `BlockPayloadView::Placeholder` → 正是"除了点击的 block，其余都是骨架屏"。

macOS 上 CoreText 度量稳定（整数倍缩放 + 一致的行高），同一段代码 0.5px 容差
足以吸收抖动，desired 窗口不漂移，所以看不到这个现象。

附注：Windows 每键还会同步执行 `ImmSetCompositionWindow`/`ImmSetCandidateWindow`
并在每个帧请求消息上调 `accepts_text_input` → `session.input_context()`
（GPUI `gpui_windows/src/events.rs:601-628,1247`）。这些查询已验证为只读、无副
作用（`input_port.rs:40-71`），只增加每键工作量，不是骨架屏成因。

### 根因 3：整个渲染窗口（含 overscan）是原子就绪单元，放大了"未就绪"概率（结构性缺陷）

`viewport_window_ranges` 把 `visible_block_range` 直接设为整个渲染窗口
（`window_planning.rs:117-122`，含最多一个 viewport 的 overscan、上限 320 块）：

```rust
// The complete bounded render window is the atomic readiness unit.
visible_block_range: block_range.clone(),
```

`desired_ready = payloads_resident_for(&desired.visible_block_range)`
（`projection.rs:83`）因此要求 **overscan 里的每一个 block 都驻留** 才算就绪。
任何一个 overscan 边缘 block 缺失都会把整个投影推入 not-ready 路径；一旦此刻
stable 快照又刚被结构 bump 丢弃（根因 1）或从未建立，就是整窗骨架。

这条设计原本是为了防止滚动时"已加载核心 + 骨架 overscan"混排（见代码注释），
但它与"结构 bump 即丢快照"叠加后，把就绪判定变成了全有或全无。

### 已排除的假设

- ~~Windows 普通打字误入 IME composition 路径~~：`WM_CHAR` 硬编码
  `replace_text_in_range(None, …)`，composition API 只在真实 IME 组合时触发。
- ~~cx.notify 重绘本身导致骨架~~：重绘不改变 payload 驻留；骨架必然对应
  payload 缺失或 placeholder 投影。
- ~~焦点 block 有独立渲染实体在整窗骨架时兜底~~：不存在 pinned 渲染实体，
  `pinned` 只保护 payload 不被淘汰；整窗骨架帧连焦点 block 都不渲染。
- ~~cache eviction 是主因~~：typing 期间 trim 被
  `payload_cache_trim_allowed`（`app/payload_cache.rs:181`）阻止；淘汰只在
  idle 后发生且保护范围覆盖 stable/preparing/编辑 pin。它最多在窗口身份切换
  间隙参与竞态，是次要放大器。

## 解决方案

> **实施状态（2026-08-10）**：方案 1、2、3.1、3.2、3.3、4 已全部实施并通过回归测试。
> - 方案 1 中"保留/平移 `payload_window.block_range`"一项经评估后**不改**：
>   重置为 `0..total` 时 `payload_window_covers` 的范围包含检查恒真，驻留判断
>   完全由逐 block 存在性决定，反而是更保守、不会误报 not-covered 的选择；
>   下一帧 Stable 决策（`projection.rs`）即恢复正常窗口范围。
> - 方案 4 的"载入按 block 应用"一项**已是现状**，无需修改：`apply_payload_result`
>   即使代次过期也会为仍持有 loading 标记的 block 插入记录（见
>   `payload_window.rs` 中 "Results from an older viewport are still valid
>   cache data" 注释），`DiscardedStaleGeneration` 只是给调用方的状态报告。
> - 方案 2 实施为 `ProjectionWindowDecision::StaleFallback`：结构编辑使 stable
>   快照失效时，快照降级存入 `publication.stale_fallback`；desired 未就绪且
>   滚动位置未远离（|Δscroll| < 半 viewport、非滚动条拖拽）时重放旧帧，
>   成功发布或终端失败时清除。终端失败仍显示错误面板，不被旧帧遮蔽。
> - 方案 4 实施为：`visible_block_range` 收敛为 viewport 物理核心（滚动条
>   拖拽仍用整窗）；GPUI 层对核心之外的 placeholder block 渲染透明占位
>   （`document_editor_view.rs`），骨架只保留给可见核心的冷启动/拖拽场景。

按收益/风险排序，分四步。每一步都可独立落地、独立验证。

### 方案 1：Enter 拆分保留实测高度（修 macOS 闪烁的主因，低风险）

`transaction_apply_domain.rs:88-97` 不再无条件 `measured_height = None`：

- 被拆 block（保留前半段文本）：按"前半段字符数 / 原字符数"比例折算原
  `measured_height`，并对齐到行高整数倍；`dirty = true` 保留，等待真实重测。
  比例估算的误差远小于换一个合成宽度重新估算。
- 新 block（后半段）：同理用剩余比例折算，而不是 `estimate_block_height`。
- 目标：Enter 后第一帧的前缀和与真实值误差 < 1 行高，消除可见位移。

同时把 `rebuild_structure_index` 里 `payload_window.block_range = 0..total`
（`structure_index.rs:110-111`）改为保留/平移原窗口范围（单块插入只需要把
insert 点之后的 index 平移 1）。全量重置会让下一帧的窗口协议先经历一次
"范围异常大 → 截断"的往返。

### 方案 2：结构编辑后立即重发布 stable 快照，而不是丢弃（修"闪一下"的骨架帧）

现状：`stable_valid` 只做 `structure_version` 相等判断，任何结构编辑（哪怕
一次本地 Enter，payload 同步驻留、完全可以立即重投影）都先丢快照、再看运气。

改法（`projection.rs:88-97` + `layout_state.rs:108-158`）：

1. `reconcile` 增加一个分支：stable 因 structure_version 失效但
   `desired_ready == true` 时，直接走 `Stable(desired)` 重投影并发布新快照
   ——这在现有代码里已经成立（`:130-133` 在失效判定之后执行），需要补的是
   **`desired_ready == false` 的情形**：
2. 结构编辑属于本地事务时（Enter/删除/移动，事务里带有受影响 block 集合），
   不要把旧快照直接置 None，而是把它标记为 `stale-but-presentable`：重投影
   失败时继续展示旧快照内容（旧结构、旧高度），禁止落入 `ColdPlaceholder`。
   只有文档换载、折叠大范围变化等无法安全复用旧帧的场景才真正丢弃。
3. 验收：本地结构编辑的下一帧，`placeholder_window_height` 必须为 `None`。

**决策点（与总体架构方案的关系）**：`large-document-rich-text-architecture.md`
中"窗口原子提交"的意图是防止混排骨架；本方案把"原子"的语义从
"要么新窗口要么骨架"改为"要么新窗口要么上一个完整旧帧"，与架构意图一致，
但属于协议语义修改，需要用户确认。

### 方案 3：测量高度量化 + 输入期窗口规划冻结（修 Windows 逐键漂移）

三个互相独立的子项：

1. **物理像素量化**：`queue_measured_height` 入口处把测量高度按
   `(h * scale_factor).round() / scale_factor` 对齐到物理像素网格，再做
   0.5px 容差比较。同时把 0.5px 绝对阈值改为
   `max(0.5, line_height * 0.05)` 之类的相对阈值。消除 DirectWrite 在分数
   缩放下的亚像素抖动源头。
2. **缓存键量化**：`TextLayoutApplyKey` 的 `bounds_bits` 不再用原始 f32 位
   模式，量化到物理像素后再取位。消除无意义的布局重进入和后续帧调度。
3. **输入期规划滞回（hysteresis）**：`InteractionMode::Typing/Composing`
   期间，若 desired 窗口范围变化仅由高度回流引起（scroll_top 与结构版本都
   未变），且新旧范围偏移 < N 块（建议 N = 8）或 < 半个 viewport，则沿用上
   一个 desired，不 bump generation、不作废在途载入。滚动、结构编辑、翻页
   不受此限制。这一条同时消灭了"在途载入被逐键作废"的循环。

### 方案 4：拆开就绪单元，部分驻留不降级（修结构性放大器）

`window_planning.rs:117-122` 把 `visible_block_range` 收敛回真实 viewport
核心（当前代码里已经单独算出了 `viewport_end`，只是最后没用它），overscan
不参与 `desired_ready` 判定；同时保留滚动条拖拽时的全窗口就绪特例
（`projection.rs:67-74` 已有）。

为了不回到"已加载核心 + 骨架 overscan 混排"的老问题，配套规则：

- overscan 内缺 payload 的 block **不渲染骨架**，渲染纯占位高度（透明空盒，
  用 `layout.effective_height()`）；骨架只保留给可见核心的冷启动/失败态。
- 在途 overscan 载入不因 generation bump 作废，只要其范围与新 desired 相交
  就照常应用相交部分（`apply_payload_result` 按 block 应用而非按代次整批
  丢弃）。

**决策点**：这一条与架构文档中"完整渲染窗口是原子就绪单元"的注释直接冲突
（该注释就写在 `window_planning.rs:117-120`），是有意的设计变更，需要用户
确认后再实施。

### 实施顺序与依赖

```
方案 1（高度保留）──┐         ✅ 已实施（split_height.rs + transaction_apply_domain.rs + structure_insert.rs）
方案 3.1/3.2（量化）─┤        ✅ 已实施（measured_height_tolerance_px / quantize_measured_height_to_physical_pixels / TextLayoutApplyKey 1/8px 量化）
方案 3.3（滞回）────┼─→ 方案 2（快照保留）─→ 方案 4（就绪单元拆分）
   ✅ 已实施          │        ✅ 已实施            ✅ 已实施
（viewport_window_ranges_planned，8 块滞回）
```

### 实施细节备注

- 滞回的触发条件（`window_planning.rs::viewport_window_ranges_planned`）：
  structure_version 与 visibility_version 均未变、scroll_top 偏移 < 0.5px、
  非滚动条拖拽、新旧窗口每边漂移 ≤ 8 块、且上一窗口仍覆盖当前物理
  viewport 核心。任何一条不满足立即重规划——滚动、折叠、结构编辑都会
  绕过滞回。folding 的防护通过新增的
  `ProjectionWindowState::desired_visibility_version` 实现（折叠只改
  visibility_version 不改 structure_version）。
- 高度容差改为按 kind 分级：文本类 `max(0.5, line_height × 0.05)`，
  非文本类（图片拖拽缩放等连续测量）保持 0.5px 绝对容差不变。

## 回归测试要求

runtime 层（`cditor-runtime`，可全部离屏测试）：

1. Enter 拆分软换行 block 后，被拆 block 与新 block 的
   `effective_height()` 之和与拆分前 `measured_height` 误差 < 1 行高。
2. Enter 拆分后下一次 `projection_for_window_planned` 的
   `placeholder_window_height` 为 `None`（payload 同步驻留场景）。
3. 模拟 Windows 抖动序列：对编辑 block 连续 queue 一组 ±0.6px 交替的测量
   高度，断言 desired 窗口范围不变、`window.generation` 不增长（滞回生效）。
4. 输入期间（Typing mode）在途载入不因高度回流被 `DiscardedStaleGeneration`
   作废。
5. 结构编辑后 stable 快照重发布：`desired_ready=false` 时投影仍返回旧帧内
   容（`blocks` 非空），而非 placeholder。
6. 量化函数单测：1.25/1.5/2.0 scale factor 下，物理像素对齐后的往返误差为 0。

GPUI 层（`cditor-editor-gpui`）：

7. `TextLayoutApplyKey` 对亚物理像素 bounds 抖动稳定（同一逻辑布局不产生
   新 key）。
8. 已有测试 `enter_in_quote_soft_wraps_and_grows_block_height`
   （`tests/delete_navigation_height.rs:472`）保持通过。

人工验收（两平台）：

- Windows 1.25x/1.5x 缩放下连续输入 50 字符：无整页/邻近骨架、无闪烁。
- Windows 中文 IME 组合态：非编辑 block 稳定。
- macOS 软换行 block 按 Enter：无内容位移、无闪烁。
- 两平台滚动条快速拖拽：仍按现有协议显示整窗骨架（该行为是预期的）。

## 附：证据索引

| 结论 | 位置 |
|---|---|
| 整窗骨架帧 blocks 为空 | `cditor-runtime/src/document_runtime/projection/placeholder.rs:70,73` |
| 单 block 骨架来自 payload 缺失 | `cditor-runtime/src/document_runtime/projection.rs:317-320` |
| stable 快照失效判定 | `cditor-runtime/src/document_runtime/projection.rs:91-95` |
| structure_version 唯一 bump 点 | `cditor-runtime/src/document_runtime/structure_index.rs:104` |
| Enter 全量重建 + 窗口重置 | `cditor-runtime/src/document_runtime/structure_index.rs:97-140` |
| 拆分丢弃实测高度 | `cditor-runtime/src/document_runtime/transaction_apply_domain.rs:88-97` |
| 新 block payload 同步插入 | `cditor-runtime/src/document_runtime/transaction_apply.rs:236` |
| 打字只 bump content/layout version | `cditor-runtime/src/document_runtime/platform_text_edit.rs:8-53`、`local_transaction.rs:344-361` |
| 高度回流 0.5px 容差、无 DPI 量化 | `cditor-runtime/src/document_runtime/layout_heights.rs:56-67,146-150` |
| 高度变化 → desired 窗口漂移 | `cditor-runtime/src/document_runtime/projection/window_planning.rs:54-100` |
| generation bump 作废在途载入 | `cditor-runtime/src/document_runtime/payload_window.rs:88-90,225-231` |
| Typing 期间推迟窗口应用 | `cditor-runtime/src/scheduling/main_thread_budget.rs:385-399` |
| 整窗原子就绪单元 | `cditor-runtime/src/document_runtime/projection/window_planning.rs:117-122` |
| editing pin 只保护 payload | `cditor-runtime/src/document_runtime/payload_cache.rs:94-96` |
| f32 位模式缓存键 | `cditor-editor-gpui/src/cache/layout_update.rs:22-55` |
| 输入链路无平台分支 | `cditor-editor-gpui/src/input/`（仅 `actions.rs` 键位、mobile feature 有 cfg） |
| Windows WM_CHAR 协议 | GPUI `gpui_windows/src/events.rs:104,407-411`（pinned rev `1d217ee`） |

## 附加根因：代码块 IME 组合预览反复失效（同类结构缺陷）

**现象**：代码块（含 Mermaid 源码）里用中文输入法打字，看不到拼音临时字母；
历史上多次修复后又复发。

**根因**：与宽度乒乓同类——**prewarm 与 paint 各自构造排版输入**。paint 侧
（`block_content.rs`）在 `marked_range` 激活时会跳过语法高亮（投影 payload
已含预览文本，高亮缓存只认识已提交文本）；而 prewarm 侧
（`text_layout_prewarm.rs`）无条件套用高亮 spans——组合期间它以**旧文本**排版
并占据缓存。paint 的正确输入（含预览）miss 后，若走了 stale 兜底或预算推迟，
屏幕上就一直是旧文本，临时字母永远不出现。runtime 侧注入管线本身是完好的
（`payload_with_composition_preview` 对 Code/Html/RichText/表格单元格均正确，
有回归测试锁定）。

**修复（结构性，消灭整类分歧）**：

1. `block/layout_input.rs`：新增 `document_block_layout_input` ——文档 block
   排版输入的**唯一构造点**（含组合态高亮门控），paint 与 prewarm 共用；
   任何一方单独改逻辑在结构上不再可能。
2. `text/element.rs`：**焦点编辑 surface 永远同帧同步排版**——
   `require_prewarmed` 对焦点元素失效，IME 预览与 caret 几何不受调度预算和
   stale 兜底影响；同时 stale 兜底命中时也会照常入队真实排版并在同帧尝试
   换用新结果，杜绝旧帧长期驻留。

**回归测试**：runtime 侧
`composition_preview_is_projected_for_code_and_mermaid_blocks`（Code 与
Mermaid kind 的投影 payload 必须含预览文本且携带 marked_range）；GPUI 侧
`layout_input.rs` 单元测试锁定"组合态保留预览文本、提交态套用高亮/纯文本"。

## 附加根因：特殊 block IME 候选框遮挡文字（宽度真相分裂的第三次发作）

**现象**：代码块等特殊 block 中文输入时，候选框覆盖正在输入的文字。

**根因**：候选框位置来自精确几何链
`bounds_for_range → projected_text_geometry_for_block → layout_cache_is_current`，
其中 wrap 宽度按位比较（`matches_text_constraints`）。三个宽度消费者不一致：
投影 rect 用 runtime 权威宽度（如代码块 766），而 paint 发布的平台布局与注册
的输入 identity 用 **GPUI 盒子宽度**（短代码块 = 自然文本宽度，如 374）。
位比较失败 → 布局缓存被判不新鲜 → 精确几何不可用 →
`ime_candidate_fallback_bounds` 把候选框锚在**文本元素左上角**——正好压在
正在输入的文字上。段落不受影响（盒宽 = 排版宽），特殊 block（代码/Mermaid
源码，自然宽 < 投影宽）必现。

**修复**：`SurfaceId::Block` 的文档文本在 paint 时，发布的
`RichTextPlatformLayout.wrap_width_px` 与注册的
`TextPlatformLayoutIdentity.wrap_width_bits` 一律使用 runtime 投影宽度
（`input.width_px`，与 shaping 宽度同源），普通元素与 segmented 元素同步修改；
表格单元格等容器定宽 surface 保持盒宽。至此宽度真相在
shaping、投影 rect、发布布局、输入 identity 四处完全统一。
