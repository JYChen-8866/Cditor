# 分页索引与页内索引完善方案

> 目标：把当前“全局 BlockHeightIndex + PageLayoutIndex + 当前窗口局部索引”的实现，完善为一套清晰、可持久化、可增量修正、可恢复的分页索引体系。
>
> 适用范围：10w block 大文档、复杂富文本、跨页 selection、无感滚动、局部编辑、fold/unfold、resize、图片/表格高度收敛。

---

## 1. 实施状态

实施前代码已经具备以下基础能力：

- `crates/cditor-core/src/layout/height_index.rs`
  - `BlockHeightIndex`
  - 全局 block 高度前缀和
  - `block_at_offset / offset_of_block / update_height`
  - `Historical / Predictive / Default / Exact` 置信度

- `crates/cditor-core/src/layout/page_layout.rs`
  - `PageLayoutIndex`
  - page 切分
  - page 高度聚合
  - page → block 范围映射
  - page summary 反推总高度
  - 支持从 block height index 生成 page layout

- `crates/cditor-runtime/src/document_runtime`
  - Runtime 已使用全局高度索引与 page layout 进行滚动、anchor 修正、window 规划
  - 当前窗口还会构建局部 `RenderWindow.local_height_index`

本轮已经补齐：

- page identity、cache 校验与失配 fallback
- versioned page summary 与运行时 page-local detail 分层
- page-local Fenwick、offset lookup、批量 update 与 dirty range
- page split / merge 及后缀 page 重编号
- storage summary 恢复与 runtime 热页重建
- global / page / page-local / window-local 的单向真相边界

---

## 2. 架构目标

### 2.1 索引分层目标

应形成四层索引：

```text
DocumentIndex
→ BlockHeightIndex
→ PageLayoutIndex
→ Page-local Index
→ Window-local Index
```

#### DocumentIndex
- 保存结构真相：block 顺序、父子关系、kind、visible order、轻量 layout meta
- 负责 block 身份和顺序的一致性

#### BlockHeightIndex
- 保存全局高度真相
- 负责 `block index ↔ global offset`
- 负责整篇文档的 prefix sum

#### PageLayoutIndex
- 保存分页聚合真相
- 负责 `global offset ↔ page`
- 负责 page summary、page height、page 置信度、page 版本

#### Page-local Index
- 保存 page 内的局部高度加速视图
- 负责 `local block index ↔ local offset`
- 负责局部 height update / 局部 dirty range

#### Window-local Index
- 保存当前 viewport / overscan 的即时布局真相
- 负责 hit test、selection、caret、局部绘制

### 2.2 设计原则

1. **全局索引是真相源，分页索引是聚合层，页内索引是局部加速层。**
2. **Page 是布局派生物，不是结构真相。**
3. **页内索引可以独立加载、独立失效、独立收敛。**
4. **任何索引更新都必须可验证版本一致性。**
5. **不会为了局部方便把 page 变成第二套主真相。**

---

## 3. 已落地的关键改造

### 3.1 PageLayoutIndex identity 与缓存边界

`PageLayoutIndex` 现在能够：

- 从 block 高度序列切页
- 聚合 page height
- 记录 page index / block range / measured ratio / confidence
- 从 cached pages 恢复并验证 identity
- 通过 `CachedPageMismatchPolicy` 明确 Reject / Historical / Rebuild
- 派生每个 page 的完整 `PageLayoutIdentity`
- split / merge 后重新聚合 summary 和 page prefix

### 3.2 BlockHeightIndex 与 PageLocalHeightIndex

`BlockHeightIndex` 继续作为唯一全局高度真相，并新增 range view。`PageLocalHeightIndex` 负责：

- local prefix sum 与双向 offset lookup
- 单 block / range height update
- confidence、max-error 和 dirty range 聚合
- current / neighbor page 热缓存
- 结构或 identity 失配后的整体失效重建

### 3.3 Page-local 与 Window-local 的所有权边界

`PageLocalHeightIndex` 常驻 current / neighbor 热页；`RenderWindow.local_height_index` 仍是帧内 owned bounded index。projection 从 page-local cache 或 global truth 读取高度与 confidence 后构造 window-local index，不持有跨帧借用，因此结构变化时不会悬挂旧局部视图。

---

## 4. 需要拆解的实施任务

下面按架构分层拆成任务。每个大任务都建议进一步拆为小任务执行。

### 4.1 任务组 A：补齐 PageLayoutIndex 的身份与版本边界

- [x] 定义 page identity 的最小集合
  - [x] `document_id`
  - [x] `structure_version`
  - [x] `visibility_version`
  - [x] `layout_key_hash`
  - [x] `page_policy_version`
  - [x] `page_index`
- [x] 检查 `PageLayoutIndex` 当前是否能表达这些 identity
  - [x] 若不能，新增 `PageLayoutIdentity`
  - [x] 将 page summary 与 identity 分离
- [x] 增加 page identity 的一致性校验
  - [x] cached page 加载时校验 document / structure / visibility / layout key / policy
  - [x] 版本不匹配时支持 Reject、Historical 或 Rebuild
- [x] 明确 page summary 是否可跨版本复用
  - [x] identity 完全匹配时按原 confidence 恢复
  - [x] structure / visibility / layout key / policy 失配时不能作为 Exact
  - [x] fallback 由 `CachedPageMismatchPolicy` 显式选择

### 4.2 任务组 B：完善 PageLayoutIndex 的分页语义

- [x] 梳理 `PageLayoutIndex::from_block_estimates` 的切页规则
  - [x] `max_blocks`
  - [x] `target_height`
  - [x] `max_estimated_cost`
  - [x] `max_text_bytes`
  - [x] `max_inline_runs`
  - [x] `max_complex_blocks`
- [x] 明确 page summary 的字段语义
  - [x] `block_start`：当前 visible order 中的起始下标
  - [x] `block_count`：连续覆盖的可见 block 数
  - [x] `height`：页内 block 高度总和
  - [x] `measured_ratio`：Exact block 数占比
  - [x] `confidence`：页内最弱高度置信度
  - [x] `max_error_hint`：页内误差提示之和
  - [x] `dirty`：局部变更未持久化或仍含非 Exact 高度
- [x] 增加 page split / merge 规则
  - [x] policy 预算超限时由重分页触发 split，也支持显式 `split_page`
  - [x] 相邻小页可通过 `merge_page_with_next` 合并
  - [x] split / merge 后统一重编号并传播后缀 `block_start`
  - [x] summary 从全局高度真相重新聚合，不做比例猜测
- [x] 明确 page summary 是“聚合缓存”而不是结构真相
  - [x] summary 可恢复且必须版本化；失配时降级或重建

### 4.3 任务组 C：补齐真正的页内索引

- [x] 定义 page-local index 的职责
  - [x] 页内 block 顺序
  - [x] 页内 prefix sum
  - [x] 页内 offset lookup
  - [x] 页内 height update
- [x] 设计 page-local index 结构体
  - [x] 不复制 local block ids；由 `VisibleDocumentIndex + block_start/range` 提供顺序真相
  - [x] local heights
  - [x] local confidence
  - [x] local Fenwick / prefix tree
- [x] 明确 page-local index 的构建入口
  - [x] 从 page summary + 全局 `BlockHeightIndex` 构建
  - [x] projection 预热 current / neighbor page 后供 window 裁切
  - [x] 持久化仅恢复 summary，local detail 运行时重建
- [x] 明确 page-local index 的失效条件
  - [x] structure_version 变化
  - [x] visibility_version 变化
  - [x] layout_key 变化时由缓存 identity 阻止复用
  - [x] page split / merge 后重建
  - [x] page 内 block 插入删除移动后随全局结构重建
- [x] 定义页内索引的精确查询 API
  - [x] local block index → local offset
  - [x] local offset → local block index
  - [x] local block height update
  - [x] local range total height

### 4.4 任务组 D：建立全局索引与页内索引的协同协议

- [x] 明确四者职责边界
  - [x] `DocumentIndex`：顺序与结构真相
  - [x] `BlockHeightIndex`：全局高度真相
  - [x] `PageLayoutIndex`：分页聚合真相
  - [x] `Page-local Index`：局部加速真相
- [x] 明确 local-only 更新边界
  - [x] local 可暂存 dirty range 和批量测量结果
  - [x] image/table/文本高度一旦提交，必须在同一批次回写 global 与 summary
- [x] 明确必须回写 global 的更新
  - [x] block 插入 / 删除 / 移动
  - [x] fold / unfold
  - [x] page split / merge
  - [x] layout key 变化
- [x] 定义 global→local→page 的提交流程
  - [x] 先更新全局高度真相
  - [x] 再重建命中页 local detail
  - [x] 再聚合 page summary / page prefix
  - [x] 整批完成后只触发一次 anchor 修正
- [x] 定义 global→local 的重建流程
  - [x] 全局结构变化后清空热页 cache
  - [x] identity 匹配的 summary 可复用
  - [x] current / neighbor page 在 projection 前按需 build

### 4.5 任务组 E：补齐分页索引的持久化闭环

- [x] 设计 page layout 持久化字段
  - [x] page identity
  - [x] page block range 与首尾 block identity
  - [x] page height
  - [x] measured_ratio
  - [x] confidence
  - [x] max_error_hint
  - [x] dirty
- [x] 设计 page-local index 持久化策略
  - [x] 不全量持久化 detail，避免复制全局高度真相
  - [x] 仅持久化 versioned summary
  - [x] current / neighbor local index 运行时重建
- [x] 定义 layout_key 的版本体系
  - [x] width bucket / exact width
  - [x] content/attrs/style/font/theme/scale
  - [x] structure / visibility version
  - [x] page policy version
- [x] 定义缓存失配降级策略
  - [x] summary 支持 Reject / Historical / Rebuild
  - [x] local index 失配直接丢弃并重建
  - [x] 历史高度不可标为 Exact
- [x] 定义冷启动恢复流程
  - [x] 先校验并恢复 summary
  - [x] projection 前恢复 current / nearby local index
  - [x] 后续测量通过既有 scheduler refine

### 4.6 任务组 F：修正当前代码中的索引接入点

#### F1. `crates/cditor-core/src/layout/page_layout.rs`

- [x] 检查是否需要新增 `PageLayoutIdentity`
  - [x] 是否需要把 `document_id` / `structure_version` / `visibility_version` / `layout_key_hash` / `page_policy_version` / `page_index` 作为独立结构体
  - [x] 是否需要让 `PageLayoutIndex` 持有 identity 而不是只持有 `PagePolicy`
- [x] 检查是否需要新增 page summary / detail 的分层字段
  - [x] summary：`block_start` / `block_count` / `height` / `confidence` / `dirty`
  - [x] detail：页内 block 高度与局部 prefix
- [x] 新增 page split / merge API
  - [x] `split_page(page, split_at, global)`
  - [x] `merge_page_with_next(page, global)`
  - [x] mutation 后重建 page prefix 并传播后缀 `block_start`
- [x] 新增 cached page 失配处理
  - [x] identity 不匹配可 Reject
  - [x] 允许显式降级为 Historical
  - [x] 支持返回 RebuildRequired
- [x] 补齐 page query API
  - [x] `page_at_offset`
  - [x] `page_for_block_index`
  - [x] `offset_of_page`
  - [x] `page_bounds`

#### F2. `crates/cditor-core/src/layout/height_index.rs`

- [x] 新增 page-local 视图构造器
  - [x] `BlockHeightIndex::view(range)`
  - [x] `PageLayoutIndex::local_height_index_from_global`
  - [x] snapshot 只恢复 summary，detail 从全局索引重建
- [x] 新增局部更新辅助 API
  - [x] 单 block 与 range height update
  - [x] insert/delete/move 归全局结构层处理，local cache 失效重建
  - [x] dirty range 合并与清理
- [x] 新增与 page summary 的同步工具
  - [x] local total → page height
  - [x] local confidence → page confidence
  - [x] local measured ratio / max error → page summary
- [x] 补齐 offset 映射辅助
  - [x] block index → local offset
  - [x] local offset → block index
  - [x] block boundary / 文档末尾 clamp

#### F3. `crates/cditor-runtime/src/document_runtime/layout_state.rs`

- [x] 统一 `height_index` 与 `page_layout` 更新链
  - [x] 先写 global truth，再同步 local / page summary
  - [x] page prefix 后同步 scroll total height
  - [x] 整批高度修正只恢复一次 anchor
- [x] 引入 page-local cache
  - [x] 当前热页 cache
  - [x] 前后邻页 cache
  - [x] local dirty range 与 page dirty summary
- [x] 明确 dirty / range 信息
  - [x] page summary dirty
  - [x] page-local dirty range
  - [x] window 继续使用既有 bounded block range
- [x] 明确 page 更新对 scroll / anchor 的影响
  - [x] viewport anchor 上方高度变化时 restore
  - [x] scrollbar drag 期间只更新 model，延迟 displayed correction

#### F4. `crates/cditor-runtime/src/document_runtime/projection.rs`

- [x] 检查并约束 window local 重建
  - [x] `RenderWindow` 仍拥有 bounded local index，避免借用跨帧失效
  - [x] 高度与 confidence 优先读取 page-local cache，未命中回退 global
- [x] page-local cache 接入 window planning
  - [x] 当前页数据供 window 裁切
  - [x] 邻页预热
  - [x] 远端页仅保留 summary

#### F5. 测试目录

- [x] page layout tests
  - [x] 分页切分正确
  - [x] page summary 正确
  - [x] page identity / fallback 正确
- [x] height index property tests
  - [x] 全局 prefix 正确
  - [x] offset 映射正确
  - [x] 随机高度更新后不变量成立
- [x] window projection tests
  - [x] 当前窗口局部索引正确
  - [x] 局部编辑与 atomic projection 不降级为 skeleton
- [x] large window tests
  - [x] 10w block 打开与 bounded projection
  - [x] 连续滚动 / window planning
  - [x] 当前页编辑与高度收敛
  - [x] 跨页 selection 已由 runtime selection suite 覆盖

### 4.7 任务组 G：补强测试

#### G1. PageLayoutIndex 单元测试

- [x] page 切分正确
  - [x] 按 `max_blocks` / `target_height` 切分
  - [x] 按 cost / text / inline-runs / complex 预算切分
- [x] page 总高度正确
  - [x] total height 等于所有 page height 之和
  - [x] page update / split / merge 后总高度同步
- [x] page offset 映射正确
  - [x] `offset_of_page` / `page_at_offset` / `page_bounds`
  - [x] 边界值与末尾值
- [x] cached pages 恢复与失配正确
  - [x] 版本一致 Exact 恢复
  - [x] Historical / Rebuild fallback

#### G2. Page-local index 单元测试

- [x] local offset lookup 双向映射
- [x] 单 block 与 range height update
- [x] dirty range 合并与清理
- [x] local total / confidence / measured ratio / error 回写 summary

#### G3. 全局与局部一致性测试

- [x] page summary.height 与 local total 一致
- [x] global prefix 与 page prefix 一致
- [x] page split / merge 后边界和映射正确

#### G4. Property tests

- [ ] 随机 insert/delete/move 后映射仍正确
- [ ] 随机 fold/unfold 后 page 与局部索引仍正确
- [ ] 随机 resize 后高度收敛仍正确
- [ ] 随机 page split/merge 后版本仍正确

#### G5. 端到端测试

- [x] 10w block 打开与 bounded window
- [x] 连续滚动 / scrollbar drag / 冷跳转 atomic commit
- [x] 当前页编辑导致高度变化
- [x] 跨页 selection 与 copy
- [x] 局部索引失配后按 identity 重建

---

## 5. 实施顺序与状态

### Phase 1：先把分页身份补齐

目标：让 page 是一个有版本的可恢复对象。

任务：

- [x] page identity
- [x] layout key
- [x] cached page 校验
- [x] 失配 fallback

### Phase 2：再补页内索引

目标：让 page 内部的定位和修正不必依赖整全局重建。

任务：

- [x] page-local index
- [x] local prefix sum
- [x] local offset lookup
- [x] local height update

### Phase 3：打通全局与局部协同

目标：更新只动该动的层，不动多余层。

任务：

- [x] global truth→local→page 回写
- [x] global→local 失效重建
- [x] split / merge

### Phase 4：补持久化闭环

目标：重启后能快速恢复分页和页内定位。

任务：

- [x] page summary cache
- [x] runtime hot page-local cache（不持久化 detail）
- [x] layout key versioning
- [x] cold start recover

### Phase 5：补测试与回归门禁

目标：保证大文档场景不抖、不乱、不退化。

任务：

- [x] 单测
- [x] 高度 / page prefix property test
- [x] 现有 runtime 端到端与 10w window 测试
- [ ] trace replay

---

## 6. 当前代码的直接改造结果

### 6.1 `crates/cditor-core/src/layout/page_layout.rs`

已完成：

- 新增 page identity 结构
- 新增 page summary / page detail 的区分
- 新增 split / merge 辅助 API
- 新增 cached page 校验入口

### 6.2 `crates/cditor-core/src/layout/height_index.rs`

已完成：

- page-local 构造器
- 局部范围视图
- 与 page summary 同步的辅助函数
- 更适合局部更新的 API

### 6.3 `crates/cditor-runtime/src/document_runtime/layout_state.rs`

已完成：

- page-local cache 字段
- page-local dirty 标记
- page 级更新后的 anchor 修正入口
- page summary 与全局索引同步状态

### 6.4 `crates/cditor-runtime/src/document_runtime/projection.rs`

已按以下边界接入，索引逻辑没有继续堆入 projection：

- current window 优先读取 page-local index 后构建 owned bounded index
- 当前页与相邻页的布局尽量来自缓存
- 局部重建只针对需要更新的范围

---

## 7. 文档结论

完成后的实现可以概括为：

```text
DocumentIndex 保存结构与 visible order 真相
BlockHeightIndex 保存全局高度真相
PageLayoutIndex 保存带 identity 的 versioned summary 与 page prefix
PageLocalHeightIndex 保存当前页和邻页的运行时局部加速数据
RenderWindow 保存 bounded、帧内稳定的 window-local index
storage 恢复 summary，runtime 按需重建 page-local detail
```

page-local detail 明确不全量持久化，也不复制 block ID 顺序，避免形成第二套结构或高度真相。高度提交遵循 global truth → local cache → page summary → scroll total → 单次 anchor restore。

---

## 8. 验证结果与后续门禁

本轮完成并通过：

1. `cargo check -p cditor-storage -p cditor-runtime -p cditor-session -p cditor-desktop`
2. `cditor-core` page layout：13 tests
3. `cditor-storage` page snapshot：2 tests
4. `cditor-runtime`：580 tests
5. `cditor-session` cold start：7 tests

仍保留为后续质量门禁，而不是本次索引模型的阻塞项：

1. 随机 insert/delete/move/fold 与 split/merge 的组合 property test
2. resize、冷跳转和异步测量结果的 trace replay
3. page-local cache 命中率、重建耗时和内存压力 telemetry
