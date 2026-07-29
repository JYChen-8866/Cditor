# Cditor

[English](README.md) | 简体中文

<img width="1920" height="1140" alt="Cditor 编辑器截图" src="https://github.com/user-attachments/assets/7224e1ec-a13a-4d0c-987c-75d3db81289a" />

Cditor 是一个使用 Rust 和 [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) 构建的开源 Block 富文本编辑器。项目专注于大文档、原生桌面性能、稳定虚拟滚动、结构化编辑，以及嵌入其他 GPUI 应用的能力。

> [!IMPORTANT]
> Cditor 仍在积极开发中。在稳定版本发布之前，API、持久化格式和用户交互行为都可能发生变化。

## 主要特性

- 面向最高 100,000 个 Block 文档设计的窗口化渲染与虚拟滚动
- 支持段落、标题、引用、Callout、列表、Todo、Toggle、代码块、表格、图片、Mermaid 图表和白板
- 富文本 marks、Markdown 导入导出、结构化编辑和跨 Block 选择
- 原生键盘、剪贴板、鼠标和 IME 集成，支持中文、日文、韩文及 Emoji 输入
- 基于持久化文档事务的撤销与重做
- 表格单元格选择、复制粘贴、合并拆分、尺寸调整、重排和横向滚动
- SQLite 与 PostgreSQL 持久化适配器
- 通过 OpenAI 兼容 Provider 接入内联 AI
- 可复用的 SDK 和 GPUI 组件 API
- 支持 macOS 和 Windows 桌面目标

## 项目状态

Cditor 当前适合开发、实验和集成测试，暂未作为生产稳定版编辑器发布。

编辑器架构将文档真相与当前渲染视口分离：

> UI 只是当前视口的投影。文档、Selection、布局高度、事务和滚动状态属于编辑器内核，而不依赖 GPUI Entity 的生命周期。

因此，Cditor 可以只加载和渲染有限的 payload 窗口，同时保留整篇文档级别的编辑能力。

详细设计与实现说明：

- [大文档架构](doc/large-document-rich-text-architecture.md)
- [实现状态](doc/large-document-rich-text-implementation-status.md)
- [项目结构](doc/architecture/project-structure.md)
- [组件 API 与集成指南](doc/guides/cditor-component-integration.md)

## 快速开始

### 环境要求

- 支持 Rust 2024 Edition 的稳定版 Rust 工具链
- Git
- GPUI 在目标平台所需的原生编译工具

Windows 请使用 64 位 MSVC Rust 工具链，并安装 Visual Studio Build Tools、**使用 C++ 的桌面开发**工作负载和当前版本的 Windows SDK。

### 运行桌面编辑器

```bash
cargo run -p cditor-desktop
```

没有配置数据库时，Cditor 会打开内存中的演示文档。

运行小型演示文档：

```bash
CDITOR_SMALL_DEMO=1 cargo run -p cditor-desktop
```

运行 100,000 Block 性能演示：

```bash
CDITOR_LARGE_DEMO=1 cargo run -p cditor-desktop
```

PowerShell 写法：

```powershell
$env:CDITOR_LARGE_DEMO = "1"
cargo run -p cditor-desktop
```

### 可选数据库后端

使用 SQLite：

```bash
CDITOR_SQLITE_PATH=./cditor.db cargo run -p cditor-desktop
```

启动开发用 PostgreSQL 容器并运行编辑器：

```bash
docker compose up -d postgres
./scripts/dev/run_editor_postgres.sh
```

也可以直接配置 PostgreSQL：

```bash
export CDITOR_DATABASE_URL='postgres://user:password@localhost:5432/cditor'
export CDITOR_DOCUMENT_ID=1
cargo run -p cditor-desktop
```

`CDITOR_SQLITE_PATH` 与 `CDITOR_DATABASE_URL` 不能同时使用。

## 构建与测试

构建默认桌面应用：

```bash
cargo build
```

检查完整 Workspace：

```bash
cargo check --workspace
```

运行所有 Workspace 测试：

```bash
cargo test --workspace
```

检查格式并运行仓库质量门禁：

```bash
cargo fmt --all -- --check
./scripts/dev/check_workspace.sh
```

日常性能开发建议使用：

```bash
./scripts/dev/run_editor_sqlite.sh
./scripts/dev/run_editor_postgres.sh
```

这些脚本默认使用经过优化的 `editor-dev` Cargo Profile，同时保留开发期诊断能力。

## 配置

常用环境变量：

| 变量 | 说明 |
| --- | --- |
| `CDITOR_DATABASE_URL` | PostgreSQL 连接 URL |
| `CDITOR_SQLITE_PATH` | SQLite 数据库路径 |
| `CDITOR_DOCUMENT_ID` | 数据库模式下要打开的文档 |
| `CDITOR_SMALL_DEMO` | 加载内置小型演示文档 |
| `CDITOR_LARGE_DEMO` | 加载 100,000 Block 演示文档 |
| `CDITOR_READONLY` | 以只读模式打开编辑器 |
| `CDITOR_DEBUG_OVERLAY` | 显示布局和视口诊断信息 |
| `CDITOR_AI_API_KEY` | OpenAI 兼容 AI Provider 的 API Key |
| `CDITOR_AI_BASE_URL` | OpenAI 兼容 API 的 Base URL |
| `CDITOR_AI_MODEL` | AI 模型名称 |

布尔变量不区分大小写，支持 `1`、`true`、`yes`、`on` 及对应的 false 值。

请勿提交 API Key 或生产数据库凭据。应使用进程环境变量或本地且已被忽略的 `.env` 文件。

## 嵌入 Cditor

Cditor 可以嵌入其他 GPUI 应用。可复用的集成能力分布在 SDK、Protocol、Session、Runtime 和 GPUI Editor Crate 中。

直接嵌入 `CditorV2View` 的应用必须在 GPUI 启动阶段安装编辑器键位：

```rust
cditor_editor_gpui::input::bind_cditor_keys(cx);
```

初始化、命令、事件、持久化 Provider 和生命周期的完整说明，请参阅 [Cditor 组件 API 与集成指南](doc/guides/cditor-component-integration.md)。

## Workspace 概览

```text
apps/
├── cditor-desktop/              GPUI 桌面应用
└── cditor-web/                  Web 应用实验
components/
├── cditor-component/            共享 GPUI 组件
├── cditor-whiteboard/           Cditor 白板组件
└── cditor-whiteboard-drafft/    drafft-ink 的 GPUI 适配
crates/
├── cditor-core/                 文档模型、Block、Selection 和事务
├── cditor-viewport/             无框架依赖的视口算法
├── cditor-runtime/              实时文档状态与投影
├── cditor-session/              应用服务与任务协调
├── cditor-editor-gpui/          GPUI 渲染与平台输入
├── cditor-editor-protocol/      命令、查询、事件和协议类型
├── cditor-sdk/                  公开嵌入 API
├── cditor-text/                 文本 shaping、布局和几何
├── cditor-storage*/             存储契约与适配器
├── cditor-import-export/        外部格式支持
└── cditor-ai*/                  AI 契约与 Provider 适配器
```

更详细的职责划分和依赖关系请参阅[项目结构](doc/architecture/project-structure.md)。

## 第三方组件

Cditor 包含并适配了其他开源项目中的组件。原项目的版权声明与许可证条款继续有效。

### drafft-ink 白板

白板实现基于 [PatWie/drafft-ink](https://github.com/PatWie/drafft-ink)。Cditor 将其用户界面和集成层适配为 GPUI，同时保留上游绘图引擎和适用的版权声明。

Vendored 上游代码位于：

```text
components/cditor-whiteboard-drafft/vendor/drafft-ink/
```

Vendored drafft-ink 源码附带上游的 **GNU Affero General Public License v3** 许可证文本。修改或分发该组件时，除了仓库其他部分适用的许可证外，还必须遵守这些上游条款。

### Zed Mermaid 渲染器

Cditor 使用 Zed 的 `mermaid_render` Crate 实现原生 Mermaid 图表渲染：

- 上游项目：[zed-industries/zed](https://github.com/zed-industries/zed)
- 上游组件：`crates/mermaid_render`
- 集成方式：由 `Cargo.lock` 记录固定 revision 的 Git 依赖

Zed 及其 Mermaid 渲染组件保留各自上游版权和许可证声明。

具体 revision、传递依赖、字体、图标和必要署名信息请参阅 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 参与贡献

欢迎提交 Issue 和 Pull Request。提交改动前请确保：

1. 在可能的情况下，让领域逻辑保持独立于 GPUI。
2. 为新增行为和回归问题添加测试。
3. 运行格式检查、编译检查和相关测试。
4. 不提交密钥、本地数据库或生成的构建产物。
5. 保留第三方版权和许可证声明。

建议运行：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

## 许可证

除非具体文件或打包的第三方组件另有声明，Cditor 使用 **GNU General Public License v3.0 or later**（`GPL-3.0-or-later`）发布，详见 [LICENSE-GPL](LICENSE-GPL)。

第三方组件继续适用各自的许可证。尤其是基于 drafft-ink 的 Vendored 白板代码，适用上游的 **GNU Affero General Public License v3** 条款。重新分发前，请阅读 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 以及各组件随附的许可证文件。
