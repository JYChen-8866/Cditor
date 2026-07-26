# 编辑器重构验收基线（2026-07-16）

> 分支：`codex/parley-text-layout`
>
> 基线提交：`c9b69176504de6f8a9afcaa2b595df2c9a45be06` 加当前未提交重构改动
>
> 目标架构：`doc/architecture/cditor-mature-notion-editor-master-design.md`

本文固化 Phase 0 的可重复能力矩阵、工具链结果和已知缺口。它只记录当前事实，
不把存在类型、历史任务复选框或 ignored test 等同于产品验收完成。

## 1. 运行环境

| 项目 | 值 |
|---|---|
| 日期/时区 | 2026-07-16 / Asia/Shanghai |
| OS | macOS 27.0（Build 26A5378j） |
| 架构/机型 | arm64 / Mac13,1 |
| CPU | Apple M1 Max |
| 内存 | 64 GiB |
| Rust | rustc 1.95.0（59807616e，LLVM 22.1.2） |
| Cargo | cargo 1.95.0（f2d3ce0bd） |
| Profile | Cargo `dev` / test，未使用 release benchmark profile |

## 2. 可重复能力矩阵

| 能力 | 当前状态 | 代码证据 | 自动化证据 | 尚未覆盖 |
|---|---|---|---|---|
| Runtime 会话真相、结构/内容投影 | 已实现 | `crates/cditor-runtime/src/document_runtime/` | `cargo test -p cditor-runtime --lib`，workspace 回归通过 | Local-first、同步和协作仍未接入统一事务协议 |
| 大文档索引、窗口投影、高度模型 | 已实现骨架 | `crates/cditor-core/src/index/`、`crates/cditor-runtime/src/projection/` | Core/Runtime 单测和 workspace 回归通过 | 真实 100k GUI soak、frame benchmark 尚未建立 |
| 文本 shaping、fallback、Bidi、换行 | 已集成 | `crates/cditor-text/src/engine.rs`、`crates/cditor-text/tests/fixtures/text-layout/v1/` | `cargo test -p cditor-text --lib`，38 项通过；versioned multilingual/variable/COLRv1 corpus、Unicode geometry property test 与三档 scale 结构化视觉 golden 通过 | macOS/Windows/Linux 的 GPUI raster screenshot golden 仍属于 GUI/release gate |
| UTF-8/UTF-16/grapheme/shaping cluster 映射 | 已实现 | `crates/cditor-text/src/text_snapshot.rs`、`crates/cditor-text/src/snapshot.rs` | scalar、surrogate pair、CJK、combining、emoji ZWJ、RTL 单测通过 | 平台 IME adapter 全面改用该映射属于 Phase 3 |
| 文本布局缓存 | 已实现策略层并接入 focused surface | `crates/cditor-text/src/cache.rs`、`crates/cditor-editor-gpui/src/editor_view/render.rs` | surface 隔离、LRU、双预算、优先级、pin、Warning/Critical 压力测试通过 | OS memory-pressure 事件桥和 selection/dirty/drag 显式 pin 调用点仍属后续集成 |
| 异步布局结果身份 | 已实现调度契约 | `crates/cditor-core/src/version.rs`、`crates/cditor-runtime/src/scheduling/async_version_control.rs` | 8 项定向测试覆盖九个身份维度、Block/table cell 隔离、分页与 stale hint；Runtime 424 项通过 | 同步 GPUI text snapshot 尚未携带完整文档身份；非布局异步任务按各自 Phase 继续迁移 |
| caret、selection、hit-test、word/line navigation | 已集成，Editor 单一来源 | `crates/cditor-text/src/geometry.rs`、`crates/cditor-editor-gpui/src/text/platform.rs`、`crates/cditor-editor-gpui/src/text/diagnostics.rs` | CJK、combining、emoji ZWJ、RTL/mixed Bidi、soft-wrap affinity、point/index property test、同快照 round-trip 和 caret/selection golden 通过；range bounds 不再生成 synthetic IME bounds，telemetry 区分 snapshot/sync fallback/unavailable | 跨平台 GPUI screenshot、实机 IME 候选框矩阵与正常输入 soak 的 fallback-rate=0 验收尚未完成 |
| Parley exact font/glyph bridge | 已集成 | `crates/cditor-text/src/font_identity.rs`、`crates/cditor-text/src/paint_plan.rs`、`crates/cditor-editor-gpui/src/text/layout_adapter/paint.rs`、`crates/cditor-editor-gpui/src/text/layout_adapter/exact_raster.rs` | 17 项 adapter 测试覆盖 GPUI exact-candidate 路由、glyph mismatch fallback、TTC face-1、variable coords、faux skew/bold、COLRv1 chromatic pixels、subpixel、双预算 LRU 与失败身份；实机小文档截图检查通过 | GPUI 原生 glyph atlas 仍不能表达完整 instance，当前用 GPUI image sprite atlas 承载 exact raster；OT-SVG 尚需专用 renderer，跨平台 screenshot gate 未完成 |
| 普通 Block 与 table cell 共用文本 element | 已集成 | `crates/cditor-editor-gpui/src/text/element.rs`、`crates/cditor-editor-gpui/src/features/table/text.rs` | `cargo test -p cditor-editor-gpui --lib`；paragraph/heading/list/code/cell 共享 pipeline 测试通过 | Code 内部行虚拟化、caption/collection surface 未统一 |
| IME preview/commit 与 handler identity | 部分完成 | `crates/cditor-runtime/src/editing/session.rs`、`crates/cditor-runtime/src/document_runtime/`、`crates/cditor-editor-gpui/src/input/ime/`、`crates/cditor-editor-gpui/src/text/platform.rs` | session/target/composition/content identity、selected range 单一 caret 真相、反向 focus 投影、printable input 单一平台通道、显式 focus/surface commit policy、candidate rect 的 exact surface/content/layout cache identity、同 target refocus stale handler、UTF range、Block/cell preview/commit/cancel、cancel 恢复 base range/reversed/affinity/document selection、`unmark_text` 单步 commit/undo 测试通过 | caption/collection、remote rebase、zoom/scroll/font epoch 和 macOS/Windows/Linux 实机矩阵未完成 |
| inline box | layout/renderer contract 已实现 | `crates/cditor-text/src/engine.rs`、`crates/cditor-text/src/snapshot.rs`、`crates/cditor-editor-gpui/src/text/element.rs` | box 参与布局和 cache identity，snapshot 保留 id/kind/geometry，Editor renderer hook 测试通过 | mention/equation/date/user 的 payload token、atomic editing、clipboard、持久化与协作未实现 |
| Accessibility 文本投影 | adapter 已实现 | `crates/cditor-text/src/accessibility.rs` | focused snapshot/selection projection 测试通过 | 当前 GPUI 未暴露 OS tree update 注入 API |
| 文本布局性能基线 | benchmark harness 已建立 | `crates/cditor-text/benches/text_layout.rs`、`doc/acceptance/2026-07-16-cditor-text-benchmark.md` | full corpus：focused reflow p95 10µs、100 cached surfaces p95 147µs；命令内置预算失败退出 | 10MiB code full build p95 2.543s、reflow p95 746.951ms，必须内部切片/虚拟化 |
| PostgreSQL 存储 | 单测通过，集成测试需外部服务 | `crates/cditor-storage-postgres/` | 非 ignored workspace 测试通过 | 55 项 Docker/PostgreSQL 测试默认 ignored；生产目标将改为同步服务端边界 |
| SQLite 本地存储 | crate/基础能力存在 | `crates/cditor-storage-sqlite/` | workspace 回归通过 | 目标 local-first journal/outbox/crash recovery Gate 未完成 |

## 3. 自动化命令基线

以下命令在上述机器和当前工作树执行：

| 命令 | 结果 |
|---|---|
| `./scripts/dev/check_structure.sh` | 通过；验证依赖方向、Parley 唯一直接消费者、caret 单一真相和 700 行上限 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace` | 通过 |
| `cargo test -p cditor-text --lib` | 通过：38 passed |
| `cargo bench -p cditor-text --bench text_layout -- --full` | 通过 harness 预算；large-code 性能未达输入帧预算，详见独立报告 |
| `cargo test -p cditor-runtime --lib` | 通过：424 passed |
| `cargo test -p cditor-app --lib` | 通过：365 passed，1 ignored |
| `cargo test --workspace` | 通过；所有非 ignored 测试通过，57 ignored |
| `cargo clippy -p cditor-text --lib --tests --no-deps -- -D warnings` | 通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 未通过；首先被 `cditor-core` 的 25 项存量 lint 阻断 |
| `cargo clippy -p cditor-runtime --lib --tests --no-deps -- -D warnings` | 未通过；14 项存量 lint，`async_version_control.rs` 与本次 composition/session 文件无报错 |
| `cargo clippy -p cditor-app --lib --tests --no-deps -- -D warnings` | 未通过；App 存量 lint 为 lib 78 项、lib test 82 项，本次新增平台几何文件无报错 |

Clippy 失败不是本次 `cditor-text` 拆分或 P2-013 身份校验引入的编译回归，但它会继续阻止
Phase 0 Gate，不得把“已记录基线”描述为“workspace strict Clippy 已通过”。当前主要类别包括
`large_enum_variant`、`too_many_arguments`、`collapsible_if`、`let_unit_value`、
`manual_clamp`、`manual_strip`、`needless_range_loop` 和 `ptr_arg`。

## 4. Ignored Test 清单

源码中共有 57 个显式 ignored test：

| 分类 | 数量 | 原因 |
|---|---:|---|
| `cditor-storage-postgres` | 55 | 需要 `docker compose postgres_test` 和 `CDITOR_TEST_DATABASE_URL`；其中包含 100k 数据测试 |
| `cditor-app` cold start | 1 | 需要同一 PostgreSQL/Docker 环境 |
| `cditor-core` 100k demo | 1 | 构造完整 100k Block demo，默认测试不执行 |

复核命令：

```bash
rg -n '#\[ignore' crates -g '*.rs'
```

## 5. Gate 判断

- P0-005：完成。当前分支能力、代码位置、测试入口和缺口已形成可重复矩阵。
- P0-006：完成。fmt/check/clippy/test 结果和 ignored test 已固化；这不表示 Clippy 已清零。
- Gate P0：未通过。还缺版本化 fixture、正式 benchmark/报告、telemetry schema、模板，
  并且 workspace strict Clippy 仍失败。
- Gate P2：未通过。P2-009/P2-010 已通过 exact raster image-atlas bridge 闭环，
  P2-017 结构化视觉回归和 P2-018 benchmark harness 也已完成；但 full benchmark 证明
  10MiB 整块 layout 未达预算。内部 text-surface 虚拟化、OT-SVG renderer、跨平台 GPUI
  raster screenshot、同步 GPUI text snapshot 的完整文档身份与正常输入 fallback-rate
  telemetry 仍属于后续集成边界。
- Gate P3：未通过。P3-001/002/003/004/005/006/008/009/010/011/014/015 已由实现和自动化覆盖；
  remote composition rebase、caption/collection surface、三平台人工矩阵和 IME preview
  性能预算。
