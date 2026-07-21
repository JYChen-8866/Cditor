# Frame Benchmark 基线报告（2026-07-17）

> 任务：P0-008
>
> Harness：`crates/runtime/benches/frame_baseline.rs`（`frame-baseline-v1`，
> 无外部 benchmark framework，bench profile）
>
> 运行：`cargo bench -p cditor-runtime --bench frame_baseline -- --full`
>
> 报告 JSON：`target/benchmark-reports/frame-baseline-full.json`（含本表全部数据）

## 1. 环境

| 项 | 值 |
|---|---|
| 日期/时区 | 2026-07-17 / Asia/Shanghai |
| OS | macOS 27.0（Build 26A5378j），target_os=macos |
| 架构/机型 | arm64 / Mac13,1（Apple M1 Max，10 逻辑核） |
| 内存 | 64 GiB |
| Rust/Cargo | 1.95.0 |
| Profile | Cargo `bench` |
| 模式 | `--full`（open 12 次/夹具，场景 3 次取最差） |

## 2. Fixture manifest（P0-007 版本化语料）

| fixture | version | blocks | semantic checksum |
|---|---|---:|---|
| mixed | 1 | 100000 | 9388435979756894105 |
| bidi-stress | 1 | 4096 | 17590547867001496452 |
| large-code（10 MiB 单块） | 1 | 1 | 6830128722496044695 |
| tall-table（50k 行） | 1 | 1 | 11541031340359534021 |
| wide-table（500 列） | 1 | 1 | 2277308060352881936 |

checksum 由 `cditor_core::fixtures::document_semantic_checksum`（FNV-1a over
文档语义 JSON）产生；生成器语义改动必须提升版本并更新本表。

## 3. 结果（headless Runtime acceptance 场景）

### Open（first screen，p95 ms，12 次迭代）

| fixture | p95 | 内置 Gate |
|---|---:|---|
| 100k-one-line | 181.0 | 通过 |
| 100k-uneven-heights | 187.5 | 通过 |
| image-dense | 156.6 | 通过 |
| 10mb-code-block | 41.3 | 通过 |
| 50k-row-table | 41.3 | 通过 |
| emoji-cjk-bidi | 202.1 | 通过 |

### Scroll（模拟帧轨迹，最差 p99 frame ms，3 次）

TopToMiddle 6.0 / MiddleToTop 6.0 / TenMinuteContinuousScroll 5.0 /
RandomHeightCorrection 7.0 / WindowLoadDelay 8.0 / ScrollbarDrag 8.0 —— 全部通过
（p99 ≤ 16ms、anchor jitter p95 ≤ 1px）。

### Editing（最差 latency ms，3 次）

| 场景 | p95 | p99 |
|---|---:|---:|
| ContinuousInput1000Chars | 3.2 | 3.2 |
| InputCausesMultipleLineWraps | 3.2 | 3.2 |
| ImeComposition | 3.0 | 4.0 |
| TypingWhileScrolling | 4.0 | 4.0 |
| TypingWhileResize | 5.0 | 7.0 |

全部通过（p95 ≤ 8ms、p99 ≤ 16ms、caret drift ≤ 1px）。

### Structure edit（最差 UI blocking ms，3 次）

Paste10k 5.5 / Delete50k 11.5 / UndoLargeDelete 12.0 / Move10kSubtree 5.5 /
CollapseExpand10k 10.0 —— 全部通过（≤ 16ms、rebuild passes ≤ 2）。

## 4. 边界与不作声明

- 本基线测的是 Runtime headless 场景（index/height/page/window/编辑事务的
  模拟帧），**不是** GPUI raster 帧；GUI 帧预算（首帧 p95 < 250ms 等第 28 节
  条目）仍需 GUI soak/screenshot gate 覆盖后才可勾选 Gate P6。
- 文本布局微基准见独立报告
  `doc/acceptance/2026-07-16-cditor-text-benchmark.md`；其 large-code 未达
  预算的结论不受本报告影响。
- 任一场景未通过内置预算时 harness 以非零码退出，可直接接入 CI。
