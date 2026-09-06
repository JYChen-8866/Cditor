# Cditor 工程结构索引

本文只记录 workspace 当前结构和结构门禁，不再维护一套与执行方案重复的目标目录。
长期目标、crate 责任、依赖拓扑和逐项迁移任务以
[重构方案 0722](重构方案%200722.md)为唯一依据；产品能力以
[成熟 Notion 类编辑器总体设计](cditor-mature-notion-editor-master-design.md)为准；10 万
Block 的性能约束以[大文档富文本架构](../large-document-rich-text-architecture.md)为准。

## 当前结构

```text
crates/
  cditor-editor-protocol/      Command、query、projection 和 event 协议
  cditor-core/                 纯文档模型和领域不变量
  cditor-theme/                theme token、resolver、typography、metrics
  cditor-text/                 Parley 私有实现和框架无关文本 API
  cditor-viewport/             虚拟滚动、窗口、anchor、hit-test
  cditor-runtime/              当前活文档真相和 command/query 执行
  cditor-session/              Runtime 串行所有权和应用服务编排
  cditor-storage/              存储 port 和通用 DTO
  cditor-storage-sqlite/       SQLite adapter
  cditor-storage-postgres/     PostgreSQL adapter
  cditor-import-export/        外部格式解析、ImportPlan、安全限制
  cditor-ai/                   AI provider contract 和 mock
  cditor-ai-openai/            OpenAI-compatible HTTP adapter
  cditor-sdk/                  无框架 SDK options/command/event/provider
  cditor-editor-gpui/          GPUI view、input、overlay 和 adapter
  cditor-test-support/         fixture、acceptance 和 benchmark 支撑
apps/
  cditor-desktop/              desktop composition root
components/
  cditor-component/            共享 GPUI presentation component
  cditor-whiteboard/           独立白板产品组件
```

目录叶子、Cargo package 和 Rust crate 名称保持一一对应；Desktop 位于 `apps/`，可复用
组件位于 `components/`，其余稳定层和 adapter 位于 `crates/`。

## 当前依赖原则

- `cditor-core` 不依赖 GPUI、Parley、SQLx、网络、SDK 或本地化呈现。
- `cditor-text` 是 Parley 的唯一直接消费者，不依赖 GPUI。
- `cditor-editor-protocol` 是 Command、query、projection 和 event 的稳定协议边界。
- `cditor-viewport` 只保存框架无关的窗口、滚动、anchor 和 hit-test 算法。
- `cditor-runtime` 不依赖 GPUI、具体 Storage adapter、SQLx 或 OpenAI。
- `cditor-storage` 只定义 port/DTO/error，不依赖 Runtime、Editor 或具体 adapter。
- `cditor-session` 串行拥有 Runtime，并负责任务、持久化、import 和 AI 应用服务。
- GPUI View 不是文档真相，只消费 Session projection/event 并发出 Command。
- Desktop 是最终 composition root，具体数据库、AI 和平台实现只在此装配。
- `cditor-whiteboard` 独立演进；编辑器只通过版本化 payload/projection 适配它。

## 目录和源码门禁

- Cargo package 使用 `cditor-<domain>` 或 `cditor-<domain>-<adapter>`；目录叶子与
  package 名相同。
- 所有 Rust 文件不超过 700 行；白板不再享有永久豁免。
- 同一功能的状态、行为、投影和测试放在同一模块树，不以超大 façade 文件聚合实现。
- 历史计划进入 `doc/archive/`；当前文档不得把历史路径描述为现状。
- 一次性脚本进入 `scripts/archive/`；持续入口按 `dev/`、`database/`、`packaging/` 分类。
- 根目录只保留 workspace、许可证、配置入口和顶层说明。

`scripts/dev/check_structure.sh` 强制执行命名、文件规模、Core/Runtime/Storage/Viewport、
GPUI 和 Parley 边界。每个阶段至少执行：

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
./scripts/dev/check_structure.sh
./scripts/dev/check_dependency_graph.sh
```

机器生成的 workspace 依赖快照位于 `dependency-graph-v2.txt`。

涉及 Runtime、Viewport、Text 或 Session 热路径时，还必须运行对应 benchmark，并与
`doc/acceptance/2026-07-22-refactor-architecture-baseline.md` 比较 p95/max、resident
payload、layout cache 和 undo memory。
