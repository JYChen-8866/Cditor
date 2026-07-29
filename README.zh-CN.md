# Cditor

[English](README.md) | [简体中文](README.zh-CN.md)

<img width="1920" height="1140" alt="Cditor 编辑器" src="https://github.com/user-attachments/assets/7224e1ec-a13a-4d0c-987c-75d3db81289a" />

Cditor 是一款使用 Rust 和 GPUI 构建的 Block 富文本编辑器。它面向大文档、原生级编辑体验、
独立文档状态，以及在其他 Rust/GPUI 应用中嵌入的使用场景而设计。

当前仓库包含原生桌面应用、实验性 WASM 预览、无框架依赖的控制面 SDK、GPUI 编辑器组件，
以及支撑它们的文档、Runtime、Session、文本、视口、存储契约、AI、导入导出和白板层。

## 当前状态

Cditor 正在持续开发，当前版本为 `0.2.6`。原生桌面端是目前主要的运行和集成路径。Web
目标是用于验证 GPUI WASM 能力的开发预览；浏览器输入、IME 和白板行为仍在持续完善。

本项目还不是一个完整的跨语言编辑器 SDK。当前可复用的公开接口主要面向 Rust 和 GPUI。
在稳定版本发布之前，API、文档格式和 feature 边界仍可能调整。

## 当前能力

| 领域 | 已实现能力 |
| --- | --- |
| 大文档 | 轻量文档索引与可见索引、分页高度模型、虚拟滚动、渲染窗口、锚点恢复、payload 窗口加载，以及 100,000 Block fixture 和基准测试 |
| 富文本与输入 | 行内 marks、Markdown 快捷输入与导入、跨 Block 选择和剪贴板操作、撤销重做、UTF-8/UTF-16 偏移映射、原生按键 action、IME composition 和候选框坐标 |
| 结构化 Block | 段落、标题、引用、Callout、Todo、列表、Toggle、代码、公式、分隔线、表格、图片、Collection、文件、Embed、HTML、Mermaid、分栏、思维导图、白板和自定义 Block |
| 高级编辑面 | 表格单元格编辑与导航、语法高亮、原生 Mermaid 渲染、媒体预览、嵌入式 Drafft 白板和流式内联 AI 预览 |
| 可靠性 | 类型化命令、串行 Runtime 所有权、事务编辑、持久化编排、自动保存策略、关闭保护、恢复导出、Schema 检查、不透明 payload 无损往返和过期异步结果丢弃 |
| 集成 | 无框架 SDK 契约、GPUI 组件与非持有 Handle、宿主提供的存储和 AI 端口、原生桌面组装，以及实验性 WASM 入口 |

实现了某项能力不代表其所有路径都已达到生产成熟度。尤其是 Web 目标和自定义存储 adapter，
目前仍属于宿主和集成工作，而不是完整的最终用户发行版。

## 架构

### 核心原则

> UI 只是当前视口的投影。文档内容、Selection、布局高度、滚动状态、事务和持久化状态必须
> 独立于 GPUI Entity 生命周期存在。

这一原则允许编辑器按视口创建和销毁 Entity，而不丢失文档真相。输入热路径不会同步打开
数据库、加载完整文档、对所有 Block 做文本 shaping，也不会等待后台任务。

```text
宿主应用 / cditor-desktop / cditor-web
                         |
                 cditor-editor-gpui
                         |
                    cditor-session
                         |
                    cditor-runtime
                         |
                  cditor-viewport
                         |
                     cditor-core

cditor-sdk ------------> 类型化配置、命令、事件和 Provider
宿主存储 adapter -------> cditor-storage::DocumentStorage ---> cditor-session
```

依赖只能向内。`cditor-core`、`cditor-viewport` 和 `cditor-runtime` 不依赖 GPUI 或具体数据库。
`cditor-session` 持有唯一串行 Runtime，并负责后台任务协调、导入分发、AI 分发和持久化策略。
`cditor-editor-gpui` 负责渲染和平台交互，但不持有文档真相。

### Workspace 分层

| Package 或目录 | 职责 |
| --- | --- |
| `cditor-config`、`cditor-theme`、`cditor-text` | 编译期配置、设计 token、排版、Parley shaping 和文本几何 |
| `cditor-core`、`cditor-editor-protocol` | Block 与 payload schema、文档索引、Selection、事务、布局、类型化命令和投影 |
| `cditor-viewport` | 无框架依赖的虚拟滚动、窗口规划、锚点、Hit Test 和 trace replay |
| `cditor-runtime` | 实时文档变更、composition、命令执行、投影、布局调度和缓存 |
| `cditor-session` | 单一 Runtime 所有权、任务协调、持久化、导入、AI、恢复和应用服务 |
| `cditor-storage` | 无运行时依赖的 `DocumentStorage` 端口、DTO、错误、布局快照、checkpoint 和持久化策略 |
| `cditor-import-export` | 外部格式解析、验证、安全限制和类型化导入计划 |
| `cditor-sdk` | 无 GPUI 的 Builder、命令、事件、诊断、文档类型和 Provider 契约 |
| `cditor-editor-gpui` | GPUI View、组件 Handle、渲染、键盘/鼠标/IME 路由、Overlay 和 feature UI |
| `cditor-ai`、`cditor-ai-openai` | AI Provider 契约与 Mock，以及 OpenAI 兼容 HTTP adapter |
| `components/` | 通用 GPUI 控件，以及独立白板和基于 Drafft 的白板实现 |
| `apps/cditor-desktop`、`apps/cditor-web` | 原生 composition root 和实验性 WASM composition root |
| `cditor-test-support` | 共享契约、fixture、验收模型和帧性能基准 |

## 仓库结构

```text
.
├── apps/
│   ├── cditor-desktop/          # 原生 GPUI 应用与默认 Factory
│   └── cditor-web/              # 实验性 GPUI/WASM 应用与 Vite 宿主
├── components/
│   ├── cditor-component/        # 可复用 GPUI 控件
│   ├── cditor-whiteboard/       # 独立 GPUI 白板
│   └── cditor-whiteboard-drafft/ # Drafft 集成与 vendored 上游 core
├── crates/                       # Core、Runtime、Session、SDK、UI、文本、存储端口、AI
├── assets/                       # 字体与共享应用资源
├── config/                       # 非敏感的编译期和应用配置
├── doc/                          # 架构、指南、计划和带日期的验收报告
├── scripts/                      # 验证、打包和历史迁移辅助脚本
├── Cargo.toml                    # Workspace 与构建 profile
└── Cargo.lock                    # 锁定的 Rust 依赖图
```

## 快速开始

### 环境要求

- 支持 Rust 2024 edition 的较新 Rust 工具链。
- Git，因为 GPUI 和 Mermaid 依赖固定到 Zed 的特定 Git revision。
- 对应目标平台的原生构建工具。
- Web 开发还需要仓库指定的 nightly 工具链、`wasm32-unknown-unknown` target、
  `wasm-bindgen-cli`、Node.js 和 npm。

Windows 请使用 MSVC Rust target，并安装包含“使用 C++ 的桌面开发”的 Visual Studio Build
Tools；不支持 Windows GNU target。Linux 需要 GPUI 使用的 Wayland/X11、Vulkan、字体、音频、
Clang 和 CMake 开发包，具体列表见[桌面端 CI workflow](.github/workflows/desktop-builds.yml)。

### 桌面端

在仓库根目录运行当前完整桌面产品：

```bash
cargo run -p cditor-desktop
```

默认桌面入口打开内置演示文档，并启用完整产品 feature：内联 AI 集成、语法高亮、Mermaid 和
白板编辑。如果没有配置 AI Key，编辑器会继续运行，但 AI 功能会被禁用。

运行 100,000 Block 混合演示文档：

```bash
CDITOR_LARGE_DEMO=1 cargo run -p cditor-desktop
```

评估交互性能时使用优化过的开发 profile：

```bash
cargo run -p cditor-desktop --profile editor-dev
```

构建可分发的原生可执行文件：

```bash
cargo build --locked --release -p cditor-desktop
```

### Web 预览

Web 应用当前打开演示文档，并打包 CJK 字体、Mermaid 和白板代码。它是实验性的第一方预览，
不是稳定的 JavaScript SDK 或 Web Component。

```bash
cd apps/cditor-web
make install
make dev
```

Vite 默认使用 `3000` 端口；如果端口被占用，会在终端输出实际 URL。构建生产 WASM 和 Web
资源：

```bash
cd apps/cditor-web
make build-wasm-release
cd www
npm run build
```

## SDK 嵌入

### 公开 Package

| Package | 用途 |
| --- | --- |
| `cditor-sdk` | 无框架依赖的 `Cditor` 配置、类型化命令/事件、文档 DTO、诊断和 Provider 契约 |
| `cditor-editor-gpui` | `CditorComponent`、`CditorHandle`、`CditorV2View`、键位、渲染和原生输入 |
| `cditor-desktop` | 可直接使用的 Factory、桌面生命周期、远程图片加载、Demo/Memory 组装和 OpenAI 兼容 Provider 选择 |

`cditor-sdk` 有意不创建 GPUI View。需要显示编辑器的宿主应同时使用
`cditor-editor-gpui`，通常通过 `cditor-desktop::wiring::build_component` 完成组装。

### 依赖配置

标准原生集成应使用完整桌面 feature 集：

```toml
[dependencies]
cditor-sdk = { path = "../CDitor-V2/crates/cditor-sdk" }
cditor-editor-gpui = {
    path = "../CDitor-V2/crates/cditor-editor-gpui",
    default-features = false,
    features = ["font-kit"]
}
cditor-desktop = { path = "../CDitor-V2/apps/cditor-desktop" }

gpui = {
    git = "https://github.com/zed-industries/zed",
    rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe",
    default-features = false,
    features = ["font-kit"]
}
gpui_platform = {
    git = "https://github.com/zed-industries/zed",
    rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe",
    default-features = false,
    features = ["font-kit"]
}
```

宿主与 Cditor 必须解析到相同的 GPUI revision，否则 `App`、`Entity`、`Context` 和 View
类型会来自不同 crate 实例，无法互操作。

在 GPUI 启动阶段创建并挂载组件：

```rust
use cditor_desktop::wiring::build_component;
use cditor_editor_gpui::bind_cditor_keys;
use cditor_sdk::Cditor;
use gpui::{App, AppContext, WindowOptions};

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        bind_cditor_keys(cx);
        cx.activate(true);

        let component = build_component(Cditor::new().memory(), cx)
            .expect("build Cditor component");
        let editor = component.view.clone();

        cx.open_window(WindowOptions::default(), move |_window, _cx| editor.clone())
            .expect("open Cditor window");
    });
}
```

打开编辑器窗口前只能调用一次 `bind_cditor_keys`。通过组件的非持有 `handle` 执行焦点控制、
只读切换、撤销重做、类型化命令、Selection、诊断、保存/flush、恢复导出和关闭保护。宿主
不应直接修改 `DocumentRuntime`。

### 存储集成

当前仓库提供存储契约，但不包含具体 SQLite 或 PostgreSQL adapter。宿主负责打开并迁移自己的
后端、实现 `DocumentStorage`，然后注入已经打开的端口：

```rust
use cditor_sdk::Cditor;

let configured = Cditor::new()
    .with_document_id(42)
    .with_storage(storage)
    .with_autosave(2);
```

`storage` 的类型是 `Arc<dyn cditor_storage::DocumentStorage>`。持久化数据源必须提供文档 ID。
路径、URL、凭据、连接池、Migration、Workspace 策略和后端专用恢复由宿主负责；Cditor 负责
文档与 Session 策略，并异步调用注入的契约。

完整 Factory、事件、生命周期、持久化和错误契约见[组件集成指南](doc/guides/cditor-component-integration.md)。

## Feature 模型

| Package | 默认值与生产语义 |
| --- | --- |
| `cditor-editor-gpui` | 默认是 `full + test-support`；`full` 展开为 `font-kit`、`code-highlight`、`mermaid` 和 `whiteboard` |
| `cditor-desktop` | 默认是 `full`；它启用 `ai`、`code-highlight`、`mermaid` 和 `whiteboard`，原生 `font-kit` 由其编辑器依赖始终启用 |
| `cditor-web` | 关闭原生默认 feature，在不使用 `font-kit` 的情况下启用 `mermaid` 和 `whiteboard` |

标准桌面构建是完整产品，不是精简版本。不使用 `cditor-desktop` 的自定义生产 GPUI 宿主，
应避免编辑器默认的 `test-support` feature，同时显式启用所有生产能力：

```toml
cditor-editor-gpui = {
    path = "../CDitor-V2/crates/cditor-editor-gpui",
    default-features = false,
    features = ["font-kit", "code-highlight", "mermaid", "whiteboard"]
}
```

Feature 开关控制具体 UI 实现和重型渲染依赖，不会删除文档 schema。例如关闭 `whiteboard`
会移除缩略图和编辑器实现，但保留 `RichBlockKind::Whiteboard`、`WhiteboardPayload`、命令、
撤销重做、剪贴板行为和持久化的 `scene_json`。在支持白板的宿主重新打开文档前，编辑器会显示
稳定占位。

Cargo feature 会在整张依赖图中做加法合并。外部宿主不应直接依赖内部实现
`cditor-whiteboard-drafft`。

## 配置

### 桌面端配置

仓库自带桌面二进制当前只接受展示和演示相关配置：

| 变量 | 含义 |
| --- | --- |
| `CDITOR_SMALL_DEMO` | 选择小型演示文档；它已经是当前默认值，不能与 `CDITOR_LARGE_DEMO` 同时使用 |
| `CDITOR_LARGE_DEMO` | 打开 100,000 Block 混合演示文档 |
| `CDITOR_READONLY` | 以只读模式启动编辑器 |
| `CDITOR_DEBUG_OVERLAY` | 显示编辑器布局和视口诊断信息 |
| `CDITOR_PAYLOAD_WINDOW_SIZE` | 设置 payload 加载窗口；默认值为 `128`，最小值为 `1` |

布尔值不区分大小写，接受 `1/true/yes/on` 和 `0/false/no/off`。

桌面边界会主动拒绝 `CDITOR_SQLITE_PATH`、`CDITOR_DATABASE_URL` 和
`CDITOR_WORKSPACE_ID`。演示入口不读取 `CDITOR_DOCUMENT_ID`。持久化文档必须由宿主通过
`Cditor::with_document_id(...).with_storage(...)` 配置。

### 内联 AI

非敏感默认配置位于 [config/ai.toml](config/ai.toml)。API Key 不应写入仓库：

```bash
export CDITOR_AI_API_KEY='your-api-key'
cargo run -p cditor-desktop
```

支持 `CDITOR_AI_API_KEY`、`CDITOR_AI_BASE_URL`、`CDITOR_AI_MODEL` 和
`CDITOR_AI_CONFIG`。同时兼容 `OPENAI_AUTH_TOKEN`、`OPENAI_API_KEY`、
`OPENAI_BASE_URL` 和 `OPENAI_MODEL`。进程环境变量优先于 `.env`，`.env` 优先于选中的
`config/ai.toml` 配置。

### 诊断

定位特定子系统问题时，将对应 trace 开关设置为 `1`：

```bash
CDITOR_TRACE_INPUT=1 cargo run -p cditor-desktop
CDITOR_TRACE_TABLE=1 cargo run -p cditor-desktop
CDITOR_TRACE_FPS=1 cargo run -p cditor-desktop --profile editor-dev
```

当前其他开关包括 `CDITOR_TRACE_SELECTION`、`CDITOR_TRACE_MARKDOWN`、
`CDITOR_TRACE_PAYLOAD` 和 `CDITOR_TRACE_BLOCK_COLOR`。

## 开发与验证

常用定向检查：

```bash
cargo fmt --all -- --check
cargo check -p cditor-desktop
cargo test -p cditor-core
cargo test -p cditor-runtime
cargo test -p cditor-session
cargo test -p cditor-editor-gpui
cargo test -p cditor-desktop --lib
```

Workspace 全量检查：

```bash
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Web target 应在其目录下检查，以便 Cargo 选择局部 nightly 工具链：

```bash
cd apps/cditor-web
cargo check --target wasm32-unknown-unknown
```

相关基准测试包括：

```bash
cargo bench -p cditor-text --bench text_layout
cargo bench -p cditor-test-support --bench frame_baseline
```

新增行为必须包含定向单元测试。跨越 Session、持久化、导入、恢复或公开组件契约的改动，
还必须增加集成测试或契约测试。

## 平台与发布

桌面 CI 会构建并测试 Linux x64、Windows x64、macOS Apple Silicon 和 macOS Intel。版本标签
会发布带校验和的 Windows EXE，以及 macOS arm64/x64 DMG。Linux 当前会执行构建验证，但不
上传打包后的发行产物。

已发布安装包可从 [GitHub Releases](https://github.com/JYChen-8866/Cditor/releases) 下载。
macOS 包使用 ad-hoc 签名，未经过 Apple 公证。

## 文档

- [文档索引](doc/README.md)
- [成熟 Notion 类编辑器总体架构](doc/architecture/cditor-mature-notion-editor-master-design.md)
- [100,000 Block 大文档架构](doc/large-document-rich-text-architecture.md)
- [可持续架构与依赖规则](doc/architecture/cditor-sustainable-architecture.md)
- [编辑器组件边界](doc/architecture/editor-component-boundary.md)
- [组件 API 与集成指南](doc/guides/cditor-component-integration.md)
- [白板集成架构](doc/whiteboard-integration-architecture.md)
- [当前带日期的验收报告](doc/acceptance/)

`doc/plans`、`doc/refactor` 和带日期的验收目录记录的是设计或迁移快照。如果旧文档与当前
Workspace 冲突，以源码、Cargo manifest、本 README 和明确标注为当前状态的文档为准。

## 当前边界

- 原生桌面路径是主路径；Web 路径是实验性的，当前只打开演示文档。
- 目前没有稳定的 Web Component、JavaScript、C ABI、Swift、Java 或其他跨语言 SDK。
- Workspace 暴露 `DocumentStorage`，但当前不包含具体 SQLite 或 PostgreSQL adapter package。
- `scripts/dev` 下的数据库启动脚本面向已经移除的 adapter，不是当前桌面端启动入口。
- 外部 GPUI 宿主必须使用与 Cditor 相同的 Zed/GPUI 固定 revision。
- 公开 API 和文档格式仍处于 1.0 之前，后续仍可能演进。

## 许可证

Workspace package 声明为 `GPL-3.0-or-later`。被排除在 Workspace 外的 Drafft 集成 package
声明为 `AGPL-3.0`。打包的第三方组件和资源保留各自许可证。重新分发或嵌入前请阅读
[LICENSE-GPL](LICENSE-GPL)、[LICENSE-APACHE](LICENSE-APACHE) 和
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
