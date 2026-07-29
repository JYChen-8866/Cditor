# Cditor 组件 SDK API 设计

> 状态：Implemented
>
> 当前实现：`crates/cditor-sdk`、`crates/cditor-editor-gpui/src/component_sdk`、
> `apps/cditor-desktop`

## 1. 设计结论

Cditor 的公共面分为三层，不能重新合并成一个依赖所有实现的 API crate：

```text
cditor-sdk                 framework-free contract/configuration
        ^
        |
cditor-editor-gpui         GPUI component, handle and view contract
        ^
        |
cditor-desktop             concrete composition factory and executable
```

- SDK 保存稳定的 owned value、command/event、options、diagnostics 和 provider contract。
- GPUI Editor 保存 `Entity`、`WeakEntity`、`Task`、focus/window 和可渲染组件。
- Desktop 选择 SQLite/PostgreSQL、OpenAI-compatible、远程图片和 GPUI application。
- Runtime 和 Session 不依赖 SDK 或 Desktop；它们通过 Editor Protocol 和 port 交互。

## 2. Crate 责任

### `cditor-sdk`

公开模块：

```text
cditor-sdk/src/
  cditor.rs          Cditor 配置对象
  command.rs         public command facade
  diagnostics.rs     诊断快照
  document.rs        文档信息、选区、保存状态、close guard
  error.rs           CditorError
  event.rs           host event
  import_export.rs   import/export options
  options.rs         backend-neutral options
  providers.rs       AI/asset/host/translation/extension contract
```

禁止依赖 GPUI、Runtime、具体 Storage adapter、SQLx、HTTP client、环境变量和白板实现。
`Cditor` 只保存 options 和注入的 provider，不构造 View，不打开连接。

### `cditor-editor-gpui`

公开：

- `CditorComponent<V>`：可渲染 Entity 与非保留 Handle；
- `CditorHandle<V>`：通过 `WeakEntity` 控制 View；
- `CditorViewContract`：Handle 所需的小型 UI contract；
- `CditorViewFactory`：composition root 的构造边界；
- `CditorV2View`：默认 GPUI 实现；
- `bind_cditor_keys`：每个 GPUI App 一次的键位注册。

GPUI 类型只允许从这一层及 Desktop 暴露。Handle 不持有 Runtime 或 StorageSession。

### `cditor-desktop`

公开的可复用组装面仅包括：

- `wiring::build_component`；
- `wiring::run_desktop`；
- `CditorStorageExt`；
- SQLite/PostgreSQL 具体 option/provider 的显式入口。

Desktop 不重新导出整个 Core、Runtime、Editor、SDK 或 Storage crate，避免形成第二个总 API。

## 3. 构造流程

```text
Cditor options
  -> Desktop validates target
  -> concrete StorageProvider opens DocumentStorage
  -> cold-start creates EditorSession
  -> Session serially owns DocumentRuntime
  -> CditorV2View receives EditorSessionHandle
  -> CditorComponent returns Entity + WeakEntity Handle
```

无效配置在构造前返回 `CditorError::InvalidInput`。构造过程中不能暴露半初始化 Runtime。

## 4. 命令与读取

- 所有 mutation 通过 typed `CditorCommand`/Editor Protocol dispatch。
- command state 与 execute 共享同一 capability/precondition 语义。
- 文档读取通过 immutable info/selection/diagnostics/projection，不返回 Runtime 引用。
- save/flush 返回 Task，调用方必须等待完成结果。
- 高频输入和 IME 不经过外部回调、serde 或异步 channel。

## 5. 生命周期

Handle 的状态结果必须区分：

- `ComponentDropped`：WeakEntity 无法升级；
- `NotReady`：cold start 尚未完成；
- `Readonly`：compatibility 或 host policy 拒绝 mutation；
- `Unsupported`：backend/command 不支持；
- `Internal`：composition 或运行错误。

窗口关闭前，宿主读取 close guard；需要保存时等待 flush Task。销毁 View 后 Handle 不延长其
生命周期。

## 6. Provider 边界

- `StorageProvider` 属于 `cditor-storage`，SDK 只接受 trait object。
- `AiProvider` 属于 `cditor-ai`，HTTP 实现在 `cditor-ai-openai`。
- asset、host delegate、translation 和 extension contract 使用 owned DTO。
- provider 回调不得在输入/布局热路径执行阻塞 I/O。
- 环境配置和具体 provider 选择只在 Desktop 或宿主 composition root。

## 7. Semver

受 semver 保护的表面：公开 type、method、command/event ID、错误类别和 provider trait。
不承诺稳定的表面：`CditorV2View` 私有字段、Runtime 内部状态、布局缓存、GPUI 元素树和
Storage adapter 内部 row 类型。

SDK compile contract 位于 `crates/cditor-sdk/tests/public_api.rs`，必须证明外部消费者只使用
framework-free API 即可编译。

## 8. 验收门禁

```sh
cargo test -p cditor-sdk --all-targets
cargo test -p cditor-desktop --test component_sdk
cargo clippy -p cditor-sdk -p cditor-editor-gpui -p cditor-desktop \
  --all-targets -- -D warnings
scripts/dev/check_structure.sh
```

结构门禁禁止 SDK 引入 GPUI/SQLx/Runtime/具体 adapter，禁止 Desktop 恢复 broad re-export，
并禁止旧 `cditor-api`/`cditor-app` 路径返回。
