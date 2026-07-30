# 分页索引与页内索引完善方案

> 目标：把当前“全局 BlockHeightIndex + PageLayoutIndex + 当前窗口局部索引”的实现，完善为一套清晰、可持久化、可增量修正、可恢复的分页索引体系。
>
> 适用范围：10w block 大文档、复杂富文本、跨页 selection、无感滚动、局部编辑、fold/unfold、resize、图片/表格高度收敛。

---

## 1. 现状判断

当前代码已经具备基础能力：

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

但目前还不是完整的“分页索引 + 页内索引”体系，因为还缺少：

- 明确的 page identity 版本边界
- page summary 与 page detail 的分层
- 独立 page-local 高度索引
- page split / merge 的局部更新协议
- page 局部索引的持久化与恢复
- 全局索引、分页索引、页内索引的职责边界

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
- 保存 page 内的局部高度真相
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

## 3. 当前实现的具体缺口

### 3.1 PageLayoutIndex 目前已有，但身份边界不够完整

当前 `PageLayoutIndex` 已经能：

- 从 block 高度序列切页
- 聚合 page height
- 记录 page index / block range / measured ratio / confidence
- 从 cached pages 恢复

但还缺：

- page identity 的完整版本字段
- 和 `layout_key` 的强绑定
- 和 `structure_version / visibility_version` 的一致性表达
- page split / merge 的一等支持
- page summary 与 page detail 的分层建模

### 3.2 BlockHeightIndex 是全局真相，但页内优化不足

当前 `BlockHeightIndex` 已有：

- 全局 prefix sum
- update_height
- insert_range / delete_range / move_range
- block_at_offset / offset_of_block

但它仍然是全局维度，不是 page-local 索引。

现在的局部索引主要在 `RenderWindow` 内临时构建，意味着：

- 窗口外 page detail 不常驻
- page 局部恢复不够快
- page 级增量修正与持久化闭环还没形成

### 3.3 当前页内索引更像“窗口局部索引”

当前 `RenderWindow.local_height_index` 只服务于当前 render window。

这能保证 UI 局部布局，但还不算完整 page-local index，因为它：

- 不长期驻留
- 不作为 page 级稳定缓存
- 不作为 page summary 的回写来源
- 不支持 page-level refinement 计划

---

## 4. 需要拆解的实施任务

下面按架构分层拆成任务。每个大任务都建议进一步拆为小任务执行。

### 4.1 任务组 A：补齐 PageLayoutIndex 的身份与版本边界

- [ ] 定义 page identity 的最小集合
  - [ ] `document_id`
  - [ ] `structure_version`
  - [ ] `visibility_version`
  - [ ] `layout_key_hash`
  - [ ] `page_policy_version`
  - [ ] `page_index`
- [ ] 检查 `PageLayoutIndex` 当前是否能表达这些 identity
  - [ ] 若不能，新增 `PageLayoutIdentity`
  - [ ] 将 page summary 与 identity 分离
- [ ] 增加 page identity 的一致性校验
  - [ ] cached page 加载时校验版本
  - [ ] 版本不匹配时降级为 Historical 或重建
- [ ] 明确 page summary 是否可跨版本复用
  - [ ] 能复用的条件
  - [ ] 不能复用的条件
  - [ ] 失配后的 fallback 策略

### 4.2 任务组 B：完善 PageLayoutIndex 的分页语义

- [ ] 梳理 `PageLayoutIndex::from_block_estimates` 的切页规则
  - [ ] `max_blocks`
  - [ ] `target_height`
  - [ ] `max_estimated_cost`
  - [ ] `max_text_bytes`
  - [ ] `max_inline_runs`
  - [ ] `max_complex_blocks`
- [ ] 明确 page summary 的字段语义
  - [ ] `block_start`
  - [ ] `block_count`
  - [ ] `height`
  - [ ] `measured_ratio`
  - [ ] `confidence`
  - [ ] `max_error_hint`
  - [ ] `dirty`
- [ ] 增加 page split / merge 的规则文档
  - [ ] 何时 split
  - [ ] 何时 merge
  - [ ] split 后如何传播后缀 page 的 block_start
  - [ ] merge 后如何合并 page summary
- [ ] 明确 page summary 是“聚合缓存”还是“可恢复真相”
  - [ ] 建议定义为可恢复真相，但必须版本化

### 4.3 任务组 C：补齐真正的页内索引

- [ ] 定义 page-local index 的职责
  - [ ] 页内 block 顺序
  - [ ] 页内 prefix sum
  - [ ] 页内 offset lookup
  - [ ] 页内 height update
- [ ] 设计 page-local index 结构体
  - [ ] local block ids
  - [ ] local heights
  - [ ] local confidence
  - [ ] local Fenwick / prefix tree
- [ ] 明确 page-local index 的构建入口
  - [ ] 从 page summary + block layout meta 构建
  - [ ] 从当前 window 局部构建
  - [ ] 从持久化快照恢复
- [ ] 明确 page-local index 的失效条件
  - [ ] structure_version 变化
  - [ ] visibility_version 变化
  - [ ] layout_key 变化
  - [ ] page split / merge
  - [ ] page 内 block 插入删除移动
- [ ] 定义页内索引的精确查询 API
  - [ ] local block index → local offset
  - [ ] local offset → local block index
  - [ ] local block height update
  - [ ] local range total height

### 4.4 任务组 D：建立全局索引与页内索引的协同协议

- [ ] 明确三者职责边界
  - [ ] `DocumentIndex`：顺序与结构真相
  - [ ] `BlockHeightIndex`：全局高度真相
  - [ ] `PageLayoutIndex`：分页聚合真相
  - [ ] `Page-local Index`：局部加速真相
- [ ] 明确哪些更新只改 page-local
  - [ ] 当前页内 block 高度变化
  - [ ] 当前页内 image/table 收敛
  - [ ] 当前页局部 layout 修正
- [ ] 明确哪些更新必须回写 global
  - [ ] block 插入 / 删除 / 移动
  - [ ] fold / unfold
  - [ ] page split / merge
  - [ ] layout key 变化
- [ ] 定义 local→global 的回写流程
  - [ ] 页内变更先更新 local
  - [ ] 再更新 page summary
  - [ ] 再更新 global prefix / page prefix
  - [ ] 最后触发 anchor 修正
- [ ] 定义 global→local 的重建流程
  - [ ] 全局结构变化后，局部索引如何失效
  - [ ] 哪些页可复用历史
  - [ ] 哪些页必须重新 build

### 4.5 任务组 E：补齐分页索引的持久化闭环

- [ ] 设计 page layout 持久化字段
  - [ ] page identity
  - [ ] page block range
  - [ ] page height
  - [ ] measured_ratio
  - [ ] confidence
  - [ ] max_error_hint
  - [ ] dirty
- [ ] 设计 page-local index 持久化策略
  - [ ] 是否全量持久化
  - [ ] 仅持久化当前页/热页
  - [ ] 仅持久化 summary，局部索引运行时重建
- [ ] 定义 layout_key 的版本体系
  - [ ] width bucket
  - [ ] font/theme/scale
  - [ ] structure / visibility version
  - [ ] page policy version
- [ ] 定义缓存失配降级策略
  - [ ] summary 失配
  - [ ] local index 失配
  - [ ] 历史高度可复用但不能当 Exact
- [ ] 定义冷启动恢复流程
  - [ ] 先恢复 summary
  - [ ] 再恢复 current/nearby page local index
  - [ ] 最后后台 refine

### 4.6 任务组 F：修正当前代码中的索引接入点

- [ ] 检查 `crates/cditor-core/src/layout/page_layout.rs`
  - [ ] 是否需要新增 page identity
  - [ ] 是否需要新增 split / merge API
  - [ ] 是否需要新增 cached page 修复入口
- [ ] 检查 `crates/cditor-core/src/layout/height_index.rs`
  - [ ] 是否需要新增 page-local 视图构造器
  - [ ] 是否需要新增局部更新辅助 API
  - [ ] 是否需要新增与 page summary 的同步工具
- [ ] 检查 `crates/cditor-runtime/src/document_runtime/layout_state.rs`
  - [ ] `height_index` 与 `page_layout` 的更新链是否一致
  - [ ] 是否需要引入 page-local cache 字段
  - [ ] 是否需要补充 dirty/range 信息
- [ ] 检查 `crates/cditor-runtime/src/document_runtime/projection.rs`
  - [ ] 渲染窗口是否仍依赖全局局部重建
  - [ ] page-local index 是否可复用到 window-local index
- [ ] 检查测试目录
  - [ ] page layout tests
  - [ ] height index property tests
  - [ ] window projection tests
  - [ ] large window tests

### 4.7 任务组 G：补强测试

- [ ] PageLayoutIndex 单元测试
  - [ ] page 切分正确
  - [ ] page 总高度正确
  - [ ] page offset 映射正确
  - [ ] cached pages 恢复正确
  - [ ] 版本失配时降级正确
- [ ] Page-local index 单元测试
  - [ ] local offset lookup
  - [ ] local height update
  - [ ] local dirty range
  - [ ] local 与 page summary 同步
- [ ] 全局与局部一致性测试
  - [ ] page summary.height 与 local total 一致
  - [ ] global prefix 与 page prefix 一致
  - [ ] page 边界变化后映射正确
- [ ] Property tests
  - [ ] 随机 insert/delete/move 后映射仍正确
  - [ ] 随机 fold/unfold 后 page 与局部索引仍正确
  - [ ] 随机 resize 后高度收敛仍正确
- [ ] 端到端测试
  - [ ] 10w block 打开
  - [ ] 连续滚轮滚动
  - [ ] scrollbar 拖动
  - [ ] 当前页编辑导致高度变化
  - [ ] 跨页 selection 与 copy

---

## 5. 建议的实现顺序

### Phase 1：先把分页身份补齐

目标：让 page 是一个有版本的可恢复对象。

任务：

- [ ] page identity
- [ ] layout key
- [ ] cached page 校验
- [ ] 失配 fallback

### Phase 2：再补页内索引

目标：让 page 内部的定位和修正不必依赖整全局重建。

任务：

- [ ] page-local index
- [ ] local prefix sum
- [ ] local offset lookup
- [ ] local height update

### Phase 3：打通全局与局部协同

目标：更新只动该动的层，不动多余层。

任务：

- [ ] local→page→global 回写
- [ ] global→local 失效重建
- [ ] split / merge

### Phase 4：补持久化闭环

目标：重启后能快速恢复分页和页内定位。

任务：

- [ ] page summary cache
- [ ] page-local cache
- [ ] layout key versioning
- [ ] cold start recover

### Phase 5：补测试与回归门禁

目标：保证大文档场景不抖、不乱、不退化。

任务：

- [ ] 单测
- [ ] property test
- [ ] 端到端测试
- [ ] trace replay

---

## 6. 当前代码的直接改造建议

### 6.1 `crates/cditor-core/src/layout/page_layout.rs`

建议重点改造：

- 新增 page identity 结构
- 新增 page summary / page detail 的区分
- 新增 split / merge 辅助 API
- 新增 cached page 校验入口

### 6.2 `crates/cditor-core/src/layout/height_index.rs`

建议补充：

- page-local 构造器
- 局部范围视图
- 与 page summary 同步的辅助函数
- 更适合局部更新的 API

### 6.3 `crates/cditor-runtime/src/document_runtime/layout_state.rs`

建议补充：

- page-local cache 字段
- page-local dirty 标记
- page 级更新后的 anchor 修正入口
- page summary 与全局索引同步状态

### 6.4 `crates/cditor-runtime/src/document_runtime/projection.rs`

建议仅做接入，不把索引逻辑继续堆在 projection 中：

- current window 直接复用 page-local index
- 当前页与相邻页的布局尽量来自缓存
- 局部重建只针对需要更新的范围

---

## 7. 文档结论

目前的实现可以概括为：

```text
有分页索引基础
有全局高度索引基础
有当前窗口局部索引基础
但还没有完整的 page identity
没有完整的 page-local index 常驻层
没有持久化恢复闭环
没有明确的 split / merge / 局部回写协议
```

因此“完善分页索引与页内索引”的核心不是再加一个新概念，而是把已有的三层索引变成一个完整、版本化、可恢复、可增量修正的体系。

---

## 8. 推荐下一步

建议优先推进：

1. page identity 和 layout key
2. page summary 持久化
3. page-local index 结构体
4. local → global 回写协议
5. split / merge 规则
6. 测试与 trace

这样可以先把架构边界定稳，再逐步把局部优化接上。
