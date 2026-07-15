# Cditor 组件接口与集成指南

Cditor 当前以 Rust crate 的形式提供基于 GPUI 的原生桌面编辑器组件。它适合嵌入使用同一 GPUI 版本的 Rust 桌面应用，目前不提供 React、Vue、Web Component、C ABI、Swift、Java 或 JavaScript SDK。

本文以仓库当前版本 `v0.2.4` 为基准。公开组件入口位于 `cditor-app` crate：

```rust
use cditor_app::CditorBuilder;
```

## 1. 集成前提

- Rust 工程使用 Rust 2024 edition。
- 宿主应用使用 GPUI。
- 宿主与 Cditor 必须依赖相同的 Zed commit，避免出现两个不同版本的 `gpui::App`、`Entity` 和 `Context` 类型。
- Windows 使用 MSVC toolchain，不支持 GNU target。
- 使用 PostgreSQL 时，数据库必须能够执行 Cditor migrations。
- 使用 SQLite 时，宿主进程必须能创建或写入目标数据库文件及其 WAL 文件。

## 2. 添加依赖

### 2.1 从 GitHub tag 引入

```toml
[dependencies]
cditor-app = {
    git = "https://github.com/JYChen-8866/Cditor.git",
    tag = "v0.2.4"
}

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

升级 Cditor 时，应同时检查 `crates/app/Cargo.toml` 中固定的 Zed revision，并让宿主应用保持一致。

### 2.2 本地路径依赖

如果宿主工程和 Cditor 位于同一台开发机：

```toml
[dependencies]
cditor-app = { path = "../CDitor-V2/crates/app" }
```

宿主仍然需要使用与 Cditor 相同来源和 revision 的 GPUI。

## 3. 最小可运行示例

下面的示例创建一个使用系统标题栏、内存后端的空白编辑器窗口：

```rust
use cditor_app::CditorBuilder;
use gpui::*;

fn main() {
    let app = gpui_platform::application();

    app.run(|cx: &mut App| {
        // 每个 GPUI App 注册一次，必须在打开编辑器窗口前调用。
        cditor_app::gui::input::bind_cditor_keys(cx);

        cx.activate(true);
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
            |_window, cx| {
                CditorBuilder::new()
                    .memory()
                    .build(cx)
                    .expect("build Cditor component")
                    .view
            },
        )
        .expect("open Cditor window");
    });
}
```

`bind_cditor_keys` 不可省略。未注册时，编辑器可以被绘制，但回车、删除、选择、撤销和剪贴板等按键命令不能完整工作。

## 4. 嵌入现有 GPUI View

推荐的 `CditorBuilder::build` 返回：

```rust
CditorComponent {
    view: Entity<CditorV2View>,
    handle: CditorHandle,
}
```

宿主可以将它保存为自身 View 的字段，并在 `Render` 中直接挂载：

```rust
use cditor_app::{CditorBuilder, CditorComponent};
use gpui::*;

struct WorkspaceView {
    editor: CditorComponent,
}

impl WorkspaceView {
    fn new(cx: &mut Context<Self>) -> Self {
        let editor = CditorBuilder::new()
            .memory()
            .build(cx)
            .expect("build Cditor component");

        Self { editor }
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.editor.view.clone())
    }
}
```

宿主 App 启动时仍需先执行一次：

```rust
cditor_app::gui::input::bind_cditor_keys(cx);
```

## 5. `CditorBuilder` 构建接口

`CditorBuilder` 使用 builder 模式。`Cditor` 暂时保留为兼容名称。所有配置方法都会消费并返回 `Self`，适合链式调用。

### 5.1 后端选择

| 方法 | 作用 | 是否持久化 |
| --- | --- | --- |
| `CditorBuilder::new()` | 创建 builder，默认使用小型演示文档 | 否 |
| `.demo()` | 加载内置小型演示文档 | 否 |
| `.large_demo()` | 加载内置 100,000 Block 性能演示文档 | 否 |
| `.memory()` | 创建空白内存文档 | 否 |
| `.with_sqlite_path(path)` | 打开或创建本地 SQLite 文档库 | 是 |
| `.with_sqlite_options(options)` | 配置 SQLite durability、busy timeout 和连接数 | 是 |
| `.with_postgres_url(url)` | 由 Cditor 建立 PostgreSQL 连接并执行 migrations | 是 |
| `.with_postgres_pool(pool)` | 复用宿主提供的 `sqlx::PgPool` | 是 |
| `.with_cloud_endpoint(endpoint)` | 配置 Cloud endpoint | 尚未实现完整加载 |

`.with_cloud_endpoint` 目前只会让 View 进入后台加载提示状态，尚未连接远端文档协议，不应作为生产后端。

### 5.2 文档与工作区

```rust
.with_workspace_id(1)
.with_document_id(42)
```

`WorkspaceId` 和 `DocumentId` 当前都是 `u64`。

SQLite 和 PostgreSQL 后端都必须指定 `document_id`。推荐的 `.build(cx)` 在缺少文档 ID 时返回 `CditorError::InvalidInput`；兼容入口 `.build_view(cx)` 仍构造 `LoadFailed` View。

### 5.3 行为配置

| 方法 | 说明 |
| --- | --- |
| `.with_readonly(bool)` | 开启或关闭只读模式 |
| `.with_debug_overlay(bool)` | 显示布局、视口和滚动调试信息 |
| `.with_payload_window_size(usize)` | 设置持久化后端 payload 分片加载窗口，最小为 1 |
| `.with_autosave(seconds)` | 设置自动保存秒数，最小为 1 秒 |
| `.with_autosave_interval(duration)` | 使用 `Duration` 设置自动保存间隔，最小为 1 秒 |
| `.without_autosave()` | 关闭自动保存 |
| `.with_ai_provider(provider)` | 注入宿主管理的 AI Provider |
| `.without_ai()` | 关闭 AI 入口和请求能力 |
| `.with_postgres_large_demo_seed(count, force)` | 向 PostgreSQL 写入大文档测试数据 |

大文档 seed 接口用于开发、性能测试和验收，不建议在普通产品启动流程中启用。

### 5.4 SQLite 配置

最小 SQLite 接入：

```rust
let component = CditorBuilder::new()
    .with_workspace_id(1)
    .with_document_id(42)
    .with_sqlite_path("./workspace.cditor.db")
    .with_autosave(2)
    .build(cx)?;
```

默认配置使用 WAL、外键校验、5 秒 busy timeout 和 `synchronous=FULL`。需要调整时使用结构化配置：

```rust
use cditor_app::{CditorBuilder, SqliteDurability, SqliteStorageOptions};
use std::time::Duration;

let sqlite = SqliteStorageOptions::file("./workspace.cditor.db")
    .durability(SqliteDurability::Full)
    .busy_timeout(Duration::from_secs(5))
    .max_connections(4);

let component = CditorBuilder::new()
    .with_document_id(42)
    .with_sqlite_options(sqlite)
    .build(cx)?;
```

同一个编辑器实例只使用一个 active backend。SQLite 与 PostgreSQL 可以由同一 binary 选择，但当前不会对两个数据库做朴素双写；本地 SQLite + 云端 PostgreSQL 同步属于后续 LocalFirst/outbox 协议。

独立 GUI 入口可通过环境变量启动 SQLite：

```sh
CDITOR_SQLITE_PATH=./workspace.cditor.db \
CDITOR_DOCUMENT_ID=42 \
cargo run -p cditor-app
```

### 5.5 构造与配置读取

| 方法 | 返回值 | 使用场景 |
| --- | --- | --- |
| `.build(cx)` | `Result<CditorComponent, CditorError>` | 推荐入口，同时取得 View 和弱引用 Handle |
| `.build_view(cx)` | `CditorV2View` | 在 `cx.new` 构造闭包内部创建 View |
| `.build_entity(cx)` | `Entity<CditorV2View>` | 直接创建可挂载到宿主 View 的 Entity |
| `.options()` | `&CditorOptions` | 构建前检查配置 |
| `.into_options()` | `CditorOptions` | 消费 builder 并取得配置对象 |

## 6. PostgreSQL 集成

### 6.1 让 Cditor 管理连接

```rust
let component = CditorBuilder::new()
    .with_workspace_id(1)
    .with_document_id(42)
    .with_postgres_url(
        "postgres://cditor:cditor@localhost:5432/cditor_dev",
    )
    .with_payload_window_size(128)
    .with_autosave(3)
    .build(cx)?;
```

URL 模式的冷启动流程会：

1. 创建 SQLx PostgreSQL 连接池。
2. 执行 Cditor migrations。
3. 加载文档元数据和 Block 索引。
4. 加载初始视口附近的 payload。
5. 在后台完成后将组件从 `Loading` 切换到 `Ready`。

普通 PostgreSQL 冷启动超时为 90 秒；启用大文档 seed 时超时为 30 分钟。

### 6.2 复用宿主连接池

```rust
use cditor_app::CditorBuilder;
use sqlx::PgPool;

fn create_editor(pool: PgPool, cx: &mut gpui::App) {
    let _component = CditorBuilder::new()
        .with_workspace_id(1)
        .with_document_id(42)
        .with_postgres_pool(pool)
        .with_autosave(3)
        .build(cx)
        .expect("build Cditor component");
}
```

`with_postgres_pool` 假定连接池已经可用，并且数据库 schema 已经初始化。与 URL 模式不同，该入口不会自动调用 migrations。宿主可以显式使用 Cditor 的存储接口初始化：

```rust
use cditor_app::storage_postgres::run_migrations;

run_migrations(&pool).await?;
```

数据库结构、远程连接和运维方式参见：

- [数据库实现方案](../architecture/database-implementation-plan.md)
- [远程 PostgreSQL](../architecture/remote-postgres.md)
- [PostgreSQL 最小编辑器](../architecture/minimal-postgres-editor.md)

## 7. Handle、状态与事件

宿主通过 `CditorHandle` 控制组件，不需要读取 `CditorV2View` 内部状态：

```rust
let handle = component.handle.clone();

if handle.is_ready(cx) {
    let document = handle.document_info(cx);
    let save_status = handle.save_status(cx);
    let close_guard = handle.close_guard(cx);
}

handle.set_readonly(true, cx)?;
handle.undo(cx)?;
handle.redo(cx)?;
handle.scroll_to_block(42, ScrollAlignment::Center, cx)?;
```

持久化后端还提供真正可等待的保存 barrier：

```rust
let save_report = handle.save(cx).await?;
let flush_report = handle.flush(cx).await?;
```

`save` 等待调用时已有 dirty generation 提交完成；`flush` 在此基础上继续等待后端可靠写队列和 SQLite WAL checkpoint。保存期间产生的新编辑不会被旧 generation 的成功回执误标为 clean。宿主关闭窗口前应检查 `close_guard()`，调用 `flush()` 成功后再释放 `CditorComponent`。当前还没有公开 `close_document` 生命周期命令。

Handle 内部只持有 `WeakEntity<CditorV2View>`。View 被释放后，修改操作返回 `CditorError::ComponentDropped`，Handle 不会延长组件生命周期。

`CditorV2View` 实现了 `EventEmitter<CditorEvent>`。宿主 View 可以使用 GPUI 原生订阅：

```rust
let events = cx.subscribe(&component.view, |host, _view, event, cx| {
    host.handle_cditor_event(event, cx);
});
```

当前事件包括加载成功/失败、内容 revision、dirty、保存、焦点和选区变化。拖选产生的选区变化在渲染帧边界合并；`ContentChanged` 只携带 revision 和来源，不复制整个文档。

统一命令入口支持 Undo、Redo、Select All、Delete Selection、行内格式、Block 转换和标题折叠。菜单或工具栏可先读取 `command_state`：

```rust
let state = handle.command_state(&CditorCommand::ToggleBold, cx);
if state.enabled {
    handle.execute(CditorCommand::ToggleBold, cx)?;
}
```

## 8. 冷启动高级接口

应用层还导出了以下 PostgreSQL 冷启动类型：

```rust
use cditor_app::{
    CditorColdStartPlan,
    CditorPostgresStores,
    CditorRuntimeLoadResult,
    PostgresRuntimeLoadOptions,
    load_runtime_from_options,
};
```

适用场景包括：

- 宿主希望在创建 GPUI View 之前预加载文档。
- 宿主需要查看冷启动报告。
- 宿主需要自定义首屏 payload 数量、视口高度或布局缓存 key。
- 集成测试需要绕过 UI，直接验证 PostgreSQL 文档加载。

默认高级加载参数为：

| 参数 | 默认值 |
| --- | --- |
| `viewport_height` | `720` |
| `initial_payload_window_blocks` | `64` |
| `visible_index_version` | 当前存储层可见索引版本 |
| `layout_key.exact_width_px` | `800` |
| `layout_key.scale_factor_milli` | `1000` |

普通宿主应用优先使用 `CditorBuilder`；只有需要控制冷启动细节时才直接使用这些接口。

## 9. 当前组件 API 边界

当前接口已经支持：

- 创建完整编辑器 View。
- 内存、演示、SQLite 和 PostgreSQL 后端。
- 大文档虚拟滚动与 payload 窗口加载。
- 只读模式。
- SQLite/PostgreSQL 共用的版本化自动保存。
- 宿主主动触发且可等待的 `save`、`flush`。
- 弱引用 Handle 和统一错误模型。
- 加载、保存、dirty、焦点、内容与选区事件。
- 只读、聚焦、失焦、Undo/Redo 和统一命令。
- UTF-8/UTF-16 明确单位的文档选区。
- 基于虚拟滚动内核的 Block 定位。
- 保存状态、close guard 和结构化诊断快照。
- AI Provider 注入和显式关闭。
- 将组件挂载到其他 GPUI View。

当前尚未形成稳定公开接口的能力：

- `open_document`、运行时切换文档。
- `get_markdown`、`set_markdown`。
- `get_json`、`set_json`。
- `close_document` 以及运行时文档关闭状态。
- 大范围 Block 异步查询和批量事务修改。
- 运行时主题切换。
- 搜索、替换和白板导出。
- 自定义 Block、菜单和工具栏扩展。
- Web、C ABI 和其他语言绑定。

当前已经形成 Rust + GPUI 组件 SDK 的第一阶段边界，但还不是跨平台、跨语言 SDK。宿主不应依赖 `CditorV2View`、`DocumentRuntime`、布局缓存或持久化状态机的内部字段；这些实现细节不属于稳定接口。

## 10. SDK 分层

公共接口收敛为三部分：

```text
CditorBuilder  -> 创建和配置组件
CditorHandle   -> 状态、聚焦、选区、命令、滚动与诊断
CditorEvent    -> 内容、选区、加载、保存和错误事件
```

宿主只持有 `CditorComponent` 或分别持有其 View 和 Handle，不直接访问 `DocumentRuntime`。这样可以保持“UI 是视口投影、runtime 是文档真源”的大文档架构边界。

完整的接口分层、Provider、事件、并发约束和实施顺序参见 [Cditor 组件 SDK 接口设计](../architecture/cditor-component-sdk-api-design.md)。

## 11. 源码入口

- `crates/app/src/api/cditor.rs`：Builder 配置与 View 构造。
- `crates/app/src/api/component.rs`：`CditorComponent`。
- `crates/app/src/api/handle.rs`：稳定控制 Handle。
- `crates/app/src/api/document.rs`：文档、选区和 Block 快照类型。
- `crates/app/src/api/command.rs`：统一命令类型。
- `crates/app/src/api/event.rs`：组件事件。
- `crates/app/src/api/providers.rs`：AI、附件、主题与宿主委托契约。
- `crates/app/src/api/error.rs`：统一错误模型。
- `crates/app/src/api/options.rs`：后端与配置类型。
- `crates/app/src/api/cold_start.rs`：持久化后端冷启动与 runtime 构造。
- `crates/store-sqlite`：SQLite migration、连接配置和存储实现。
- `crates/store-postgres`：PostgreSQL 存储实现。
- `crates/app/src/lib.rs`：crate 对外导出。
- `crates/app/src/main.rs`：完整独立窗口启动示例。
- `crates/app/src/gui/app/cditor_v2_view.rs`：GPUI 编辑器 View。
- `crates/app/src/gui/input/actions.rs`：编辑器 keymap 注册入口。
