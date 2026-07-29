# 大文档富文本架构实施状态

本文档记录 `large-document-rich-text-architecture.md` 与 `large-document-rich-text-task-list.md` 的当前落地状态、验证结果和已知限制。

## 当前状态

`large-document-rich-text-task-list.md` 中所有任务均已完成并勾选。

已落地的主要模块：

- `core/document`
  - `DocumentIndex`
  - `VisibleDocumentIndex`
- `core/layout`
  - `BlockHeightIndex`
  - `PageLayoutIndex`
  - `LoadedPageLayout`
  - block layout provider
  - complex block editor model
  - complex block inner / outer wheel protocol
- `core/edit`
  - document-level selection
  - text offset map
  - edit transaction
  - undo grouping / large snapshot payload
- `editor/scroll`
  - virtual scroll state
  - global offset mapper
  - wheel accumulator
  - scrollbar drag model
  - height correction pipeline
  - anchor restore pipeline
- `editor/window`
  - render window
  - window planner
  - two-phase window commit
- `editor`
  - hit test model
  - debug overlay
  - trace event log
  - scroll trace replay / regression gate
- `runtime`
  - editing session
  - single-char input hot path
  - IME composition controller
  - main-thread budget arbiter
  - layout scheduler
  - worker pool policy
  - async version controller
  - media cache
  - paste/import pipeline
  - external content security policy
  - document query index
  - final acceptance models
- `storage`
  - storage traits
  - layout cache schemas and recovery policy
  - height write debounce
  - optimistic persistence state machine

## 架构不变量覆盖

当前实现保持以下原则：

1. UI entity 不是文档真相。
2. `DocumentIndex` 是结构真相。
3. `VisibleDocumentIndex` 是可见顺序真相。
4. `BlockHeightIndex` / `PageLayoutIndex` 是高度索引真相。
5. `VirtualScrollState` 是全局滚动真相。
6. `DocumentSelection` 是 selection 真相。
7. 当前编辑 block 会被 pin，且不可 evict。
8. 输入 hot path 不同步执行 SQLite、FTS、全 block shaping、page reflow 或等待异步结果。
9. 异步结果带 generation / version 校验，旧结果可观测并丢弃。
10. complex block 的内部 viewport 与 document viewport 分离。
11. 媒体资源缓存独立于 UI entity cache。
12. 全局查询不依赖 UI entity。

## 验证结果

最近一次验证：

```bash
cargo test
```

结果：

```text
242 passed; 0 failed
```

诊断状态：

- 0 errors
- 1 warning

唯一 warning：

```text
crate `Cditor_V2` should have a snake case name
```

原因是 `Cargo.toml` 中包名为：

```toml
name = "Cditor"
```

该 warning 未处理，因为改包名可能影响 crate 名称与外部引用，除非明确决定迁移包名。

## 当前实现边界

当前代码主要完成架构模型层、调度策略、索引结构、验收模型和测试闭环。以下内容仍属于后续产品化集成工作：

1. 尚未接入真实 GPUI 渲染树。
2. 尚未接入真实 SQLite 读写执行器。
3. `block_fts` 当前提供 schema / 模型化增量索引，未绑定真实 SQLite FTS5 runtime。
4. paste/import pipeline 当前是模型化流水线，未接真实系统剪贴板。
5. media cache 当前是资源策略与 LRU 模型，未接真实图片 decoder。
6. final acceptance 当前通过可重复模型测试和 trace replay 模拟验收，不等同于真实 10 分钟人工 soak。
7. debug overlay 当前输出 view model，未接真实 UI 浮层绘制。

这些边界不违背当前任务清单，因为任务产物要求主要是架构闭环、测试、trace/overlay 指标与验收模型。

## 后续建议

如果继续推进到产品化，建议顺序：

1. 将 `DocumentStore` trait 接入真实 SQLite 实现。
2. 将 `DocumentRuntime` 串联 `DocumentIndex`、`VisibleDocumentIndex`、height/page index、selection、scroll state。
3. 接入 GPUI render window，只渲染当前窗口。
4. 将 debug overlay view model 接真实浮层。
5. 将 trace event log 接入真实滚动、window commit、height correction、async discard 路径。
6. 使用真实 10w block SQLite 数据跑端到端性能基准。
7. 再决定是否迁移 crate 名称以消除 snake_case warning。
