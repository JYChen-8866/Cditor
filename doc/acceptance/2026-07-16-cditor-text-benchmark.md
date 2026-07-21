# cditor-text 性能基准报告（2026-07-16）

> 分支：`codex/parley-text-layout`
>
> Fixture：`text-layout-v1`
>
> Harness：`crates/text/benches/text_layout.rs`

本文记录 P2-018 的可重复基线。它区分“benchmark 已建立”和“所有性能 Gate 已通过”：
focused relayout 与 100 visible surfaces 已满足当前预算；10MiB code 整块布局没有满足输入帧预算，
其结果直接约束后续内部切片与虚拟化设计。

## 1. 环境

| 项目 | 值 |
|---|---|
| 日期/时区 | 2026-07-16 / Asia/Shanghai |
| OS | macOS 27.0（Build 26A5378j） |
| 架构/机型 | arm64 / Mac13,1 |
| CPU | Apple M1 Max |
| 内存 | 64 GiB |
| Rust | rustc 1.95.0（59807616e，LLVM 22.1.2） |
| Cargo | cargo 1.95.0（f2d3ce0bd） |
| Profile | Cargo `bench`；workspace release 配置为 opt-level 3、fat LTO、1 codegen unit |
| 字体 | League Spartan variable `wght` |
| 字体 SHA-256 | `2dbb6290b39ab7c48a40b18f74ca59ef48a69a015c3ea0542703f0c6ce51d617` |

## 2. 方法

Harness 只计量被测操作本身，fixture 构造、字体注册、初始 warm layout 和 cold-cache 清理不进入
对应样本。每组样本按耗时排序，使用 nearest-rank 计算 p50/p95/p99。

运行时同时校验语义，避免“测到了错误路径”：

- focused surface 每次增加 `layout_version` 并改变宽度，结果必须是 `Reflow`。
- 100 visible cold frame 的每个结果必须是 `FullBuild`。
- 100 visible cached frame 的每个结果必须是 `CacheHit`。
- large code 分别直接测量完整 build 和从 immutable shaped snapshot 执行 reflow。
- 所有场景使用同一份 vendored exact font，避免系统字体变化污染趋势。

三档 corpus：

| 模式 | focused samples | cold 100 samples | cached 100 samples | large code |
|---|---:|---:|---:|---:|
| quick | 40 | 2 | 20 | 256KiB |
| standard | 300 | 10 | 200 | 1MiB |
| full | 1000 | 30 | 500 | 10MiB |

## 3. Full Corpus 结果

命令：

```bash
cargo bench -p cditor-text --bench text_layout -- --full
```

| 场景 | samples | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|---:|
| focused relayout | 1000 | 6µs | 10µs | 18µs | 107µs |
| 100 visible cold build | 30 | 2.553ms | 2.778ms | 2.781ms | 2.781ms |
| 100 visible cached frame | 500 | 128µs | 147µs | 177µs | 232µs |
| 10MiB code full build | 3 | 2.451s | 2.543s | 2.543s | 2.543s |
| 10MiB code reflow | 5 | 736.708ms | 746.951ms | 746.951ms | 746.951ms |

## 4. 预算判断

| Gate | 预算 | 结果 | 判断 |
|---|---:|---:|---|
| focused text relayout | p95 < 16ms，且 O(affected surface) | 10µs | 通过当前 microbenchmark |
| 100 cached visible surfaces | p95 < 16.7ms | 147µs | 通过当前 microbenchmark |
| 100 cold visible surfaces | 无独立硬阈值；不得阻塞一帧 | 2.778ms | 当前机器通过一帧预算 |
| 10MiB code 同步 full build | 不得进入输入帧 | 2.543s | 不通过 |
| 10MiB code 同步 reflow | 不得进入输入帧 | 746.951ms | 不通过 |

因此 P2-018 “建立基准”可以完成，但 Gate P2 的整体性能预算不能勾选。当前
`RichBlockKind::Code` 虽标记为 `InternalVirtualized`，App 文本布局仍会把完整字符串交给单个
Parley layout；10MiB 结果证明该路径必须在 Phase 6 实现：

- 分段 text snapshot 与 shaping cache；
- 只为可见 visual-line window 建立布局；
- 局部编辑只 reshape/reflow 受影响 chunk；
- surface 内部 scroll anchor、selection 与 IME geometry 跨 chunk 映射；
- 后台预取和回收受 frame/memory budget 控制。

该工作已登记为 P6-015，完成前禁止宣称 10MiB code 达到输入帧预算。

## 5. 重跑与趋势

```bash
# 开发中快速检查
cargo bench -p cditor-text --bench text_layout -- --quick

# 日常标准基线
cargo bench -p cditor-text --bench text_layout

# 发布/架构 Gate
cargo bench -p cditor-text --bench text_layout -- --full
```

Harness 会把完整报告打印为 versioned JSON，并在 focused p95 超过 16ms，或 100 cached
visible surfaces p95 超过 16.7ms 时返回失败。跨提交比较仍应固定机器、电源、温度和后台负载；
本报告没有覆盖 GPUI paint、glyph raster、GPU 合成、输入事件分发或真实窗口帧时间。
