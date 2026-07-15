# Cditor 组件 SDK 接口设计

## 1. 文档定位

本文定义 Cditor 从“可嵌入的 GPUI View”演进为稳定组件 SDK 时应抽取的公共接口、职责边界与实施优先级。

本文是接口设计方案，不代表所列 API 已经全部实现。当前已经可用的接口和集成示例参见 [Cditor 组件接口与集成指南](../guides/cditor-component-integration.md)。

设计必须遵循 [大文档富文本架构](../large-document-rich-text-architecture.md) 的核心约束：

> UI 只是当前视口的投影；文档、选区、布局高度和滚动状态的真源必须位于编辑器内核。

因此公共 SDK 应暴露命令、不可变快照、事件和 Provider，不应暴露内部可变 `DocumentRuntime`、布局缓存、虚拟视口 Entity 或持久化状态机。

## 2. 设计目标

- 让 GPUI 宿主能够创建、控制、观察和销毁编辑器组件。
- 让宿主不依赖 `CditorV2View` 的内部字段和 `pub(crate)` 方法。
- 所有内容修改统一经过事务，保证 Undo/Redo、选区、持久化和布局状态一致。
- 大文档操作可以异步、分片或流式执行，不阻塞 GPUI 主线程。
- 高频事件可以合并和限流，不让宿主回调拖慢输入与滚动热路径。
- 存储、AI、附件、主题和系统行为通过稳定 Provider 注入。
- 为将来的其他语言绑定保留纯数据边界，但本阶段仍以 Rust + GPUI 为主。

## 3. 推荐的公共模块

```text
cditor_app
├── CditorBuilder
├── CditorComponent
├── CditorHandle
├── CditorEvent
├── CditorCommand
├── CditorError
├── document/
│   ├── DocumentSource
│   ├── DocumentInfo
│   ├── DocumentSnapshot
│   └── DocumentSelection
├── import_export/
│   ├── MarkdownImportOptions
│   ├── MarkdownExportOptions
│   └── ExportFormat
├── provider/
│   ├── AssetProvider
│   ├── AiProvider
│   ├── ThemeProvider
│   └── CditorHostDelegate
└── diagnostics/
    └── CditorDiagnostics
```

建议将公共 SDK 放在 `crates/app/src/api/` 下，按职责拆分文件，避免继续扩大单个 `cditor.rs`：

```text
crates/app/src/api/
├── mod.rs
├── builder.rs
├── component.rs
├── handle.rs
├── command.rs
├── event.rs
├── error.rs
├── document.rs
├── import_export.rs
├── providers.rs
└── diagnostics.rs
```

## 4. P0：组件控制基础

### 4.1 `CditorComponent` 与 `CditorHandle`

当前 `build_entity` 只返回 `Entity<CditorV2View>`，迫使宿主通过 View 类型控制组件。构建结果应同时包含可渲染 Entity 和稳定控制句柄：

```rust
pub struct CditorComponent {
    pub view: Entity<CditorV2View>,
    pub handle: CditorHandle,
}

#[derive(Clone)]
pub struct CditorHandle {
    entity: WeakEntity<CditorV2View>,
}
```

Handle 优先持有弱引用，防止宿主持有 Handle 时阻止 View 释放。View 已销毁时，操作统一返回 `CditorError::ComponentDropped`。

基础控制接口：

```rust
impl CditorHandle {
    pub fn focus(&self, cx: &mut App) -> Result<(), CditorError>;
    pub fn blur(&self, cx: &mut App) -> Result<(), CditorError>;

    pub fn is_ready(&self, cx: &App) -> bool;
    pub fn is_readonly(&self, cx: &App) -> bool;
    pub fn set_readonly(&self, readonly: bool, cx: &mut App)
        -> Result<(), CditorError>;

    pub fn undo(&self, cx: &mut App) -> Result<(), CditorError>;
    pub fn redo(&self, cx: &mut App) -> Result<(), CditorError>;
    pub fn can_undo(&self, cx: &App) -> bool;
    pub fn can_redo(&self, cx: &App) -> bool;
}
```

Builder 建议新增统一构建入口：

```rust
let component = CditorBuilder::new()
    .memory()
    .build(cx)?;
```

为了兼容现有使用方，`Cditor` 可以暂时作为 `CditorBuilder` 的类型别名，旧的 `build_view` 和 `build_entity` 在一个弃用周期后再移除。

### 4.2 文档生命周期

宿主需要在组件存活期间打开、切换、重载和关闭文档：

```rust
pub enum DocumentSource {
    Empty,
    Snapshot(DocumentSnapshot),
    PostgreSql { document_id: DocumentId },
    Markdown(String),
    Json(String),
}

pub enum ClosePolicy {
    RejectIfDirty,
    SaveThenClose,
    DiscardChanges,
}
```

```rust
impl CditorHandle {
    pub async fn open_document(
        &self,
        source: DocumentSource,
    ) -> Result<DocumentInfo, CditorError>;

    pub async fn reload(&self) -> Result<DocumentInfo, CditorError>;

    pub async fn close_document(
        &self,
        policy: ClosePolicy,
    ) -> Result<(), CditorError>;

    pub fn document_info(&self, cx: &App) -> Option<DocumentInfo>;
}
```

切换文档必须作为完整生命周期操作，依次处理：

1. 检查未保存内容。
2. 取消旧文档 AI、payload、布局、Mermaid 和附件异步任务。
3. 结束或等待持久化。
4. 清空 View 级缓存和浮层。
5. 创建新 runtime 并恢复加载状态。
6. 只在新文档 generation 仍有效时应用异步结果。

宿主不能直接调用 `apply_loaded_runtime` 替换 runtime。

### 4.3 保存与关闭保护

```rust
pub struct SaveReport {
    pub revision: u64,
    pub saved_blocks: usize,
    pub duration: Duration,
}

pub struct CloseGuard {
    pub dirty: bool,
    pub saving: bool,
    pub failed_operations: usize,
    pub can_close_safely: bool,
}
```

```rust
impl CditorHandle {
    pub async fn save(&self) -> Result<SaveReport, CditorError>;
    pub async fn flush(&self) -> Result<SaveReport, CditorError>;

    pub fn is_dirty(&self, cx: &App) -> bool;
    pub fn save_status(&self, cx: &App) -> SaveStatus;
    pub fn close_guard(&self, cx: &App) -> CloseGuard;
}
```

`save` 负责触发一次保存，`flush` 必须等待调用时已存在的结构、payload、属性和必要附件操作完成。两者语义不能混用。

## 5. P0：事件系统

### 5.1 事件类型

```rust
pub enum CditorEvent {
    LoadStarted {
        document_id: Option<DocumentId>,
    },
    LoadProgress {
        loaded: usize,
        total: Option<usize>,
    },
    Ready {
        document: DocumentInfo,
    },
    LoadFailed {
        error: CditorError,
    },

    ContentChanged {
        revision: u64,
        origin: ChangeOrigin,
    },
    SelectionChanged {
        selection: DocumentSelection,
    },
    FocusChanged {
        focused: bool,
    },

    SaveStarted {
        revision: u64,
    },
    SaveSucceeded {
        revision: u64,
    },
    SaveFailed {
        revision: u64,
        error: CditorError,
    },
    DirtyChanged {
        dirty: bool,
    },

    LinkActivated {
        url: String,
    },
    AssetActivated {
        asset: AssetDescriptor,
    },
}
```

`CditorV2View` 应实现：

```rust
impl EventEmitter<CditorEvent> for CditorV2View {}
```

宿主使用 GPUI 订阅：

```rust
cx.subscribe(&component.view, |host, _view, event, cx| {
    host.handle_cditor_event(event, cx);
});
```

### 5.2 事件性能约束

- `ContentChanged` 只携带 revision 和来源，不携带整个文档。
- 同一帧连续输入产生的变化允许合并。
- `SelectionChanged` 在鼠标拖选期间按帧合并，不逐平台事件通知。
- 滚动、布局高度修正、代码高亮和缓存写入不属于内容变化。
- 后台任务事件必须携带 document generation，过期结果不得发给新文档。
- 宿主事件处理不得运行在输入事务的内部可变借用期间。

## 6. P0：导入、导出与快照

### 6.1 内容格式接口

```rust
pub enum ExportFormat {
    Markdown,
    CditorJson,
    PlainText,
    Html,
}

pub struct ImportReport {
    pub inserted_blocks: usize,
    pub warnings: Vec<ImportWarning>,
}

pub struct ExportReport {
    pub blocks: usize,
    pub bytes: u64,
    pub warnings: Vec<ExportWarning>,
}
```

```rust
impl CditorHandle {
    pub async fn import_markdown(
        &self,
        markdown: String,
        options: MarkdownImportOptions,
    ) -> Result<ImportReport, CditorError>;

    pub async fn export_markdown(
        &self,
        options: MarkdownExportOptions,
    ) -> Result<String, CditorError>;

    pub async fn import_json(
        &self,
        json: String,
    ) -> Result<ImportReport, CditorError>;

    pub async fn export_json(&self) -> Result<String, CditorError>;
    pub async fn snapshot(&self) -> Result<DocumentSnapshot, CditorError>;
}
```

### 6.2 大文档约束

返回 `String` 的便捷方法适合小文档。生产级接口还应支持流式写出：

```rust
pub async fn export_to<W: AsyncWrite + Unpin>(
    &self,
    format: ExportFormat,
    writer: W,
) -> Result<ExportReport, CditorError>;
```

- 不能在 GPUI 主线程同步遍历 100,000 Block。
- 未加载 payload 应通过存储层分批读取，不应强行全部装入当前 runtime 热窗口。
- 导出应基于固定 revision 或明确报告导出期间发生的并发修改。
- 附件应支持引用导出、复制导出和跳过三种策略。

## 7. P1：统一命令系统

格式化、Block 操作和插入操作应统一为命令，避免给 View 增加大量专用方法：

```rust
pub enum CditorCommand {
    Undo,
    Redo,
    SelectAll,
    DeleteSelection,

    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrike,
    ToggleInlineCode,

    InsertBlock(BlockInput),
    TransformBlock(BlockTransform),
    DeleteSelectedBlocks,
    DuplicateSelectedBlocks,

    InsertTable { rows: usize, columns: usize },
    InsertImage,
    InsertWhiteboard,
    InsertMermaid,

    FoldHeading,
    UnfoldHeading,
}

pub struct CommandState {
    pub enabled: bool,
    pub active: bool,
    pub visible: bool,
}
```

```rust
impl CditorHandle {
    pub fn execute(
        &self,
        command: CditorCommand,
        cx: &mut App,
    ) -> Result<CommandOutcome, CditorError>;

    pub fn command_state(
        &self,
        command: &CditorCommand,
        cx: &App,
    ) -> CommandState;
}
```

内置菜单、Slash Menu、快捷键和宿主自定义工具栏最终都应调用同一命令层，避免产生四套行为逻辑。

## 8. P1：选区、位置与滚动

公共位置不能用无类型的整数：

```rust
pub enum TextOffset {
    Utf8Bytes(usize),
    Utf16CodeUnits(usize),
}

pub struct DocumentPosition {
    pub block_id: BlockId,
    pub offset: TextOffset,
    pub affinity: Affinity,
}

pub struct DocumentSelection {
    pub anchor: DocumentPosition,
    pub head: DocumentPosition,
}
```

```rust
impl CditorHandle {
    pub fn selection(&self, cx: &App) -> Option<DocumentSelection>;

    pub fn set_selection(
        &self,
        selection: DocumentSelection,
        cx: &mut App,
    ) -> Result<(), CditorError>;

    pub fn selected_text(&self, cx: &App) -> Option<String>;

    pub fn scroll_to_block(
        &self,
        block_id: BlockId,
        alignment: ScrollAlignment,
        cx: &mut App,
    ) -> Result<(), CditorError>;
}
```

滚动定位必须走虚拟滚动内核和 ScrollAnchor，不允许宿主直接修改像素偏移。

## 9. P1：Block 查询与事务修改

```rust
impl CditorHandle {
    pub async fn block(
        &self,
        id: BlockId,
    ) -> Result<Option<BlockSnapshot>, CditorError>;

    pub async fn blocks(
        &self,
        range: BlockRange,
    ) -> Result<Vec<BlockSnapshot>, CditorError>;

    pub fn insert_blocks(
        &self,
        position: InsertPosition,
        blocks: Vec<BlockInput>,
        cx: &mut App,
    ) -> Result<TransactionId, CditorError>;

    pub fn update_block(
        &self,
        id: BlockId,
        patch: BlockPatch,
        cx: &mut App,
    ) -> Result<TransactionId, CditorError>;

    pub fn delete_blocks(
        &self,
        ids: Vec<BlockId>,
        cx: &mut App,
    ) -> Result<TransactionId, CditorError>;
}
```

约束：

- 只返回不可变 `BlockSnapshot`，不返回内部可变引用。
- 所有修改生成事务和 revision。
- 批量操作必须是一个可撤销的逻辑事务。
- 大范围读取允许从存储层分片读取，不能依赖 Block 当前是否在视口。
- 修改必须同步更新选区、折叠、脏状态、索引和异步 generation。

## 10. P1：Provider 与宿主委托

### 10.1 AI Provider

项目已经定义 `AiProvider` trait，Builder 需要增加正式注入入口：

```rust
pub fn with_ai_provider(
    self,
    provider: Arc<dyn AiProvider>,
) -> Self;

pub fn without_ai(self) -> Self;
```

宿主控制接口：

```rust
pub async fn ask_ai(
    &self,
    request: AiRequest,
) -> Result<AiRequestId, CditorError>;

pub fn cancel_ai(
    &self,
    request_id: AiRequestId,
    cx: &mut App,
) -> Result<(), CditorError>;
```

API Key 和用户身份不应保存在文档或 View 中，应由 Provider 管理。

### 10.2 资源与附件 Provider

```rust
#[async_trait]
pub trait AssetProvider: Send + Sync {
    async fn import(
        &self,
        input: AssetInput,
    ) -> Result<AssetRef, AssetError>;

    async fn resolve(
        &self,
        asset: &AssetRef,
    ) -> Result<ResolvedAsset, AssetError>;

    async fn delete(
        &self,
        asset: &AssetRef,
    ) -> Result<(), AssetError>;
}
```

该接口应支持本地目录、PostgreSQL metadata、S3/OSS、HTTP CDN 和宿主自有附件系统。资源解析必须异步，并具备缓存、取消、大小限制和错误占位策略。

### 10.3 宿主系统委托

```rust
pub trait CditorHostDelegate: Send + Sync {
    fn open_link(&self, url: &str);
    fn open_file(&self, asset: &AssetRef);
    fn request_file_picker(&self, request: FilePickerRequest);
    fn show_context_menu(&self, context: MenuContext);
}
```

编辑器不应自行决定如何打开浏览器、文件选择器和外部附件。宿主委托能避免组件与具体产品壳层耦合。

## 11. P2：主题、字体与本地化

当前 View 使用固定浅色主题。公共接口应支持构建时和运行时配置：

```rust
.with_theme(theme)
.with_theme_provider(theme_provider)
.with_locale("zh-CN")
.with_translations(translation_provider)
```

```rust
handle.set_theme(theme, cx)?;
handle.set_locale(locale, cx)?;
```

主题必须统一覆盖：

- 文档背景、文字和链接。
- Block hover、选中和 gutter。
- 光标、选区和 IME 标记。
- 表格、菜单和浮层。
- 代码高亮主题。
- Mermaid、白板容器和媒体占位。
- 骨架图、加载态和错误态。
- 字体、字号、行高和页面宽度。

主题版本变化必须使相关布局缓存失效，但不能同步重排整个大文档。

## 12. P2：搜索与替换

```rust
pub async fn search(
    &self,
    query: SearchQuery,
) -> Result<SearchSession, CditorError>;

pub fn next_match(
    &self,
    session: SearchSessionId,
    cx: &mut App,
) -> Result<(), CditorError>;

pub async fn replace(
    &self,
    request: ReplaceRequest,
) -> Result<ReplaceReport, CditorError>;
```

搜索结果使用 `BlockId + TextRange`，不能引用视口 Entity。内存文档可以走 runtime 索引，PostgreSQL 文档可以走全文检索，但两种后端应返回统一结果类型。

全部替换必须生成可撤销事务；超大范围替换需要分批执行，但对用户仍表现为一个逻辑操作。

## 13. P2：白板接口

白板内部状态继续由 `ding-board` 管理，Cditor SDK 只暴露文档级操作：

```rust
pub trait WhiteboardProvider: Send + Sync {
    fn create_scene(&self) -> WhiteboardScene;
    fn load_scene(&self, id: WhiteboardId) -> WhiteboardScene;
    fn save_scene(&self, scene: WhiteboardScene);
}
```

```rust
impl CditorHandle {
    pub fn open_whiteboard(
        &self,
        block_id: BlockId,
        cx: &mut App,
    ) -> Result<(), CditorError>;

    pub fn close_whiteboard(&self, cx: &mut App)
        -> Result<(), CditorError>;

    pub async fn export_whiteboard(
        &self,
        block_id: BlockId,
        format: WhiteboardExportFormat,
    ) -> Result<Vec<u8>, CditorError>;
}
```

不要把 `ding-board` 内部 Entity、工具状态或撤销栈直接暴露给 Cditor 宿主。

## 14. P2：扩展系统

```rust
pub trait CditorExtension: Send + Sync {
    fn commands(&self) -> Vec<CommandDescriptor>;
    fn slash_items(&self) -> Vec<SlashItem>;
    fn toolbar_items(&self) -> Vec<ToolbarItem>;
}
```

第一阶段只建议开放：

- 自定义命令。
- Slash Menu 条目。
- Toolbar 条目。
- Embed Block。
- 自定义只读卡片。

暂不开放任意可编辑 Block renderer。第三方 renderer 如果无法正确报告高度、焦点、IME、选区和持久化数据，会直接破坏虚拟滚动与大文档性能。完整可编辑 Block 扩展应有独立协议和验收标准。

## 15. P2：诊断接口

```rust
pub struct CditorDiagnostics {
    pub document_blocks: usize,
    pub loaded_payloads: usize,
    pub rendered_blocks: usize,
    pub pending_layout_tasks: usize,
    pub pending_saves: usize,
    pub dirty_blocks: usize,
    pub estimated_document_height: f64,
    pub memory_estimate_bytes: u64,
}
```

```rust
impl CditorHandle {
    pub fn diagnostics(&self, cx: &App)
        -> Result<CditorDiagnostics, CditorError>;
}
```

诊断接口返回结构化快照，宿主不应解析 debug overlay 文本。性能事件应限流，并默认关闭详细 trace。

## 16. 错误模型

所有公共操作应使用统一错误类型：

```rust
pub enum CditorError {
    ComponentDropped,
    NotReady,
    Readonly,
    DocumentNotFound(DocumentId),
    BlockNotFound(BlockId),
    InvalidSelection,
    InvalidInput(String),
    Unsupported(String),
    Cancelled,
    Timeout,
    Persistence(String),
    Import(String),
    Export(String),
    Asset(String),
    Ai(String),
    Internal(String),
}
```

错误需要保留可供日志定位的 source，但公共显示文本应通过本地化层生成。组件不能在普通宿主输入错误时 panic。

## 17. 并发与线程模型

- GPUI View、FocusHandle 和命令执行留在 UI 线程。
- PostgreSQL、导入导出、AI、附件和大范围查询在后台任务执行。
- 后台任务只处理不可变快照或存储中性数据，不持有 View 可变引用。
- 所有异步结果携带 document generation、revision 或 task token。
- 应用结果前验证 generation，旧文档结果直接丢弃。
- 取消 `open_document`、关闭 View 或切换文档时，相关任务必须可取消。
- 不允许在输入、滚动和布局热路径同步等待网络或数据库。

## 18. API 版本与兼容性

- 稳定公共类型集中从 `cditor_app` 根模块或 `cditor_app::api` 导出。
- 不把 `gui::*`、存储实现类型和内部 runtime 类型承诺为稳定 API。
- 新字段优先通过 `#[non_exhaustive]` enum、Builder 或配置结构增加。
- 序列化文档格式必须有独立 schema version，不能直接等同 Rust struct 布局。
- 事件和命令应使用稳定标识，便于未来日志、远程控制和其他语言绑定。
- 弃用 API 至少保留一个发布周期，并提供迁移说明。

## 19. 测试要求

每一组公共接口都需要覆盖：

- 正常调用和错误调用。
- View 已销毁后的 Handle 行为。
- Loading、Ready、LoadFailed 三种状态。
- 只读模式拒绝修改。
- 文档切换时旧异步结果不会污染新文档。
- 事件顺序、去重和同帧合并。
- Undo/Redo 与命令系统一致。
- 保存失败后的 dirty 和 close guard 状态。
- 100,000 Block 导出、搜索和批量修改不阻塞 UI 热路径。
- Windows UTF-16、macOS IME、CJK、Emoji 和跨 Block 选区。
- Provider 超时、取消和失败降级。

公共 API 示例应进入可编译的 integration test 或 example crate，避免文档示例随 GPUI 升级失效。

## 20. 实施顺序

| 阶段 | 交付内容 | 目标 |
| --- | --- | --- |
| P0-1 | `CditorComponent`、`CditorHandle`、统一错误 | 宿主不再直接控制 View 内部实现 |
| P0-2 | `CditorEvent` | 宿主能够观察加载、内容和保存状态 |
| P0-3 | 文档打开、关闭、切换、保存、close guard | 形成完整文档生命周期 |
| P0-4 | Markdown/JSON 导入导出与快照 | 形成基本数据交换能力 |
| P1-1 | 统一命令与命令状态 | 内置 UI、快捷键和宿主 UI 共用行为层 |
| P1-2 | 选区、定位、滚动与 Block 事务接口 | 支持产品级宿主控制 |
| P1-3 | AI、附件与 Host Delegate | 消除产品壳层硬编码 |
| P2-1 | 主题、本地化、搜索 | 支持成熟产品定制 |
| P2-2 | 白板、扩展、诊断 | 支持生态与复杂集成 |

## 21. 明确禁止直接公开的内部能力

以下内容可以在 crate 内保持 `pub(crate)`，不应为了“接口多”而直接暴露：

- `&mut DocumentRuntime`。
- 可变 Block 和 payload 引用。
- 布局缓存、文本排版缓存和 Mermaid/代码高亮缓存。
- 当前虚拟窗口的 Entity 列表。
- 原始滚动像素和内部 ScrollAnchor 可变引用。
- PostgreSQL saver 与 payload window scheduler。
- 输入法平台 target、拖拽中间状态和菜单临时状态。
- `CditorV2View` 内部字段。

这些能力必须通过命令、快照、事件或诊断结构间接使用。这样才能保持内核真源、UI 投影和异步版本控制之间的边界。
