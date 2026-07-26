# Cditor 组件接口与集成指南

本文描述当前 workspace 的真实组件边界。Cditor 是基于 GPUI 的 Rust 原生组件，不提供
Web Component、C ABI、Swift、Java 或 JavaScript SDK。

## 1. 三层公开入口

| Crate | 用途 | 是否依赖 GPUI |
| --- | --- | --- |
| `cditor-sdk` | `Cditor` 配置、command/event、诊断和 provider contract | 否 |
| `cditor-editor-gpui` | `CditorComponent`、`CditorHandle`、`CditorV2View`、键位注册 | 是 |
| `cditor-desktop` | SQLite/PostgreSQL/OpenAI/远程图片的默认桌面组装 | 是 |

`cditor-sdk` 不构造 View，也不依赖 GPUI。宿主可以使用 Desktop 的默认 factory，或在自己的
composition root 实现 `CditorViewFactory`。

## 2. 集成前提

- Rust 2024 edition。
- 宿主和 Cditor 使用相同 Zed commit 的 GPUI，避免 `App`、`Entity`、`Context` 类型重复。
- Windows 使用 MSVC target，不支持 GNU target。
- 持久化后端必须有目标文件/数据库的读写和 migration 权限。
- GPUI App 启动时必须且只需注册一次 Cditor keymap。

仓库当前固定的 GPUI revision 以
`crates/cditor-editor-gpui/Cargo.toml` 和 `apps/cditor-desktop/Cargo.toml` 为准。

## 3. 添加依赖

使用默认 Desktop 组装：

```toml
[dependencies]
cditor-sdk = { path = "../CDitor-V2/crates/cditor-sdk" }
cditor-editor-gpui = { path = "../CDitor-V2/crates/cditor-editor-gpui" }
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

通过 Git tag 引入时，对三个 package 使用相同 repository 和 tag，并继续显式固定相同的
GPUI revision。

## 4. 最小可运行窗口

```rust
use cditor_desktop::wiring::build_component;
use cditor_editor_gpui::bind_cditor_keys;
use cditor_sdk::Cditor;
use gpui::*;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        bind_cditor_keys(cx);
        cx.activate(true);

        let component = build_component(Cditor::new().memory(), cx)
            .expect("build Cditor component");
        let editor = component.view.clone();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Cditor".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_window, _cx| editor.clone(),
        )
        .expect("open Cditor window");
    });
}
```

未调用 `bind_cditor_keys` 时 View 可以绘制，但回车、删除、选择、撤销和剪贴板等 command
不会完整工作。

## 5. `Cditor` 配置

`cditor_sdk::Cditor` 是不可变 builder 风格的配置对象。它不持有 View、Runtime 或数据库
连接。

### 5.1 数据来源

| 方法 | 行为 |
| --- | --- |
| `Cditor::new()` / `.demo()` | 使用小型演示文档 |
| `.large_demo()` | 使用 100,000 Block 性能文档 |
| `.memory()` | 创建空白内存文档 |
| `.with_storage_provider(provider)` | 注入 `Arc<dyn StorageProvider>` |
| `.with_storage(storage, label)` | 包装一个已打开的 `DocumentStorage` |
| `.with_cloud_endpoint(endpoint)` | 选择 Cloud 配置；远端协议仍未完成 |

Desktop 提供 `CditorStorageExt`，把具体 adapter 限制在 composition root：

```rust
use cditor_desktop::CditorStorageExt;
use cditor_sdk::Cditor;

let sqlite = Cditor::new()
    .with_document_id(42)
    .with_sqlite_path("workspace.cditor.db");

let postgres = Cditor::new()
    .with_document_id(42)
    .with_postgres_url("postgres://localhost/cditor");
```

SDK 本身不依赖 SQLite、PostgreSQL、SQLx、HTTP 或环境变量。

### 5.2 文档和行为

```rust
let configured = Cditor::new()
    .with_workspace_id(7)
    .with_document_id(42)
    .with_readonly(false)
    .with_debug_overlay(false)
    .with_payload_window_size(128)
    .with_autosave_interval(std::time::Duration::from_secs(2));
```

`.without_autosave()` 关闭自动保存；`.with_ai_provider(provider)` 注入 AI contract；
`.without_ai()` 禁用 AI。

持久化后端必须指定 `document_id`。无效配置由 `build_component` 返回
`CditorError::InvalidInput`，不会创建一个半初始化组件。

## 6. 组件和 Handle

`build_component` 返回：

```rust
CditorComponent<CditorV2View> {
    view: Entity<CditorV2View>,
    handle: CditorHandle<CditorV2View>,
}
```

- `view` 挂载到宿主的 GPUI 元素树。
- `handle` 是非保留控制面；组件销毁后操作返回 `ComponentDropped`。
- 宿主不应直接修改 `DocumentRuntime`，所有 mutation 通过 Handle command 或
  `CditorViewContract` 进入 Session/Runtime。

Handle 支持的主要能力：

- focus / blur / readonly；
- undo / redo / save / flush；
- document info、dirty、save status、close guard；
- selection、selected text、scroll to block；
- typed command 执行和 command state；
- diagnostics。

异步保存和 flush 返回 GPUI `Task<Result<SaveReport, CditorError>>`，宿主必须等待 Task，
不能把“已发起”当作“已持久化”。关闭窗口前检查 `close_guard()`。

## 7. 自定义组装

不希望依赖 `cditor-desktop` 时，宿主 composition crate 可以实现：

```rust
pub trait CditorViewFactory {
    type View: CditorViewContract;

    fn build_component(
        &self,
        builder: Cditor,
        cx: &mut App,
    ) -> Result<CditorComponent<Self::View>, CditorError>;
}
```

自定义 factory 负责：

1. 验证 backend 和 document target；
2. 打开具体 StorageProvider；
3. 创建 `EditorSession` 并保持 Runtime 单一串行所有者；
4. 注入 AI、远程图片和平台能力；
5. 构造实现 `CditorViewContract` 的 GPUI View。

不要在 SDK、Core、Runtime 或 Session 内读取环境变量、创建数据库 pool、HTTP client 或
GPUI Entity。

## 8. 错误和生命周期

公开错误使用 `CditorError`：

- `InvalidInput`：缺少文档 ID、后端配置不合法；
- `NotReady`：冷启动尚未完成；
- `Readonly`：mutation 被只读策略拒绝；
- `Unsupported`：当前 backend 或 command 不支持；
- `ComponentDropped`：Handle 指向的 View 已销毁；
- `Internal`：组装或运行时错误。

推荐生命周期：创建配置 -> build component -> 挂载 View -> 等待 ready event -> 通过 Handle
交互 -> close guard -> flush -> 销毁 View。

## 9. 验证

仓库内组件 API 回归：

```sh
cargo test -p cditor-sdk --all-targets
cargo test -p cditor-desktop --test component_sdk
cargo clippy -p cditor-sdk -p cditor-editor-gpui -p cditor-desktop \
  --all-targets -- -D warnings
```

SDK 的 compile contract 位于 `crates/cditor-sdk/tests/public_api.rs`，用于保证 SDK 的公开面
不泄漏 GPUI 和具体 adapter。
