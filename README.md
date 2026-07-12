# Cditor

Cditor 是基于 Rust 与 GPUI 构建的大文档富文本编辑器，目标是支持 10 万级 Block、复杂富文本、跨页 selection、稳定虚拟滚动和 PostgreSQL 持久化。

项目目前处于持续开发阶段。核心架构、运行时、表格、Markdown、IME、剪贴板、媒体、Mermaid、白板嵌入、Inline AI 和 PostgreSQL 存储均已有实现与测试；生产能力和性能验收应以 [大文档架构](doc/large-document-rich-text-architecture.md)、[实现状态](doc/large-document-rich-text-implementation-status.md)及相应验收文档为准。

## 核心能力

- 10 万级 Block 的轻量索引、分页高度模型和窗口化渲染。
- 独立于 UI Entity 的文档状态、selection、layout height 和 virtual scroll 状态。
- Paragraph、Heading、Quote、Callout、Todo、列表、Toggle、Code、Table、Image、File、Mermaid、Whiteboard、Embed、Database 等 Block 类型。
- 富文本 marks、Markdown 导入导出与增量快捷输入。
- 跨 Block 剪贴板、结构编辑、撤销重做和持久化事务。
- CJK、Emoji、UTF-8/UTF-16 offset、IME composition 和候选框定位。
- 表格单元格编辑、选区、复制粘贴、合并拆分、resize、reorder 和横向滚动。
- PostgreSQL 文档、payload、layout cache、FTS、资产、事务、恢复队列和同步 outbox。
- Inline AI 流式预览与替换。
- Mermaid 渲染与独立的 ding-board 白板集成。
- 大文档、滚动稳定性、输入延迟和窗口投影相关的回归测试。

## 架构原则

本项目最重要的约束是：

> UI 只是当前窗口的投影；真实文档、selection、layout height 和 scroll 状态必须存在于编辑器内核中。

因此：

- GPUI Entity 可以随着虚拟窗口创建和销毁，文档状态不能依赖 Entity 生命周期。
- Copy、Cut、Paste、Undo、Redo 和跨页 selection 读取内核数据，不读取当前 UI 树。
- 当前 viewport 附近追求精确布局，远端未测量内容允许估算并在加载后收敛。
- 连续滚动和当前视口稳定优先于远端全局高度的即时绝对精确。
- 输入热路径不能同步等待 PostgreSQL、全量 payload、全局 layout 或后台索引。

完整设计见 [10 万 Block 大文档架构](doc/large-document-rich-text-architecture.md)。

## 工程目录

```text
.
├── Cargo.toml                   # Workspace 成员、统一版本、edition 和 license
├── Cargo.lock                   # Workspace 唯一依赖锁文件
├── README.md                    # 项目入口
├── .env.example                 # 本地环境变量示例，不包含真实密钥
├── docker-compose.yml           # 开发与测试 PostgreSQL
├── assets/                      # Cditor 应用共享静态资源
├── config/                      # 可提交的非敏感运行配置
├── crates/
│   ├── core/                    # 文档内核、富文本、事务、selection、布局模型
│   ├── editor/                  # 无 GPUI 的视口、滚动、窗口规划和命中测试算法
│   ├── runtime/                 # 活文档状态、编辑编排、projection 和调度
│   ├── store/                   # 存储抽象、缓存策略和持久化状态机
│   ├── store-postgres/          # PostgreSQL 实现、迁移与集成测试
│   ├── app/                     # GPUI 应用、渲染、平台输入和 overlay
│   ├── ai/                      # Inline AI provider 与配置加载
│   └── ding-board/              # 独立、可嵌入的白板 crate
├── doc/
│   ├── architecture/            # 当前工程与子系统架构
│   ├── plans/                   # 当前功能计划和问题分析
│   ├── acceptance/              # 手动验收与完成总结
│   ├── guides/                  # 使用和操作指南
│   ├── prototypes/              # 交互原型
│   ├── refactor/                # 正在使用的重构计划
│   └── archive/                 # 历史迁移资料，不代表当前结构
└── scripts/
    ├── dev/                     # 启动、结构检查和 workspace 验证
    ├── database/                # 远程 PostgreSQL 与隧道工具
    └── archive/                 # 已完成的一次性迁移脚本
```

更精确的结构说明见 [Cditor Project Structure](doc/architecture/project-structure.md)。

## Workspace Crate 职责

| 目录 | Cargo 包 | 职责 | 不应包含 |
| --- | --- | --- | --- |
| `crates/core` | `cditor-core` | Block、DocumentIndex、RichText、Selection、Transaction、Layout 基础模型 | GPUI、SQLx、具体数据库 |
| `crates/editor` | `cditor-editor` | VirtualScroll、ScrollAnchor、WindowPlanner、HitTest、Trace Replay | GPUI View、PostgreSQL |
| `crates/runtime` | `cditor-runtime` | DocumentRuntime、编辑会话、projection、payload window、调度 | 应用窗口与视觉组件 |
| `crates/store` | `cditor-storage` | 存储契约、layout cache、debounce、optimistic persistence | PostgreSQL SQL 和 GPUI |
| `crates/store-postgres` | `cditor-storage-postgres` | PostgreSQL pool、migration、stores、queue、类型映射 | 编辑器交互和 UI 状态 |
| `crates/app` | `cditor-app` | GPUI 应用入口、Block 渲染、输入、overlay、持久化桥接 | 文档真相和全局滚动真相 |
| `crates/ai` | `cditor-ai` | AI provider、配置、流式结果和取消 | 文档结构与 UI |
| `crates/ding-board` | `ding-board` | 独立白板模型、渲染、输入和资源 | Cditor 内核依赖 |

### 依赖方向

```text
cditor-core
  ├──> cditor-editor
  ├──> cditor-storage ──> cditor-storage-postgres
  └──────────────────────────────┐
                                 v
cditor-ai ─────────────────> cditor-runtime
cditor-editor ─────────────> cditor-runtime
cditor-storage ────────────> cditor-runtime
cditor-storage-postgres ───> cditor-runtime   # 当前冷启动适配
                                 ^
                                 |
                             cditor-app <───── ding-board
```

箭头表示“被依赖层指向依赖者”：例如 `cditor-runtime` 使用 `cditor-core`、`cditor-editor`、`cditor-storage`、`cditor-storage-postgres` 和 `cditor-ai`；`cditor-app` 是最终组装层，并直接组合这些 crate 与 `ding-board`。

`cditor-editor` 名称容易被误解：它不是 GPUI UI 层，而是无 UI 框架依赖的编辑器视口算法层；实际 GPUI 渲染和交互入口位于 `cditor-app`。

当前 `cditor-runtime` 仍包含 PostgreSQL 冷启动适配，因此直接依赖 `cditor-storage-postgres`。这是已知的架构边界，新增存储能力应优先通过 `cditor-storage` 抽象，避免继续扩大具体数据库依赖。

## 环境要求

必需：

- 支持 Rust 2024 edition 的稳定 Rust toolchain。
- Git。GPUI 和 Mermaid renderer 当前固定到 Zed 仓库的指定 revision，首次构建需要获取 Git 依赖。
- GPUI 所需的平台编译环境。

可选：

- Docker 与 Docker Compose，用于本地 PostgreSQL。
- 可兼容 OpenAI API 的模型服务，用于 Inline AI。
- SSH，用于远程 PostgreSQL 初始化和隧道脚本。

检查 Rust 环境：

```bash
rustc --version
cargo --version
```

## 快速开始

### 1. 无数据库启动

不设置 `CDITOR_DATABASE_URL` 时，二进制默认打开内存后端，不需要 PostgreSQL：

```bash
cargo run -p cditor-app
```

打开小型演示文档：

```bash
CDITOR_SMALL_DEMO=1 cargo run -p cditor-app
```

打开 10 万 Block 大型演示文档：

```bash
CDITOR_LARGE_DEMO=1 cargo run -p cditor-app
```

大型演示会构造大文档性能场景，启动时间和内存占用会高于普通模式。

### 2. 使用本地 PostgreSQL

启动开发数据库：

```bash
docker compose up -d postgres
```

使用默认开发连接运行：

```bash
./scripts/dev/run_editor.sh
```

该脚本默认设置开发数据库连接：

```text
CDITOR_DATABASE_URL=postgres://cditor:cditor@localhost:5432/cditor_dev
```

`CDITOR_DOCUMENT_ID` 未显式设置时，应用使用文档 ID `1`。

查看数据库状态：

```bash
docker compose ps
```

停止容器：

```bash
docker compose down
```

数据保存在 Docker volume 中；`docker compose down -v` 会删除本地数据库数据，请谨慎使用。

### 3. 使用自定义 PostgreSQL

```bash
export CDITOR_DATABASE_URL='postgres://user:password@host:5432/database'
export CDITOR_DOCUMENT_ID=42
cargo run -p cditor-app
```

远程数据库工具见 [scripts/README.md](scripts/README.md) 和 [远程 PostgreSQL 指南](doc/architecture/remote-postgres.md)。

## 运行配置

### 编辑器环境变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `CDITOR_DATABASE_URL` | 未设置 | 设置后使用 PostgreSQL；未设置时使用内存或 demo 后端 |
| `CDITOR_DOCUMENT_ID` | `1`（数据库模式） | 打开的文档 ID |
| `CDITOR_WORKSPACE_ID` | 未设置 | Workspace ID |
| `CDITOR_SMALL_DEMO` | `false` | 无数据库时打开小型演示 |
| `CDITOR_LARGE_DEMO` | `false` | 无数据库时打开 10 万 Block 演示 |
| `CDITOR_READONLY` | `false` | 只读模式 |
| `CDITOR_DEBUG_OVERLAY` | `false` | 显示布局、窗口和滚动调试信息 |
| `CDITOR_PAYLOAD_WINDOW_SIZE` | `128` | payload window 大小，最小为 1 |
| `CDITOR_SEED_LARGE_DEMO` | `false` | 在 PostgreSQL 中创建大型演示文档 |
| `CDITOR_SEED_LARGE_DEMO_BLOCKS` | `100000` | PostgreSQL 大型演示的 Block 数量 |
| `CDITOR_FORCE_RESEED_LARGE_DEMO` | `false` | 强制重建 PostgreSQL 演示数据 |
| `CDITOR_TRACE_INPUT` | `false` | 输出平台输入与 IME trace |
| `CDITOR_TRACE_TABLE` | `false` | 输出表格交互 trace |
| `CDITOR_TRACE_MARKDOWN` | `false` | 输出 Markdown 与剪贴板 trace |

布尔变量接受 `1/true/yes/on` 与 `0/false/no/off`，大小写不敏感。

### Inline AI

非敏感配置位于 [config/ai.toml](config/ai.toml)：

```toml
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
```

密钥必须通过环境变量或本地 `.env` 提供，不要提交真实 Token：

```bash
export CDITOR_AI_API_KEY='your-api-key'
```

兼容变量：

| Cditor 变量 | 兼容变量 |
| --- | --- |
| `CDITOR_AI_API_KEY` | `OPENAI_AUTH_TOKEN`、`OPENAI_API_KEY` |
| `CDITOR_AI_BASE_URL` | `OPENAI_BASE_URL` |
| `CDITOR_AI_MODEL` | `OPENAI_MODEL` |
| `CDITOR_AI_CONFIG` | 自定义 TOML 配置路径 |

AI 配置优先级为：进程环境变量、本地 `.env`、配置文件、内置默认值。未配置 Token 时应用继续运行，并使用 mock provider。

## 构建

默认成员是 `cditor-app`：

```bash
cargo build
```

构建整个 workspace：

```bash
cargo build --workspace
```

检查整个 workspace：

```bash
cargo check --workspace
```

检查指定 crate：

```bash
cargo check -p cditor-core
cargo check -p cditor-runtime
cargo check -p cditor-app
```

启用 GPUI runtime shader feature：

```bash
cargo run -p cditor-app --features runtime-shaders
```

## 测试与质量门禁

运行全部默认测试：

```bash
cargo test --workspace
```

运行指定 crate：

```bash
cargo test -p cditor-core
cargo test -p cditor-editor
cargo test -p cditor-runtime
cargo test -p cditor-app --lib
```

运行结构检查：

```bash
./scripts/dev/check_structure.sh
```

运行完整本地门禁：

```bash
./scripts/dev/check_workspace.sh
```

完整门禁依次执行：

1. 工程结构检查。
2. `cargo fmt --all -- --check`。
3. `cargo check --workspace`。
4. `cargo test --workspace`。

### PostgreSQL 集成测试

启动隔离的测试数据库：

```bash
docker compose up -d postgres_test
export CDITOR_TEST_DATABASE_URL='postgres://cditor:cditor@localhost:5433/cditor_test'
```

PostgreSQL 测试默认标记为 ignored，避免普通单元测试依赖外部服务。按 crate 运行：

```bash
cargo test -p cditor-storage-postgres -- --ignored
cargo test -p cditor-runtime -- --ignored
cargo test -p cditor-app --lib -- --ignored
```

部分 ignored 测试会插入或加载 10 万 Block，执行时间和数据库占用明显高于普通测试。

## 开发规范

### 目录和文件

- 非白板 Rust 源文件不得超过 700 行。
- 超限文件必须按模型、输入、渲染、持久化、几何或测试职责拆分。
- 测试优先放在同级 `*_tests.rs` 或模块的 `tests/` 目录。
- 一次性迁移脚本放入 `scripts/archive/`，不得作为日常开发入口。
- 历史迁移文档放入 `doc/archive/`，不得用于描述当前结构。
- Workspace 只保留根 `Cargo.lock`。
- Token、数据库密码和本地路径不得提交。

`./scripts/dev/check_structure.sh` 会检查 700 行上限、废弃的 `crates/engine` 路径和系统元数据。

### 新功能落点

| 功能类型 | 主要目录 |
| --- | --- |
| 新 Block 类型和 payload | `crates/core/src/block`、`crates/core/src/rich_text` |
| 文档事务与 selection | `crates/core/src/edit` |
| 高度估算和布局索引 | `crates/core/src/layout` |
| 虚拟滚动、anchor、window planner | `crates/editor/src/scroll`、`crates/editor/src/window` |
| 活文档编辑与 projection | `crates/runtime/src/document_runtime`、`crates/runtime/src/projection` |
| 调度和性能预算 | `crates/runtime/src/scheduling` |
| 存储抽象与缓存策略 | `crates/store/src` |
| PostgreSQL 表和查询 | `crates/store-postgres/migrations`、`crates/store-postgres/src/stores` |
| GPUI Block 渲染 | `crates/app/src/gui/block` |
| 键盘、鼠标和 IME | `crates/app/src/gui/input`、`crates/app/src/gui/app/input` |
| Overlay 和浮层交互 | `crates/app/src/gui/overlay` |
| AI provider | `crates/ai/src` |
| Cditor 与白板适配 | `crates/app/src/gui/block/whiteboard` |

新增功能必须同时补充单元测试；涉及数据库、跨 crate 事务或恢复流程时，还应增加集成测试。

## 调试

常用组合：

```bash
CDITOR_SMALL_DEMO=1 CDITOR_DEBUG_OVERLAY=1 CDITOR_TRACE_INPUT=1 cargo run -p cditor-app
```

表格问题：

```bash
CDITOR_SMALL_DEMO=1 CDITOR_TRACE_TABLE=1 cargo run -p cditor-app
```

Markdown 或剪贴板问题：

```bash
CDITOR_SMALL_DEMO=1 CDITOR_TRACE_MARKDOWN=1 cargo run -p cditor-app
```

## 文档导航

- [文档索引](doc/README.md)
- [10 万 Block 大文档架构](doc/large-document-rich-text-architecture.md)
- [当前实现状态](doc/large-document-rich-text-implementation-status.md)
- [当前工程结构](doc/architecture/project-structure.md)
- [V2 GUI 架构](doc/architecture/v2-rich-text-editor-gui-architecture.md)
- [数据库实现方案](doc/architecture/database-implementation-plan.md)
- [当前问题与任务清单](doc/plans/current-editor-issues-deep-analysis-and-task-list.md)
- [表格功能计划](doc/plans/notion-table-feature-plan.md)
- [表格手动验收](doc/acceptance/table-manual-acceptance.md)
- [脚本说明](scripts/README.md)

`doc/archive/` 中的内容仅用于保留迁移背景，不代表当前目录、命令或实现状态。

## 当前目录合理性结论

当前目录整体合理：

- Workspace crate 已按模型、视口算法、运行时、存储、UI、AI 和独立白板划分。
- `runtime` 目录与 `cditor-runtime` 包名一致。
- GPUI 代码集中在 `app`，核心模型未反向依赖 UI。
- PostgreSQL 实现和通用存储状态机分离。
- 文档、脚本和测试已经按用途归类。
- 非白板源码有明确的 700 行结构门禁。

仍有两个需要持续关注的边界：

1. `cditor-editor` 实际承担无 GPUI 的视口算法，名称可能让新贡献者误以为它是 UI crate；当前通过本文和工程结构文档明确其职责，暂不做破坏性包名变更。
2. `cditor-runtime` 仍直接依赖 `cditor-storage-postgres` 的冷启动适配。后续若增加其他存储后端，应把这部分收敛到应用组装层或通用存储接口，不能继续在 runtime 扩散具体数据库代码。

除此之外，当前结构适合继续迭代，不需要再次进行大规模目录搬迁。

## License

项目许可证与第三方声明：

- [LICENSE-GPL](LICENSE-GPL)
- [LICENSE-APACHE](LICENSE-APACHE)
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
