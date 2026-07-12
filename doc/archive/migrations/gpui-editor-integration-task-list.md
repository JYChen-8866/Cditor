# GPUI 富文本编辑器集成与对外 API 方案

> 目标：在已完成 PostgreSQL 存储层与大文档 runtime 的基础上，把 GPUI 编辑器 UI 分阶段接入 `DocumentRuntime`，并提供稳定对外 Builder API：`Cditor::new().with_*().build_view(cx)`。

## 0. 总原则

- `DocumentRuntime` 是编辑真相，GPUI 不是数据真相。
- `src/gui` 可以依赖 `gpui`，但 `src/core`、`src/runtime`、`src/storage`、`src/editor` 不允许依赖 `gpui`。
- 对外用户优先使用 `Cditor` facade，不直接依赖内部 `CditorV2View`。
- 输入 hot path 不同步等待 PostgreSQL。
- PostgreSQL / 云服务 cold start 后续必须异步化，不能阻塞 GPUI render。
- 每个阶段必须有可运行测试或 `cargo check` 验收。
- 先拆目录和 adapter，再接复杂编辑能力，不直接在 `cditor_v2.rs` 里堆代码。

---

## 1. 最终对外 API 形态

### 1.1 Demo / 内存模式

```rust
use CDitor_V2::Cditor;
use gpui::*;

cx.open_window(WindowOptions::default(), |_window, cx| {
    Cditor::new()
        .demo()
        .with_debug_overlay(true)
        .build_view(cx)
})?;
```

### 1.2 PostgreSQL 本地模式

```rust
Cditor::new()
    .with_postgres_url("postgres://cditor:cditor@localhost:5432/cditor_dev")
    .with_workspace_id(workspace_id)
    .with_document_id(document_id)
    .with_payload_window_size(128)
    .with_debug_overlay(false)
    .build_view(cx)
```

### 1.3 云服务模式预留

```rust
Cditor::new()
    .with_cloud_endpoint("https://api.cditor.example")
    .with_access_token_provider(token_provider)
    .with_workspace_id(workspace_id)
    .with_document_id(document_id)
    .build_view(cx)
```

第一版不实现完整云同步，只保留 API 口径和后续可扩展位置。

---

## 2. Public API 分层

建议新增：

```txt
src/api/
├── mod.rs
├── cditor.rs       # Cditor builder / facade
├── options.rs      # CditorOptions / CditorBackend
└── events.rs       # 后续对外事件
```

`lib.rs` 对外导出：

```rust
pub use api::{Cditor, CditorBackend, CditorOptions};
```

内部 `CditorV2View` 仍在 `src/gui`，但普通使用者不需要直接 new 它。

---

## 3. GUI 目录目标结构

```txt
src/gui/
├── mod.rs
├── app/
│   ├── mod.rs
│   └── cditor_v2_view.rs
├── document/
│   ├── mod.rs
│   ├── document_editor_view.rs
│   ├── document_surface.rs
│   └── viewport.rs
├── block/
│   ├── mod.rs
│   ├── block_view.rs
│   ├── block_shell.rs
│   ├── block_content.rs
│   ├── paragraph.rs
│   ├── heading.rs
│   ├── list.rs
│   ├── quote.rs
│   ├── code.rs
│   ├── table.rs
│   ├── media.rs
│   └── placeholder.rs
├── text/
│   ├── mod.rs
│   ├── element.rs
│   ├── input.rs
│   ├── layout.rs
│   ├── caret.rs
│   ├── selection.rs
│   └── shaping_cache.rs
├── input/
│   ├── mod.rs
│   ├── keyboard.rs
│   ├── mouse.rs
│   ├── ime.rs
│   ├── clipboard.rs
│   └── command.rs
├── overlay/
│   ├── mod.rs
│   ├── caret_overlay.rs
│   ├── selection_overlay.rs
│   ├── slash_menu.rs
│   ├── command_menu.rs
│   ├── floating_toolbar.rs
│   └── debug_overlay.rs
├── scroll/
│   ├── mod.rs
│   ├── wheel_adapter.rs
│   ├── scrollbar_view.rs
│   └── auto_scroll.rs
├── persistence/
│   ├── mod.rs
│   ├── save_indicator.rs
│   └── close_guard.rs
├── rich_text.rs
└── theme.rs
```

---

## 4. 体验入口

### 4.1 现在能体验什么

当前已经可以体验 GPUI 编辑器基础闭环：

- 打开 GPUI 窗口。
- 渲染 demo 文档。
- 点击 / 聚焦 block。
- 输入字符写入 `DocumentRuntime`。
- Enter / Backspace 等基础输入命令进入 runtime。
- 选区 / caret overlay 基础显示。
- 顶部 save indicator 显示 `Clean / Dirty / Readonly`。
- PostgreSQL backend 可显示 loading shell，并在 cold start 完成后 swap 到编辑视图。

还没有完成的体验：

- 真实 PostgreSQL 编辑后保存并重开恢复。
- slash menu。
- 完整 clipboard copy/cut/paste。
- 复杂 block 的完整交互编辑。

### 4.2 Demo 模式启动

默认不需要 PostgreSQL，直接运行：

```sh
cargo run
```

默认行为：

- 使用内置 demo 文档。
- 开启 debug overlay。
- 不依赖 Docker / PostgreSQL。

### 4.3 PostgreSQL 模式启动

如果本地已经有 PostgreSQL 测试库和目标文档，可以用环境变量启动：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@localhost:5433/cditor_test \
CDITOR_DOCUMENT_ID=190001 \
CDITOR_PAYLOAD_WINDOW_SIZE=128 \
CDITOR_DEBUG_OVERLAY=true \
cargo run
```

可选环境变量：

| 环境变量 | 说明 | 默认值 |
| --- | --- | --- |
| `CDITOR_DATABASE_URL` | PostgreSQL 连接串 | 未设置时走 demo |
| `CDITOR_DOCUMENT_ID` | runtime document id | PostgreSQL 模式必填 |
| `CDITOR_WORKSPACE_ID` | workspace id | 可选 |
| `CDITOR_PAYLOAD_WINDOW_SIZE` | 首屏 payload window block 数 | `128` |
| `CDITOR_DEBUG_OVERLAY` | 是否显示 debug overlay | `true` |
| `CDITOR_READONLY` | 是否只读 | `false` |

如果只设置了 `CDITOR_DATABASE_URL` 但没有设置 `CDITOR_DOCUMENT_ID`，程序会回退到 demo 文档，避免启动时直接失败。

### 4.4 近期可体验目标

下一步做到 `GPUI-063 ~ GPUI-066` 后，体验目标升级为：

1. PostgreSQL 打开已有文档。
2. 输入后顶部状态从 `Clean` 变成 `Dirty`。
3. 后台保存时显示 `Saving`。
4. 保存成功回到 `Clean`。
5. 保存失败显示 `Failed(message)`，但内存编辑内容不丢。
6. 关闭窗口时如果仍有 dirty / saving / failed 状态，触发 close guard。

---

## 5. 当前已落地实现细节

### 5.1 对外 facade 与 backend 分流

用户入口固定为 `Cditor` builder：

```rust
Cditor::new()
    .with_document_id(document_id)
    .with_postgres_url("postgres://cditor:cditor@localhost:5432/cditor_dev")
    .with_payload_window_size(128)
    .with_debug_overlay(false)
    .build_view(cx)
```

`Cditor::build_view(cx)` 不直接暴露内部 `CditorV2View` 构造细节，而是根据 `CditorBackend` 选择初始 UI 状态：

| Backend | 首帧行为 | 后续行为 |
| --- | --- | --- |
| `Demo` / `Memory` | 直接构建 `Ready(DocumentRuntime::demo())` | 纯内存编辑 |
| `PostgresUrl` / `PostgresPool` | 立即返回 loading shell | 后台加载 PostgreSQL，成功后 swap 到 `Ready` |
| `Cloud` | 立即返回 loading shell | 第一阶段仅预留 API，不实现云同步 |
| 参数非法，例如 PostgreSQL 缺少 `document_id` | 返回 recoverable error shell | 不 panic，不阻塞窗口打开 |

当前代码落点：

```txt
src/api/
├── cditor.rs       # Cditor builder / build_view / build_entity / backend 分流
├── cold_start.rs   # PostgreSQL cold start plan 和 runtime loader
├── options.rs      # CditorOptions / CditorBackend
└── mod.rs
```

### 5.2 GPUI 根 View 状态机

`CditorV2View` 不再假设 runtime 一定已经存在，而是显式维护三态：

```rust
pub enum CditorViewState {
    Ready(DocumentRuntime),
    Loading { message: String },
    LoadFailed { message: String },
}
```

状态转换规则：

```mermaid
stateDiagram-v2
    [*] --> Ready: Demo / Memory
    [*] --> Loading: Postgres / Cloud
    [*] --> LoadFailed: Invalid options
    Loading --> Ready: PostgreSQL runtime loaded
    Loading --> LoadFailed: cold start failed
    Ready --> Ready: editing / scroll / selection
```

`CditorV2View` 暴露内部状态切换 API，供异步加载任务和后续 persistence queue 回调使用：

```rust
pub fn apply_loaded_runtime(&mut self, runtime: DocumentRuntime)
pub fn apply_load_failed(&mut self, message: impl Into<String>)
pub fn view_state(&self) -> &CditorViewState
```

设计约束：

- `Loading` 和 `LoadFailed` 状态下不处理编辑命令。
- `Ready` 状态下才允许滚动、输入、selection、overlay 投影。
- 错误态必须可恢复显示，不能 panic 或白屏。

### 5.3 PostgreSQL Cold Start 时序

PostgreSQL cold start 不能阻塞 GPUI 首帧。当前落地方式：

1. `build_view(cx)` 先返回 `CditorV2View::loading_with_options(...)`。
2. 同时调用 `spawn_postgres_cold_start(options, cx)`。
3. GPUI `background_spawn` 中创建独立 tokio runtime。
4. tokio runtime 内执行 `load_runtime_from_options(&options)`。
5. 加载成功后通过 GPUI foreground task 更新 view：`Loading -> Ready(runtime)`。
6. 加载失败后更新 view：`Loading -> LoadFailed(message)`。

时序图：

```mermaid
sequenceDiagram
    participant User as User App
    participant Cditor as Cditor::build_view
    participant View as CditorV2View
    participant Bg as GPUI background task
    participant Pg as PostgreSQL

    User->>Cditor: build_view(cx)
    Cditor->>Bg: spawn_postgres_cold_start(options)
    Cditor-->>User: Loading View first frame
    Bg->>Bg: create tokio runtime
    Bg->>Pg: load metadata/index/layout/payload window
    Pg-->>Bg: DocumentRuntime + report
    Bg->>View: apply_loaded_runtime(runtime)
    View-->>User: Ready editor view
```

为什么后台任务里还要创建 tokio runtime：

- `sqlx` 使用 tokio runtime。
- GPUI foreground executor 不等价于 tokio runtime。
- 直接在 UI 线程里 `block_on` 会破坏首帧体验。
- 独立 tokio runtime 可把数据库 IO 和 GPUI render 解耦。

当前只加载首屏 payload window，不加载全量 payload，避免 10w block 文档打开时 IO 和内存爆炸。

### 5.4 Loading / Error shell

加载态和错误态 UI 放在：

```txt
src/gui/persistence/save_indicator.rs
```

当前抽象：

```rust
pub enum EditorLoadStateLabel {
    Loading(String),
    Failed(String),
}

pub fn render_load_state(label: &EditorLoadStateLabel, theme: GuiTheme) -> AnyElement
```

规则：

- Loading shell 用稳定页面结构居中显示，不依赖 document projection。
- Failed shell 显示错误 message，不吞掉错误。
- 后续可以在 Failed shell 上加“重试 / 切换数据源 / 打开本地副本”。

### 5.5 Save Status / 只读模式

当前已落地保存状态模型，但还没有接入真实 PostgreSQL 写入 worker：

```rust
pub enum EditorSaveStatus {
    Clean,
    Dirty,
    Saving,
    Failed(String),
    Readonly,
}
```

含义：

| 状态 | 含义 | 是否阻止静默关闭 |
| --- | --- | --- |
| `Clean` | 内存状态已保存 | 否 |
| `Dirty` | runtime 已修改但尚未保存 | 是 |
| `Saving` | 后台保存中 | 是 |
| `Failed(message)` | 保存失败，内存状态不能丢 | 是 |
| `Readonly` | 只读模式，不执行编辑命令 | 否 |

当前行为：

- 顶部 header 显示 save indicator。
- `with_readonly(true)` 会让 view 初始化为 `Readonly`。
- 只读模式下编辑命令直接忽略，不改 runtime。
- 非只读模式下，成功执行编辑命令后标记 `Dirty`。

当前代码落点：

```txt
src/gui/persistence/save_indicator.rs  # EditorSaveStatus / render_save_indicator
src/gui/app/cditor_v2_view.rs          # save_status 状态与输入后 Dirty 标记
```

### 5.6 后续 Persistence Queue 接入边界

下一步 `GPUI-063` 才会接真实 PostgreSQL persistence queue。预期设计：

```mermaid
flowchart TD
    A[用户输入] --> B[DocumentRuntime 立即更新]
    B --> C[UI 标记 Dirty]
    C --> D[debounce / coalesce]
    D --> E[后台保存任务]
    E --> F{保存结果}
    F -->|成功| G[UI 标记 Clean]
    F -->|失败| H[UI 标记 Failed(message)]
```

关键约束：

- 输入 hot path 不同步等待 PostgreSQL。
- 保存失败不能回滚或丢弃内存编辑状态。
- persistence queue 只接收 runtime 已产生的变更快照 / transaction，不成为 UI 真相。
- close guard 后续只读 `EditorSaveStatus::is_blocking_close()`，不直接查数据库。

---

## 6. 实施任务清单

### GPUI-Phase A：Public API Facade

- [x] GPUI-API-001 新增 `src/api` 模块。
- [x] GPUI-API-002 实现 `Cditor::new()` builder。
- [x] GPUI-API-003 实现 `demo()`、`with_document_id()`、`with_workspace_id()`、`with_postgres_url()`、`with_debug_overlay()`、`with_payload_window_size()`。
- [x] GPUI-API-004 实现 `build_view(cx)`，第一版支持 demo backend。
- [x] GPUI-API-005 `main.rs` 改为通过 `Cditor` facade 启动。
- [x] GPUI-API-006 添加 API builder 单元测试。

验收：

```sh
cargo fmt
cargo test api --lib
cargo check
```

---

### GPUI-Phase 0：工程目录重组

- [x] GPUI-000 新增并维护本任务清单。
- [x] GPUI-001 建立 `src/gui/app`，把根 View 放到 `app/cditor_v2_view.rs`。
- [x] GPUI-002 将 `src/gui/text_element` 迁移为 `src/gui/text`。
- [x] GPUI-003 建立 `src/gui/{document,block,input,scroll,overlay,persistence}` 模块骨架。
- [x] GPUI-004 `src/gui/mod.rs` 只导出稳定 GUI API：`CditorV2View`、`GuiTheme`。
- [x] GPUI-005 验证迁移后 `cargo test gui --lib` 与 `cargo check` 通过。

验收：

```sh
cargo fmt
cargo test gui --lib
cargo check
```

---

### GPUI-Phase 1：Root View / Document View 拆分

- [x] GPUI-010 `CditorV2View` 只保留 window/root/focus/debug 状态。
- [x] GPUI-011 新增 `DocumentEditorView` 或纯 render helper，负责文档 surface 和 render window。
- [x] GPUI-012 新增 `DocumentSurface`，统一 page width、padding、background、before/after spacer。
- [x] GPUI-013 debug header 独立为 `overlay/debug_overlay.rs` 或 `document/debug_header.rs`。
- [x] GPUI-014 `CditorV2View::render` 不再直接包含 block render 细节。

验收：

```sh
cargo test gui::app gui::document --lib
cargo check
```

---

### GPUI-Phase 2：Block View 拆分

- [x] GPUI-020 新增 `block/block_view.rs`，渲染单个 `ViewBlockSnapshot`。
- [x] GPUI-021 新增 `block/block_shell.rs`，处理 focus/selected/hover/indent/pin 装饰。
- [x] GPUI-022 新增 `block/block_content.rs`，根据 `RichBlockKind` / `BlockPayloadView` 分发内容渲染。
- [x] GPUI-023 拆出 paragraph/heading/list/quote/divider/code 基础 renderer。
- [x] GPUI-024 保持 placeholder/loading/error block 有稳定高度与明确 UI。

验收：

```sh
cargo test gui::block --lib
cargo check
```

---

### GPUI-Phase 3：Input Adapter

- [x] GPUI-030 新增 `input/command.rs`，定义 GPUI 输入到 runtime 的命令枚举。
- [x] GPUI-031 `keyboard.rs` 接管 `KeyDownEvent` 映射。
- [x] GPUI-032 `mouse.rs` 接管 focus block / hit-test 入口。
- [x] GPUI-033 `clipboard.rs` 预留 copy/cut/paste adapter。
- [x] GPUI-034 `ime.rs` 预留 composition adapter。

验收：

```sh
cargo test gui::input --lib
cargo check
```

---

### GPUI-Phase 4：Selection / Caret Overlay

- [x] GPUI-040 caret 绘制从 text element 内部迁移到 overlay 层。
- [x] GPUI-041 selection fragment 渲染不依赖 UI entity 存活。
- [x] GPUI-042 鼠标拖选更新 `DocumentRuntime` selection。
- [x] GPUI-043 selection 跨 render window 边界时可稳定显示可见 fragment。

验收：

```sh
cargo test selection gui::overlay --lib
cargo check
```

---

### GPUI-Phase 5：PostgreSQL Cold Start 接入

- [x] GPUI-050 `CditorBackend::PostgresUrl/PostgresPool` 可创建后台 store。
- [x] GPUI-051 GUI 首帧显示 loading shell，不阻塞 window 打开。
- [x] GPUI-052 后台加载 `DocumentRuntime::from_store`。
- [x] GPUI-053 首屏 payload window 加载完成后 swap 到编辑视图。
- [x] GPUI-054 加载失败显示 recoverable error view。

验收：

```sh
cargo fmt
cargo test api --lib
cargo test gui --lib
CDITOR_TEST_DATABASE_URL=postgres://cditor:cditor@localhost:5433/cditor_test \
  cargo test cditor_postgres_pool_options_load_runtime_from_store --lib -- --ignored
cargo check
```

当前说明：`PostgresUrl/PostgresPool` 已可通过 `CditorPostgresStores` 和 `load_runtime_from_options` 从 PostgreSQL 加载 `DocumentRuntime`；GUI facade 已能在 PostgreSQL / Cloud 后端先返回 loading shell，在缺少 `document_id` 时返回 recoverable failed shell。PostgreSQL backend 会在 GPUI `background_spawn` 内创建 tokio runtime 执行 `DocumentRuntime::from_store`，加载完成后通过 GPUI foreground task 将 `CditorV2View` 从 `Loading` swap 到 `Ready`；失败则 swap 到 `LoadFailed`。

---

### GPUI-Phase 6：Persistence UI / Save Pipeline 接入

- [x] GPUI-060 新增 editor save status 模型：`Clean/Dirty/Saving/Failed/Readonly`。
- [x] GPUI-061 顶部 header 显示 save indicator。
- [x] GPUI-062 GUI 输入修改 runtime 后标记 `Dirty`，只读模式不执行编辑命令。
- [ ] GPUI-063 接入 PostgreSQL persistence queue：dirty transaction 后异步保存。
- [ ] GPUI-064 保存成功将 indicator 置回 `Clean`，保存中显示 `Saving`。
- [ ] GPUI-065 保存失败显示 `Failed(message)`，且不丢失内存编辑状态。
- [ ] GPUI-066 close guard 根据 `EditorSaveStatus::is_blocking_close()` 阻止静默关闭。

验收：

```sh
cargo fmt
cargo test gui::persistence --lib
cargo test gui::app --lib
cargo test api --lib
cargo check
```

当前说明：Phase 6 已先落地纯 UI / state adapter，不把输入 hot path 直接耦合到数据库写入。后续 `GPUI-063` 起会把已有 PostgreSQL `PersistenceQueue` 接到 `CditorV2View`，以后台任务写库并通过 UI 状态回传保存结果。

---

### GPUI-Phase 7：IME / Composition 完整编辑体验

参照 V1 `/Users/jychen/Desktop/Cditor` 的实现迁移，不做“最小 IME”：

V1 关键设计：

- `EntityInputHandler` 接系统输入法。
- `replace_and_mark_text_in_range` 只更新 composition preview，不 dirty 文档。
- `replace_text_in_range` / commit 才写入正文并 dirty。
- 渲染层通过 `text_override + marked_range` 显示预编辑文本。
- marked range 用下划线显示。
- 提供 UTF16 / UTF8 offset 转换，适配系统 IME 的 UTF16 range。

V2 落地状态：

- [x] GPUI-070 新增 `gui/input/ime.rs`，提供 UTF16/UTF8 offset/range 转换。
- [x] GPUI-071 `DocumentRuntime` 暴露 focused text / platform input text / active composition / marked range 查询。
- [x] GPUI-072 projection 对 active composition block 注入 preview payload 和 `marked_range`。
- [x] GPUI-073 `RichTextElement` 支持 marked range 下划线渲染。
- [x] GPUI-074 `CditorV2View` 实现 `EntityInputHandler`：`text_for_range`、`selected_text_range`、`marked_text_range`、`unmark_text`、`replace_text_in_range`、`replace_and_mark_text_in_range`、`bounds_for_range`、`character_index_for_point`。
- [x] GPUI-075 通过自定义 `CditorImeInputElement` 在 GPUI `paint()` 阶段调用 `window.handle_input(... ElementInputHandler ...)` 注册系统 IME handler。
- [x] GPUI-076 composition preview 不写 payload，commit / replace 才写 runtime 并标记 `Dirty`。
- [ ] GPUI-077 将 IME candidate rect 从当前 root-level 估算升级为 text element bounds 精确定位。
- [ ] GPUI-078 增加手动验收记录：macOS 中文输入法连续输入、选词、取消、替换 selection、emoji/surrogate pair。

验收：

```sh
cargo fmt
cargo test gui --lib
cargo test runtime::document_runtime --lib
cargo check
```

当前说明：IME 已按 V1 的 document-level `EntityInputHandler` 语义接入。注意：`window.handle_input` 只能在 GPUI 自定义 Element 的 `paint()` 阶段调用，不能在 `Render::render()` 里调用；当前通过零尺寸 `CditorImeInputElement` 专门承载 input handler 注册，避免启动时 panic。由于 V2 当前 `RichTextElement` 仍是普通 `div` 渲染，candidate rect 先用 root-level bounds 估算。后续 `GPUI-077` 会把 `RichTextElement` 升级为自定义 Element，在 text element paint 阶段像 V1 一样注册 `window.handle_input`，获得精确 candidate rect。

---

## 7. 当前执行策略

当前已完成 Public API、GUI 拆分、selection/caret overlay、PostgreSQL cold start。接下来按以下顺序推进：

1. 完成 Phase 6 persistence queue 与 close guard。
2. 接入更多 GPUI 编辑器交互：slash menu、clipboard、IME。
3. 做 PostgreSQL 打开/编辑/保存/重开端到端验收。
