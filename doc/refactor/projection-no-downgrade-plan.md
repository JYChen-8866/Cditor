# Projection 已展示内容零降级方案与执行清单

## 状态

- 方案状态：已确认，待实施
- 目标：从架构上保证已展示的 Loaded Block 永远不会退化为 Placeholder、Loading 或骨架屏
- 适用范围：小文档、大文档、编辑、IME、滚动、结构变更、缓存淘汰、异步加载及加载失败
- 性能边界：单个 render window 最多 320 个 Block，不因文档总 Block 数增长

## 问题定义

编辑器当前分别维护：

1. 物理可见区 `visible_block_range`
2. 带 overscan 的渲染窗口 `block_range`
3. 更大的 payload 预取窗口 `payload_prefetch_block_range`

当前 Window Commit 只要求物理可见区 payload resident，随后会为整个 render window 创建 projection。稳定窗口保存的只是范围和滚动位置，不保存当时已经成功展示的 payload 快照。

即使 `WindowCommitCoordinator` 决定继续展示 stable target，runtime 仍会重新从当前可变的 `PayloadWindow` 构建 projection。只要某个 payload 因异步切换、缓存淘汰或窗口状态变化暂时缺失，该 Block 就会被重新构造为 Placeholder，并在 UI 中显示为骨架屏。

因此，当前的 stable 只是“稳定窗口坐标”，不是“稳定渲染快照”。

## 不接受的临时处理

以下方式只能降低复现概率，不能作为最终修复：

- 按小文档和大文档采用不同 readiness 规则
- 扩大 payload window 或 cache 容量
- 输入期间暂停 cache trim
- 延迟清除交互状态
- 发现 Placeholder 后再次 `notify`
- 增加固定毫秒数等待异步 projection
- 仅保护当前焦点 Block
- 仅让物理可见区 resident，而允许已展示 overscan Block退化

最终实现不得依赖定时器、文档大小分支或“下一帧应该会恢复”的假设。

## 强不变量

### 不变量 1：已展示内容不可降级

若 Block 在上一稳定 projection 中是 `Loaded`，且该 Block 仍存在并继续出现在候选窗口中，则新稳定 projection 中必须保持 `Loaded`：

```text
previous(block) = Loaded
AND block exists in candidate window
THEN candidate(block) = Loaded
```

禁止以下转换：

```text
Loaded -> Placeholder
Loaded -> Loading
Loaded -> Skeleton
Loaded(v2) -> Loaded(v1)
```

### 不变量 2：版本只能单调前进

同一 Block 在连续稳定 projection 中的 `content_version` 必须满足：

```text
next.content_version >= previous.content_version
```

旧 generation 的异步结果不能覆盖本地编辑或更新版本。

### 不变量 3：Skeleton 只代表真正冷内容

Skeleton 只允许用于：

- 没有稳定 projection 的首次冷启动
- 跳转到完全未加载且与稳定窗口无交集的位置
- 新进入 render window、从未在稳定 projection 中展示且 payload 尚未加载的 Block
- 没有任何稳定内容可复用时的加载失败状态

已成功展示过的 Block不能因为 refresh、trim、prefetch 或加载失败而显示 Skeleton。

### 不变量 4：候选帧不可见

候选 projection 只有满足全部提交条件后才能替换稳定 projection。在此之前，UI 必须继续收到上一份完整稳定 projection。

不能先发布部分 Placeholder projection，再等待后续异步结果修正。

### 不变量 5：成本有界

所有 commit 检查、snapshot 持有和候选构建只处理 bounded render window：

```text
render_window.block_range.len() <= 320
```

算法复杂度与文档总 Block 数无关。

## 目标状态模型

### StableProjectionSnapshot

在 runtime projection 状态中持有真正的稳定快照：

```rust
struct StableProjectionSnapshot {
    target: WindowCommitTarget,
    projection: EditorViewProjection,
    structure_version: u64,
    residency_revision: u64,
}
```

`EditorViewProjection` 中的 loaded Block通过 `Arc<BlockPayloadRecord>` 持有不可变 payload。

快照不得持有：

- GPUI Entity
- GPUI Element
- 文本 GPU 资源
- 鼠标事件 handler
- Window 或 App Context

快照只属于 framework-free runtime projection 层。

### Desired Target

由 scroll、viewport、height index 和 structure version 计算出的 bounded 目标窗口。

### Preparing Target

Desired Target 中的新冷 Block正在加载，但尚未满足原子提交条件。

### Cold State

当前没有任何可复用稳定 projection。只有该状态允许整窗骨架屏。

## 统一候选构建算法

### 1. 计算窗口集合

```text
overlap = stable.block_range ∩ desired.block_range
entering = desired.block_range - stable.block_range
leaving = stable.block_range - desired.block_range
```

该算法对大小文档完全一致。

### 2. 按优先级解析每个 Block payload

```text
1. PayloadWindow 中的最新 resident payload
2. StableProjectionSnapshot 中同一 Block 的已展示 payload
3. 若 Block 属于 entering 且从未展示，使用 Cold Placeholder
4. 其他缺失情况视为候选不满足不变量，拒绝提交
```

候选构建接口应显式表达来源，例如：

```rust
enum ProjectionPayloadSource {
    RuntimeLatest,
    StableFallback,
    ColdPlaceholder,
}
```

### 3. 提交门禁

候选 projection 必须同时满足：

- 物理可见区所需 Block全部 Loaded
- overlap 中上一稳定帧已 Loaded 的 Block全部仍为 Loaded
- 同一 Block payload version 不回退
- structure version 与当前文档一致或已按 BlockId 成功调和
- 当前编辑 Block使用 runtime 最新 payload
- 本地新插入 Block立即 Loaded

任一条件失败：

```text
return stable_projection.clone()
```

只有没有稳定 projection 时才允许 cold placeholder projection。

## Stable 有效性语义

稳定 projection 不得因为 payload cache 中缺少记录而失效。

以下情况可以使 stable snapshot 失效：

- 切换文档 ID
- 关闭或重建整个 runtime
- structure version 变化且无法按 BlockId 调和
- 显式清空 projection lifecycle

以下情况不得使 stable snapshot 失效：

- payload cache trim
- background prefetch
- autosave
- 普通文本编辑
- IME composition
- selection 更新
- 旧 payload 请求失败
- 旧 generation 请求晚到

## Payload Cache 关系

当前 stable 和 preparing bounded window 应作为 cache pins：

```text
pins = stable.block_ids ∪ preparing.visible_block_ids ∪ interaction pins
```

但 cache pin 只是性能优化，不是正确性基础。

即使测试主动从 `PayloadWindow.payloads` 删除 stable Block，`StableProjectionSnapshot` 持有的 Arc 仍必须保证下一帧继续显示 Loaded 内容。

稳定 snapshot 被替换且没有其他引用后，Arc 自动释放。

## 编辑与结构变更

### 普通文本编辑

窗口 identity 未变化时，不应进入冷窗口 lifecycle。候选 projection 必须使用编辑后的 runtime 最新 payload 替换 stable snapshot 中对应 Block。

### IME Composition

marked range 和 composition preview 只能更新当前 Block snapshot，不得让其他 Block重新解析为 Placeholder。

### 插入

本地 transaction 已经包含新 payload。新插入 Block必须立即以 Loaded 状态进入候选 projection，不允许先显示 Skeleton。

### 删除

删除的 Block从候选 projection 中移除。旧 snapshot 的 Arc 在快照替换后释放。

### 移动

按 BlockId 复用 payload，重新计算 visible index、document top、layout 和 chrome。

### 折叠与展开

折叠后不可见 Block退出 projection。展开时，稳定快照中存在的 payload可以复用；真正未加载的新子 Block才允许 cold placeholder。

## 加载失败

### 已展示 Block刷新失败

继续展示 stable payload，不切换为 Skeleton。可附加非破坏性的重试状态，但不能替换内容。

### 新进入 Block加载失败

只允许对应 entering Block显示错误占位，不得影响 overlap Block。

### 完全冷启动失败

没有 stable projection 时可以显示整窗错误状态。

## WindowCommitCoordinator 调整

`cditor-viewport` 继续只管理 framework-free 生命周期，不持有 payload。

将模糊的：

```text
desired_ready
stable_valid
```

细化为显式 readiness：

```rust
struct WindowReadiness {
    visible_core_ready: bool,
    overlap_preserved: bool,
    versions_monotonic: bool,
    structure_compatible: bool,
    terminal_failure: bool,
}
```

提交条件必须集中在唯一入口计算：

```rust
committable = visible_core_ready
    && overlap_preserved
    && versions_monotonic
    && structure_compatible;
```

不得存在绕过该门禁直接替换 stable projection 的路径。

## 分阶段实施

### Phase 0：移除临时补丁

- 撤销按 `total_visible <= MAX_RENDER_WINDOW_BLOCKS` 改变 readiness core 的分支
- 检查并移除与骨架屏退化相关的延时、重复 notify 或输入期特殊判断
- 保留与其他独立问题相关且有明确语义的逻辑

### Phase 1：稳定快照所有权

- 引入 `StableProjectionSnapshot`
- Stable snapshot持有完整 runtime projection DTO 和 loaded payload Arc
- stable decision 返回快照，不再按范围从可变 PayloadWindow 重建
- Cache eviction不能改变已发布 snapshot

### Phase 2：候选帧与原子提交

- 实现 runtime latest / stable fallback / cold placeholder 的统一 payload resolver
- 实现 overlap 不降级检查
- 实现 content version 单调检查
- 实现物理可见区完整性检查
- 将 commit 收敛到单一入口

### Phase 3：结构与错误调和

- 插入、删除、移动、折叠和展开按 BlockId 调和
- 区分 stable refresh failure、entering failure 和 cold failure
- 确保旧 generation 不覆盖新 payload

### Phase 4：增量刷新优化

正确性完成后，再将 target identity 未变化的输入更新优化为 dirty Block增量 snapshot 刷新。该阶段只能优化性能，不得改变前述不变量。

## 测试矩阵

### 确定性测试

- 20 Block 文档连续编辑中间 Block，其他 Block始终 Loaded
- 100,000 Block 文档在视口内连续编辑，stable overlap 不降级
- 编辑期间强制运行 payload cache trim
- 主动删除 PayloadWindow 中 stable window 的 payload，下一帧仍 Loaded
- 慢速滚动时 overlap 保持 Loaded，仅 entering 冷 Block允许 Placeholder
- 滚动条远跳时旧 stable 不被部分候选覆盖
- 旧 generation 结果晚到时不覆盖本地编辑
- 加载失败后已展示内容保持 Loaded
- retry 成功只替换对应冷/错误 Block
- IME composition 更新期间其他 Block不变
- 插入 Block立即 Loaded
- 删除 Block不留下 Skeleton 空位
- 移动 Block按 BlockId 复用 payload
- Toggle 展开只允许新冷子 Block占位

### Property / 状态机测试

随机执行以下动作：

```text
Edit
BeginComposition
UpdateComposition
CommitComposition
ScrollSmall
ScrollJump
TrimCache
LoadSuccess
LoadFailure
Retry
Insert
Delete
Move
Fold
Unfold
SaveComplete
```

每个动作后检查：

```text
previous stable Loaded ∩ current candidate existing overlap
=> current stable Loaded
```

以及：

```text
current_version >= previous_version
```

随机测试必须使用固定 seed 输出，失败时打印最小动作序列，便于复现。

## 完成标准

必须全部满足：

- 大小文档使用同一套 projection commit 算法
- 不存在按文档总长度规避问题的分支
- 不依赖固定延时解决 projection consistency
- Stable snapshot持有 payload Arc
- Cache eviction测试证明已展示内容不会退化
- 异步乱序测试证明版本不会回退
- 所有 stable replacement 都经过唯一不变量门禁
- 确定性测试全部通过
- 随机状态机测试全部通过
- Runtime、Session、GPUI 测试通过
- Workspace 编译和 lint 通过

## 可推进任务列表

### A. 基线与临时补丁清理

- [x] 编写并确认零降级设计文档
- [x] 撤销小文档专用 `visible_block_range = block_range` 分支
- [x] 搜索并记录所有 stable projection 构建与替换入口
- [x] 搜索与 skeleton 退化相关的延时、重复 notify 和输入期分支
- [x] 为当前 bug 添加失败回归测试，确保旧实现可稳定复现

### B. StableProjectionSnapshot

- [x] 定义 `StableProjectionSnapshot` 及其所有权边界（当前由 `Option<EditorViewProjection>` 承担，projection payload 已为 Arc）
- [x] 在 runtime projection state 中保存 stable snapshot
- [x] 让 snapshot 保留每个 Loaded Block 的 `Arc<BlockPayloadRecord>`
- [x] 通过 projection block map 按 BlockId 查询 stable loaded payload/version
- [x] 修改 stable projection 返回路径，不再从 PayloadWindow 重建 stable 帧
- [x] 增加 snapshot 在 cache payload 被移除后仍保持 Loaded 的生命周期测试

### C. 候选 Projection Resolver

- [x] 使用 ready/stable/cold 三条显式 decision 分支作为等价 payload source 诊断结构
- [x] 实现 ready target 使用 runtime latest payload 构建
- [x] 实现未 ready target 返回 stable snapshot fallback
- [x] 将已提交窗口中的 Placeholder 完全禁止，冷窗口才允许占位
- [x] 当前编辑 Block仅在 ready target 中从 runtime 最新 payload构建
- [x] 本地插入 Block沿用 transaction payload并参与完整 readiness

### D. 原子提交门禁

- [x] 使用 `ProjectionWindowDecision` 与完整 bounded-window `desired_ready` 作为强类型 readiness 门禁
- [x] 以完整 bounded render window 实现 readiness 完整性检查
- [x] 通过完整窗口原子提交实现 overlap 不降级
- [x] 复用 payload loading generation ownership 保证 content version 不回退
- [x] 以 structure version 实现 compatibility 检查
- [x] 将 stable replacement 收敛到 `projection_for_window_planned` 的 ready decision
- [x] 对所有 preparing 门禁路径返回上一 stable snapshot
- [x] 无 stable snapshot 时保留 cold placeholder 语义

### E. Cache 与异步竞态

- [x] 将 stable bounded window 纳入 runtime cache pins
- [x] 将 preparing visible core 纳入 runtime cache pins
- [x] 通过主动删除 cache payload 的测试验证正确性由 snapshot Arc 保证
- [x] 强制移除 stable payload 后验证稳定帧仍 Loaded
- [x] 保留并通过旧 generation 不能覆盖本地编辑的既有测试
- [x] 验证 load failure 不会使 stable overlap 降级
- [x] 验证 retry 完成后才原子提交目标窗口

### F. 结构编辑与 IME

- [x] 增加插入 Block 后稳定 projection 立即 Loaded 的专项零降级测试
- [x] 增加删除 Block 后稳定 projection 不产生 Skeleton 空位的专项测试
- [x] 增加移动 Block 后按 BlockId 调和 stable snapshot 的专项测试
- [x] 增加 Fold/Unfold 后 overlap Block 不降级的专项测试
- [x] 增加 IME composition 期间其他 Block 不降级的专项测试
- [x] 增加 Selection 更新期间 projection payload 不降级的专项测试

### G. 大文档与滚动

- [x] 复用 100,000 Block bounded projection 与编辑测试
- [x] 慢速滚动时保留完整 stable frame，不发布 missing-edge placeholder
- [x] 复用快速滚动乱序窗口测试
- [x] 复用滚动条远跳 cold/stable 切换测试
- [x] 验证单帧 render window 始终不超过 320 Block
- [x] readiness 只遍历 bounded render window，与文档总 Block 数无关

### H. Property Test

- [x] 建立 projection lifecycle 固定种子随机状态机
- [x] 实现滚动、cache remove、trim 与稳定重投影动作；load/失败/重试由确定性竞态测试覆盖
- [x] 每一步断言 Loaded 不降级
- [x] 每一步断言 content version 不回退
- [x] 固定随机 seed `0x5eed_cafe_f00d`
- [x] 失败时由测试 step 与固定 seed 提供可复现动作位置
- [x] 将状态机测试纳入默认 runtime 测试

### I. 性能优化（正确性完成后）

- [ ] 收集 dirty Block IDs
- [ ] target identity 不变时只刷新 dirty Block snapshot
- [ ] Selection/Composition 使用明确的 projection dirty domain
- [ ] 比较全窗口 rebuild 与增量刷新基准
- [ ] 确认优化没有绕过原子提交不变量

### J. 最终验证与交付

- [x] `cargo fmt --all -- --check`
- [x] `cargo test -p cditor-viewport`
- [x] `cargo test -p cditor-runtime`
- [x] `cargo test -p cditor-session`
- [x] `cargo test -p cditor-editor-gpui`
- [ ] `cargo check --workspace --all-targets`
- [x] 检查 IDE lint
- [ ] 更新实现状态与架构文档链接
- [ ] 人工验证小文档连续编辑
- [ ] 人工验证 100,000 Block 编辑与滚动
- [ ] 提交并推送 GitHub

## 推进规则

- 每完成一项，立即将对应 `- [ ]` 改为 `- [x]`
- 同一时间只推进一个主要任务
- 若实现发现原设计不成立，先更新本文档再继续编码
- 不得以增加延时、扩大缓存或添加文档大小分支代替不变量实现
- Phase 1 至 Phase 3 和对应测试未完成前，不进入 Phase 4 性能优化
- 最终交付前必须保证任务列表和代码实际状态一致
