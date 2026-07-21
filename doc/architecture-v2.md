# Cditor 架构重构方案（基于代码分析的最终版）

> 2026-07-21 | 基于对 479 个 .rs 文件的逐文件分析
>
> 设计文档：`doc/large-document-rich-text-architecture.md`

---

## 目录

1. [核心矛盾修正](#1-核心矛盾修正)
2. [最终 crate 清单](#2-最终-crate-清单)
3. [依赖拓扑图](#3-依赖拓扑图)
4. [逐 crate 内部模块](#4-逐-crate-内部模块)
5. [文件到 crate 迁移对照表](#5-文件到-crate-迁移对照表)
6. [跨层依赖打破方案](#6-跨层依赖打破方案)
7. [crate 关键类型定义](#7-crate-关键类型定义)
8. [crate 配置 schema](#8-crate-配置-schema)
9. [事件目录](#9-事件目录)
10. [测试分布矩阵](#10-测试分布矩阵)
11. [线程模型与异步边界](#11-线程模型与异步边界)
12. [执行任务清单（85 项）](#12-执行任务清单)

[附录：关键决策汇总](#附录关键决策汇总)

---

## 1. 核心矛盾修正

原架构文档将 `cditor-editor` 定义为 **"GPUI UI 层"**，但经过代码分析发现：

| 实际情况 | 原定义 |
|---|---|
| `crates/editor/` 只依赖 `cditor-core + serde + serde_json`，**零 GPUI 依赖** | cditor-editor = GPUI UI 层 |
| 内容是 command catalog、scroll 算法、window planner、hit_test、debug_overlay——纯算法 + 数据结构 | —— |
| 真正的 GPUI UI 层在 `crates/app/src/gui/`（363 个 .rs 文件，依赖 gpui、swash、lumis、image、mermaid） | cditor-app = 应用组装 |

**修正后的定义：**

| crate | 实际身份 |
|---|---|
| `cditor-editor-core`（新） | **纯算法层**：命令系统、虚拟滚动、窗口规划、hit_test、调试覆盖层（来自旧 editor/） |
| `cditor-editor` | **GPUI UI 层**：block 渲染、文本引擎、输入处理、覆盖层、持久化桥接（来自旧 app/src/gui/） |
| `cditor-app` | **应用组装**：main.rs + wiring |

> **为什么拆成两个：** 同一 crate 内没有编译边界，Rust 不阻止算法代码 import GPUI 类型。拆开后 `cditor-editor-core` 保持零 GPUI 依赖，将来做 Web/TUI 编辑器可以直接复用。

### 1.1 落地复核与修正

目录拆分方向总体合理，但不能逐字照搬原方案。实际落地采用以下修正：

1. **保留主干编译边界。** `core -> editor-core/storage -> runtime -> editor -> app` 分离了领域真相、纯算法、活文档状态、GPUI 适配和进程组装，边界能由 Cargo 与结构门禁共同执行。
2. **`storage` 不拥有导航行为。** 全文索引只返回 `BlockId` 和匹配信息；“搜索结果如何滚动到 Block”属于 runtime/editor。原实现让 `cditor-storage -> cditor-editor-core`，已移除该反向依赖。
3. **主题只保留一个定义。** `GuiTheme` 的实现位于 `cditor-theme`；`cditor-editor::theme` 仅做兼容 re-export，避免迁移后形成两个会漂移的主题类型。
4. **`app` 不依赖所有 crate。** 组装层只声明可执行程序实际使用的直接依赖。要求 composition root 依赖全部 crate 会增加无意义的重编译和错误耦合。
5. **协同 crate 独立计数。** 当前 workspace 是 16 个本次目录重构范围内的 crate，加一个预留的 `cditor-collaboration`，合计 17 个；本次不实现协同功能。
6. **`theme-types` 暂时保留但设观察点。** 它目前只被 `cditor-theme` 消费，单看现状拆分收益有限；保留它是为了未来宿主主题 token/schema 不依赖具体解析器。若下一阶段仍无第二消费者，应合并回 `cditor-theme`。
7. **API 构造不能用未落地的伪抽象。** 原文提出的 `CditorViewFactory` 尚不足以解决 GPUI `Entity` 类型擦除和组装所有权。稳定 API crate 应保存 options/command/event/handle 契约，具体 View 构造由 `cditor-app` 暴露；在真实构造入口完成前，不把相关任务标记完成。

结构门禁 `scripts/dev/check_structure.sh` 已同步到新路径，并禁止 core/runtime/text/editor-core/storage 重新跨越上述边界。

---

## 2. 最终 crate 清单

```
Cditor/
├── Cargo.toml
├── crates/
│   ├── cditor-core/              # 纯领域模型（零项目内依赖）
│   ├── cditor-theme-types/       # 主题 token 类型
│   ├── cditor-theme/             # 主题解析 + GuiTheme
│   ├── cditor-storage/           # 存储接口 + mapper + FTS 查询
│   ├── cditor-storage-postgres/  # PostgreSQL 实现
│   ├── cditor-storage-sqlite/    # SQLite 实现
│   ├── cditor-runtime/           # 活文档状态 + 编辑控制 + 布局调度
│   ├── cditor-editor-core/       # 纯算法层（scroll/window/hit_test/command，零 GPUI 依赖）
│   ├── cditor-editor/            # GPUI UI 层（block/text/input/overlay/app）
│   ├── cditor-import-export/     # Markdown/HTML/CSV 导入导出 + 剪贴板格式 + 安全过滤
│   ├── cditor-api/               # 对外 SDK API
│   ├── cditor-app/               # 应用组装（main.rs + wiring）
│   ├── cditor-ai/                # AI provider 抽象（独立）
│   ├── cditor-text/              # 框架无关文本布局（Parley 适配）
│   ├── cditor-test-support/      # 测试工具 + acceptance fixtures
│   └── ding-board/               # 可嵌入白板组件（独立）
├── migrations/
│   └── postgres/
├── assets/
│   └── themes/
├── examples/
├── tools/
└── fixtures/
```

本次重构范围共 16 个 crate；workspace 另含预留的 `cditor-collaboration`，实际合计 17 个 crate。

---

## 3. 依赖拓扑图

```
                        ┌────────────────────┐
                        │  cditor-theme-types  │  ← 仅 serde
                        └─────────┬──────────┘
                                  │
                        ┌─────────▼──────────┐
                        │    cditor-theme      │
                        │  (GuiTheme 在此)     │
                        └─────────────────────┘

    ┌──────────┐        ┌────────────────────┐
    │ cditor-ai │        │     cditor-core     │  ← 零项目内依赖
    │  (独立)   │        │  block / document   │
    │          │        │  edit / layout       │
    │          │        │  schema / telemetry  │
    │          │        │  identity / fixtures  │
    └────┬─────┘        └──┬──────┬──────┬─────┘
         │                 │      │      │
         │     ┌───────────┤      │      ├──────────────┐
         │     │           │      │      │              │
         │  ┌──▼──────┐ ┌──▼──────────▼──┐ ┌─▼──────────┐
         │  │cditor-  │ │cditor-editor-  │ │cditor-     │
         │  │text     │ │core ★          │ │storage     │
         │  │(Parley) │ │(纯算法层)      │ │(接口+mapper │
         │  └─────────┘ │scroll/window   │ └─┬────┬─────┘
         │              │hit_test/command│   │    │
         │              └──────┬─────────┘ ┌──▼──┐ ┌▼───────▼──┐
         │                     │           │stor │ │stor-sqlite│
         │                     │           │-pg  │ └───────────┘
         │                     │           └─────┘
         │  ┌──────────────────┤
         │  │                  │
         │  │  ┌───────────────▼──────────┐
         │  │  │    cditor-runtime         │
         │  │  │  document_runtime         │
         │  │  │  editing / scheduling     │
         │  │  │  projection / content     │
         │  │  └─────────────┬────────────┘
         │  │                │
         │  │  ┌─────────────┼───────────┐
         │  │  │             │           │
    ┌────▼──▼──▼──┐  ┌──────▼──────┐  ┌──▼──────────────┐
    │cditor-      │  │cditor-      │  │cditor-api        │
    │import-      │  │editor ★     │  │(Cditor/Cditor-   │
    │export       │  │(GPUI UI层)  │  │ Handle/Cditor-   │
    │(markdown/   │  │block/text   │  │ Command/Cditor-  │
    │ paste/      │  │input/       │  │ Event/Config)    │
    │ security)   │  │overlay/app  │  └───────┬──────────┘
    └─────────────┘  └──────┬──────┘          │
                             │         ┌──────▼──────────┐
                             │         │cditor-test-     │
                             │         │support          │
                             │         └─────────────────┘
                      ┌──────▼──────────────────┐
                      │      cditor-app          │
                      │   main.rs + wiring       │
                      └─────────────────────────┘
```

**依赖深度：** `core(0) → editor-core/storage(1) → runtime/import-export(2) → editor/api(3) → app(4)`
## 4. 逐 crate 内部模块

### cditor-core（~56 个 .rs 文件）

```
crates/cditor-core/src/
├── lib.rs
├── ids.rs                    # BlockId/DocumentId/WorkspaceId/SurfaceId
├── version.rs                # SnapshotIdentity/StructureVersion/LayoutVersionNumber
├── demo_fixtures.rs          # #[cfg(feature = "fixtures")]
│
├── block/
│   ├── mod.rs
│   ├── list_info.rs          # BlockListInfo, is_list_item_kind
│   ├── input_capability.rs   # BlockInputCapability, BlockKeyboardPolicy
│   ├── chrome.rs             # BlockChromeSnapshot, BlockPrefixSnapshot
│   └── drag.rs               # GutterBlockDragState, BlockDropTarget
│
├── document/
│   ├── mod.rs
│   ├── index.rs              # DocumentIndex (SoA), BlockIndexRecord, DocumentIndexStore trait
│   └── visible_index.rs      # VisibleDocumentIndex, VisibilityUpdate
│
├── edit/
│   ├── mod.rs                # TransactionId, SnapshotId, TextOffset, ScrollAnchor
│   ├── selection.rs          # DocumentSelection, NormalizedSelection, SelectionRange, TextPosition
│   ├── transactions.rs       # EditTransaction, EditOperation, TableEditOperation
│   ├── origin.rs             # ChangeOrigin (User/Ime/Undo/Redo/Remote/Ai/Plugin)
│   ├── undo.rs               # UndoStack, UndoStep, UndoGroupingPolicy
│   ├── domain_operations.rs  # TextEditOperation, BlockEditOperation, CollectionEditOperation
│   ├── text_offsets.rs       # TextOffsetMap (UTF-8↔UTF-16↔grapheme)
│   ├── transaction_codec.rs  # encode_transaction/decode_transaction
│   ├── selection_tests.rs
│   └── undo_tests.rs
│
├── rich_text/
│   ├── mod.rs
│   ├── block_kind.rs         # RichBlockKind (30 variants)
│   ├── payload.rs            # BlockPayload, BlockPayloadRecord, ImagePayload, CollectionPayload
│   ├── document.rs           # RichTextDocument, RichBlockRecord, DocumentMetadata
│   ├── attrs.rs              # BlockAttrs (color/align/indent/folded/locked)
│   ├── inline.rs             # InlineSpan, InlineMark (Bold/Italic/Code/Link/Color)
│   ├── table.rs              # TablePayload, TableCellPayload, TableRange, TableCellMerge
│   ├── span_splice.rs        # splice_spans_at_range
│   └── table/
│       ├── structure.rs      # 表格行列增删、合并拆分操作
│       ├── style.rs          # TableCellStyle, TableHeaderStyle
│       └── tests.rs
│
├── layout/
│   ├── mod.rs
│   ├── block_layout.rs       # BlockLayoutMeta
│   ├── block_metrics.rs      # estimate_block_height, estimate_text_payload_height
│   ├── block_provider.rs     # BlockLayoutProvider trait + 各实现
│   ├── block_editor_model.rs # BlockEditorModel trait (Code/Table/Canvas)
│   ├── height_index.rs       # BlockHeightIndex (Fenwick Tree)
│   ├── page_layout.rs        # PageLayout, PagePolicy, PageLayoutIndex
│   └── page_layout_property_tests.rs
│
├── schema/
│   ├── mod.rs                # SchemaVersion, SchemaDomain, ReadPolicy
│   ├── envelope.rs           # VersionedEnvelope (无损前向兼容)
│   └── registry.rs           # BlockRegistry, BlockDescriptor, BlockCapabilities
│
├── telemetry/
│   ├── mod.rs                # TelemetryRecord, TelemetryEvent, TraceContext
│   ├── input.rs              # InputEvent
│   ├── layout.rs             # LayoutEvent
│   ├── storage.rs            # StorageEvent
│   └── sync.rs               # SyncEvent
│
├── identity/
│   ├── mod.rs
│   ├── persistent_id.rs      # PersistentId (128位 UUIDv7), BlockUid, DocumentUid
│   ├── generator.rs          # PersistentIdGenerator (RFC 9562 Method 3)
│   ├── arena.rs              # IdArena (PersistentId ↔ RuntimeHandle)
│   ├── legacy_map.rs         # LegacyIdMap (u64 → PersistentId 迁移)
│   └── order_key.rs          # OrderKey (Fractional Indexing)
│
└── fixtures/                 # #[cfg(feature = "fixtures")]
    ├── mod.rs                # FixtureManifest, document_semantic_checksum
    ├── bidi.rs               # Bidi 压力测试
    ├── code.rs               # 代码块压力 fixture (10MiB+)
    └── table.rs              # 表格压力 fixture (50k rows)
```

### cditor-theme-types（~6 个 .rs 文件）

```
crates/cditor-theme-types/src/
├── lib.rs
├── color_token.rs
├── font_token.rs
├── spacing_token.rs
├── radius_token.rs
├── border_token.rs
└── icon_token.rs
```

### cditor-theme（~7 个 .rs 文件）

```
crates/cditor-theme/src/
├── lib.rs
├── theme.rs               # GuiTheme (30+ 颜色字段)
├── colors.rs
├── typography.rs
├── metrics.rs
├── resolver.rs
├── default_theme.rs
└── user_theme.rs
```

### cditor-storage（~12 个 .rs 文件）

```
crates/cditor-storage/src/
├── lib.rs
├── error.rs               # StorageError/StorageResult
├── traits.rs              # re-export DocumentIndexStore
├── version.rs             # DOCUMENT_INDEX_VISIBLE_VERSION
├── backend.rs             # DocumentStorage trait, StorageSession
├── page_layout_snapshot.rs
├── layout_cache.rs        # LayoutCacheKey, BlockLayoutRow
├── cache_recovery.rs      # CacheRecoveryPlanner
├── height_write_debounce.rs # HeightWriteDebouncer + HeightWriteSink trait
├── optimistic_persistence.rs # OptimisticPersistenceManager
├── runtime.rs             # block_on_storage (tokio runtime)
│
├── mapper/
│   ├── mod.rs
│   ├── block_mapper.rs
│   ├── payload_mapper.rs
│   ├── layout_mapper.rs
│   ├── collection_mapper.rs
│   ├── comment_mapper.rs
│   ├── permission_mapper.rs
│   └── asset_mapper.rs
│
└── assets/
    ├── mod.rs
    ├── asset_store.rs
    ├── file_store.rs
    ├── thumbnail_store.rs
    └── media_cache.rs
```

**新增：** 从 `cditor-runtime` 迁入 `content/query_index.rs`（FTS 查询）。

### cditor-storage-postgres（~50 个 .rs 文件）

```
crates/cditor-storage-postgres/src/
├── lib.rs
├── pool.rs
├── error.rs
├── migrations.rs
├── transaction.rs
├── postgres_store.rs
│
├── repositories/
│   ├── mod.rs
│   ├── workspace_repo.rs
│   ├── page_tree_repo.rs
│   ├── document_repo.rs
│   ├── block_repo.rs
│   ├── block_payload_repo.rs
│   ├── block_attrs_repo.rs
│   ├── block_layout_repo.rs
│   ├── page_layout_repo.rs
│   ├── snapshot_repo.rs
│   ├── collection_repo.rs
│   ├── collection_cell_repo.rs
│   ├── collection_view_repo.rs
│   ├── asset_repo.rs
│   ├── search_repo.rs
│   ├── comment_repo.rs
│   ├── permission_repo.rs
│   ├── transaction_repo.rs
│   └── audit_repo.rs
│
└── loaders/
    ├── mod.rs
    ├── document_loader.rs
    ├── window_loader.rs
    ├── payload_loader.rs
    ├── layout_loader.rs
    ├── collection_window_loader.rs
    ├── comment_loader.rs
    └── permission_loader.rs
```

### cditor-storage-sqlite（~15 个 .rs 文件）

```
crates/cditor-storage-sqlite/src/
├── lib.rs
├── pool.rs
├── error.rs
├── migrations.rs
├── transaction.rs
├── sqlite_store.rs
├── repositories/     # 对应 pg 的 repo，但用 rusqlite
│   ├── block_repo.rs
│   ├── document_repo.rs
│   └── ...
└── loaders/
    ├── document_loader.rs
    └── ...
```

### cditor-runtime（~55 个 .rs 文件）

```
crates/cditor-runtime/src/
├── lib.rs
├── error.rs
├── handle.rs
├── runtime_event.rs
│
├── document_runtime/
│   ├── mod.rs
│   ├── state.rs              # DocumentRuntime struct (56 字段)
│   ├── constructors.rs       # empty(), demo(), large_mixed_demo()
│   ├── domain_state.rs       # 领域查询 (block_payload_record 等)
│   ├── cold_start.rs         # 冷启动编排
│   ├── ai.rs                 # RuntimeAiSession
│   ├── block_attrs.rs
│   ├── capabilities.rs       # TextSurfaceRegistry
│   ├── clipboard.rs
│   ├── clipboard_blocks.rs
│   ├── composition.rs
│   ├── focus.rs
│   ├── focus_transition.rs
│   ├── folding.rs
│   ├── format_transaction.rs
│   ├── inline_color.rs
│   ├── inline_format.rs
│   ├── layout_heights.rs
│   ├── local_transaction.rs
│   ├── markdown_paste.rs
│   ├── markdown_transaction.rs
│   ├── media.rs
│   ├── payload_cache.rs
│   ├── payload_hydration.rs
│   ├── payload_window.rs
│   ├── platform_text_edit.rs
│   ├── projection.rs
│   ├── queries.rs
│   ├── scroll.rs
│   ├── selection.rs
│   ├── selection_materialization.rs
│   ├── selection_transaction.rs
│   ├── selection_unified.rs
│   ├── slash_command.rs
│   ├── structure_delete.rs
│   ├── structure_edit.rs
│   ├── structure_index.rs
│   ├── structure_insert.rs
│   ├── structure_move.rs
│   ├── structure_payload.rs
│   ├── text_edit.rs
│   ├── text_navigation.rs
│   ├── text_payload.rs
│   ├── text_surface.rs
│   ├── text_target.rs
│   ├── transaction_apply.rs
│   ├── transaction_apply_domain.rs
│   ├── transaction_apply_domain_validation.rs
│   ├── transaction_apply_payload.rs
│   ├── transaction_apply_structure.rs
│   ├── typing_marks.rs
│   ├── undo_redo.rs
│   ├── whiteboard.rs
│   └── table/               # 表格运行时
│       ├── mod.rs
│       ├── clipboard.rs
│       ├── edit.rs
│       ├── input.rs
│       ├── layout.rs
│       ├── navigation.rs
│       ├── projection.rs
│       ├── reorder.rs
│       ├── resize.rs
│       ├── runtime.rs
│       ├── scroll.rs
│       ├── selection.rs
│       └── transaction.rs
│
├── editing/
│   ├── mod.rs
│   ├── composition.rs       # CompositionController
│   ├── hot_path.rs          # SingleCharInputHotPath + PieceTableTextModel
│   └── session.rs           # EditingSession + InputTarget
│
├── projection/
│   ├── mod.rs
│   ├── list.rs              # ListProjectionCache
│   └── view.rs              # EditorViewProjection + ViewBlockSnapshot
│
├── scheduling/
│   ├── mod.rs
│   ├── layout_scheduler.rs  # LayoutScheduler + LayoutTask + LayoutFrameResult
│   ├── main_thread_budget.rs # MainThreadBudget + InteractionMode
│   ├── worker_pool_policy.rs # WorkerPoolScheduler
│   └── async_version_control.rs # AsyncVersionController
│
└── content/
    ├── mod.rs
    ├── media_cache.rs       # MediaCache + MediaDecodeTrigger
    ├── payload_cache.rs     # PayloadCachePolicy (暂留)
    └── payload_window.rs    # PayloadWindow (DocumentRuntime 核心字段)
```

**迁出**：`paste_import.rs` + `security.rs` → cditor-import-export；`query_index.rs` → cditor-storage；`acceptance/` → cditor-test-support。

### cditor-editor-core（纯算法层，~10 个 .rs 文件）

> 来自旧 `crates/editor/`。零 GPUI 依赖，可被任何前端（GPUI/Web/TUI）复用。

```
crates/cditor-editor-core/src/
├── lib.rs
├── command.rs              # 命令系统 (CommandId, CommandInvocation, catalog)
├── scroll/
│   ├── mod.rs
│   ├── anchor.rs           # ScrollAnchor / CaretAnchor
│   ├── global_offset.rs    # GlobalOffsetMapper
│   ├── height_correction.rs # 高度修正管道
│   ├── scrollbar.rs        # 滚动条状态机
│   ├── virtual_scroll.rs   # VirtualScrollState
│   └── wheel.rs            # 滚轮管道
├── window/
│   ├── mod.rs
│   ├── render_window.rs    # 渲染窗口定义
│   ├── window_planner.rs   # WindowPlanner + hysteresis
│   └── window_commit.rs    # 窗口提交逻辑
├── hit_test.rs             # VisualLineLayout, CaretGeometryCache, Bidi hit-test
├── debug_overlay.rs        # DebugOverlaySnapshot, DebugOverlayViewModel
├── scroll_trace_replay.rs  # ScrollTraceReplay + RegressionGate
└── trace_event_log.rs      # TraceEventLog, TraceEventKind
```

**依赖：** 仅 `cditor-core + serde + serde_json + unicode-segmentation`。零 GPUI。

---

### cditor-editor（GPUI UI 层，~350 个 .rs 文件）

> 来自旧 `app/src/gui/`。依赖 GPUI + swash + lumis + image + mermaid_render。
> 依赖 `cditor-editor-core` 来使用纯算法（scroll/window/hit_test）。

```
crates/cditor-editor/src/
├── lib.rs
│
├── block/                  # block 类型视图（约 50 文件）
│   ├── mod.rs
│   ├── block_content.rs, block_shell.rs, block_view.rs
│   ├── chrome.rs, gutter.rs, prefix.rs, placeholder.rs
│   ├── paragraph.rs, heading.rs, quote.rs, list.rs
│   ├── media.rs, collection.rs, drag_overlay.rs, skeleton.rs
│   ├── code/               # 代码块高亮 + 工具栏
│   │   ├── mod.rs, highlight.rs, toolbar/
│   ├── mermaid/            # Mermaid 图表渲染
│   │   ├── mod.rs, cache.rs, render.rs, theme.rs
│   ├── table/              # 表格渲染（约 19 文件）
│   │   ├── mod.rs, cell.rs, grid.rs, render.rs
│   │   ├── cell_menu.rs, menu.rs, toolbar.rs
│   │   ├── reorder.rs, resize.rs, selection.rs, style.rs, text.rs
│   │   └── active_border.rs, axis_grip.rs, cell_gutter.rs, cell_handle.rs, chrome.rs
│   └── whiteboard/         # 白板渲染
│       ├── mod.rs, cache.rs, render.rs, style.rs
│
├── document/               # 文档级视图（5 文件）
│   ├── mod.rs
│   ├── document_editor_view.rs, document_surface.rs
│   ├── debug_header.rs, layout_metrics.rs, skeleton_window.rs
│
├── text/                   # 文本渲染引擎（约 12 文件）
│   ├── mod.rs
│   ├── element.rs          # RichTextElement (GPUI Element)
│   ├── parley_adapter/     # CachedParleyLayout, ParleyLayoutKey, paint
│   │   ├── mod.rs, exact_raster.rs, paint.rs
│   ├── geometry.rs         # TextCaretRect, TextHitPoint
│   ├── platform.rs, input.rs, background.rs, diagnostics.rs
│
├── input/                  # 输入处理（9 文件）
│   ├── mod.rs
│   ├── actions.rs          # bind_cditor_keys() 全局按键注册
│   ├── ime.rs, mouse.rs, clipboard.rs
│   ├── ai_prompt.rs, code_language.rs, command.rs
│   ├── platform_adapter.rs, single_line.rs
│
├── overlay/                # UI 覆盖层（约 14 文件）
│   ├── mod.rs
│   ├── slash_menu.rs, command_menu.rs, color_menu.rs
│   ├── floating_toolbar.rs, selection_overlay.rs
│   ├── block_transform_menu.rs, ai_inline.rs, toast.rs
│   ├── whiteboard_editor.rs
│   └── table/
│       ├── mod.rs, reorder.rs, resize.rs, scrollbar.rs
│
├── app/                    # 编辑器核心视图（约 30 文件，最核心）
│   ├── mod.rs
│   ├── cditor_v2_view.rs   # CditorV2View 主 struct
│   ├── state.rs            # CditorViewState (Ready/Loading/LoadFailed)
│   ├── lifecycle.rs        # from_runtime, apply_loaded_runtime
│   ├── render.rs           # Render trait 实现
│   ├── command_router.rs   # CditorCommand → DocumentRuntime 路由
│   ├── sdk.rs              # sdk_* 方法（通过 CditorHandle 暴露）
│   ├── persistence_bridge.rs # 自动保存 + 脏标记桥接
│   ├── payload_cache.rs, text_hit.rs, input_trace.rs
│   ├── cditor_v2_view/     # CditorV2View 子模块
│   │   ├── ai.rs, block_actions.rs, code_language.rs, code_theme.rs
│   │   ├── folding.rs, platform_input.rs, slash_menu.rs
│   │   ├── table_actions.rs, text_surface.rs, whiteboard.rs
│   │   └── formatting/
│   │       ├── mod.rs, actions.rs, color.rs, toolbar.rs
│   ├── input/              # 编辑器级输入
│   │   ├── mod.rs, actions.rs, keyboard.rs, mouse.rs
│   │   ├── ime.rs, ime_geometry.rs, ime_support.rs, text_drag.rs
│   └── interaction/        # 编辑器交互
│       ├── mod.rs, geometry.rs, scrollbar.rs, image_resize.rs
│       ├── gutter_drag.rs, gutter_drag_commit.rs, gutter_drag_metrics.rs
│       ├── table_mode.rs, table_reorder.rs, table_resize.rs, table_scroll.rs
│
├── persistence/            # 持久化桥接（4 文件）
│   ├── mod.rs
│   ├── close_guard.rs, payload_loader.rs
│   ├── save_indicator.rs   # EditorSaveStatus
│   └── storage_saver.rs    # StoragePersistenceState
│
├── scroll/                 # GPUI 滚动适配器
│   └── mod.rs
│
├── skeleton/               # 骨架屏
│   ├── mod.rs, primitives.rs
├── diagnostics/            # 编辑器诊断
│   ├── mod.rs, block_color.rs
├── platform.rs             # normalize_external_line_endings 等平台工具
├── clipboard_assets.rs
├── image_loader.rs, image_preview.rs
├── rich_text.rs, menu_metrics.rs
└── native_menu/            # (空目录，待实现)
```

**依赖：** `cditor-core + cditor-editor-core + cditor-runtime + cditor-storage + cditor-theme + cditor-ai + cditor-text + cditor-import-export + gpui + gpui_platform + swash + lumis + image + mermaid_render + reqwest`

---

### cditor-import-export（~20 个 .rs 文件）

```
crates/cditor-import-export/src/
├── lib.rs
├── error.rs
│
# ── 来自 core/src/rich_text/ ──
├── clipboard.rs            # CditorClipboardEnvelope, ClipboardBlock (原 core)
├── markdown/
│   ├── mod.rs              # MarkdownParser, parse_markdown_document
│   ├── block.rs            # block_kind_for_marker, parse_callout_marker
│   ├── export.rs           # block_to_plain_markdown, export_plain_markdown
│   ├── inline.rs           # parse_inline_markdown
│   ├── table.rs            # parse_table_region, table_to_plain_markdown
│   ├── tests.rs
│   └── stats.rs            # MarkdownParseStats
│
# ── 来自 core/src/rich_text/table/ ──
├── table_clipboard.rs      # 表格剪贴板格式
│
# ── 来自 runtime/src/content/ ──
├── paste_import.rs         # ClipboardInput, PasteImportPipeline, PasteImportConfig
├── security.rs             # ExternalContentPolicy, PrivacyMode, SvgPolicy
│
# ── 新增 ──
├── html/
│   ├── mod.rs
│   ├── sanitizer.rs
│   ├── importer.rs
│   ├── exporter.rs
│   └── paste.rs
├── csv/
│   ├── mod.rs
│   ├── database_import.rs
│   └── database_export.rs
└── native_archive/
    ├── mod.rs
    ├── manifest.rs
    ├── exporter.rs
    ├── importer.rs
    ├── id_mapping.rs
    └── asset_bundle.rs
```

### cditor-api（~14 个 .rs 文件）

```
crates/cditor-api/src/
├── lib.rs
├── error.rs                # CditorError
├── version.rs
│
├── config/
│   ├── mod.rs
│   ├── editor_config.rs
│   ├── storage_config.rs
│   ├── layout_config.rs
│   ├── editing_config.rs
│   ├── blocks_config.rs
│   ├── permissions_config.rs
│   ├── performance_config.rs
│   ├── telemetry_config.rs
│   └── collaboration_config.rs
│
├── commands/
│   ├── mod.rs
│   ├── editor_command_api.rs
│   ├── command.rs          # CditorCommand, CommandState, CommandOutcome
│   ├── block_command.rs
│   ├── page_command.rs
│   └── collection_command.rs
│
├── queries/
│   ├── mod.rs
│   ├── editor_query_api.rs
│   ├── search_request.rs
│   └── export_request.rs
│
├── events/
│   ├── mod.rs
│   ├── editor_event.rs     # CditorEvent
│   ├── event_filter.rs
│   └── event_stream.rs
│
├── services/
│   ├── mod.rs
│   ├── document_service.rs
│   ├── block_service.rs
│   ├── collection_service.rs
│   ├── asset_service.rs
│   ├── search_service.rs
│   └── telemetry_service.rs
│
├── extensions/
│   ├── mod.rs
│   ├── editor_extension.rs
│   ├── command_registry.rs
│   ├── keymap_registry.rs
│   ├── decoration_registry.rs
│   └── import_export_registry.rs
│
└── integration/
    ├── mod.rs
    ├── token.rs
    ├── scopes.rs
    ├── webhook.rs
    └── rate_limit.rs
```

**关键设计：** `cditor-api` 不依赖 `cditor-editor`。通过 trait 抽象打破耦合（见第 6 节）。

### cditor-app（~15 个 .rs 文件）

```
crates/cditor-app/src/
├── main.rs                 # GPUI 桌面应用入口
├── lib.rs
├── bootstrap.rs
├── settings.rs
├── app_state.rs
├── window.rs
├── logging.rs
├── wiring/
│   ├── mod.rs
│   ├── storage_wiring.rs
│   ├── runtime_wiring.rs
│   ├── editor_wiring.rs
│   ├── theme_wiring.rs
│   └── api_wiring.rs
└── config/
    ├── mod.rs
    ├── load_config.rs
    ├── default_config.rs
    └── env.rs
```

### cditor-test-support（~15 个 .rs 文件）

```
crates/cditor-test-support/src/
├── lib.rs
├── fixtures/
│   ├── mod.rs
│   ├── documents.rs
│   ├── blocks.rs
│   ├── collections.rs
│   └── traces.rs
├── builders/
│   ├── mod.rs
│   ├── block_builder.rs
│   ├── document_builder.rs
│   ├── transaction_builder.rs
│   └── collection_builder.rs
├── fake_store.rs
├── fake_asset_store.rs
├── fake_permission.rs
├── fake_task_spawner.rs
├── fake_runtime.rs
│
# ── 来自 runtime/src/acceptance/ ──
├── acceptance/
│   ├── mod.rs
│   ├── editing.rs
│   ├── open.rs
│   ├── scroll.rs
│   ├── structure_edit.rs
│   └── table.rs
│
└── trace_replay/
    ├── mod.rs
    ├── scroll_trace.rs
    ├── edit_trace.rs
    └── replay_runner.rs
```

### cditor-ai / cditor-text / ding-board（不变）

```
crates/cditor-ai/src/
├── lib.rs
├── provider.rs             # AiProvider trait + AiStreamEvent
├── openai.rs               # OpenAiCompatibleProvider
└── mock.rs                 # MockAiProvider

crates/cditor-text/src/
├── lib.rs
├── (Parley 文本布局适配器)

crates/ding-board/src/
├── lib.rs
├── font.rs
└── render_perf.rs
```

---

## 5. 文件到 crate 迁移对照表

### 5.1 旧 core/ → 新 crate

| 旧文件 | 新文件 | 备注 |
|---|---|---|
| `core/src/lib.rs` | `cditor-core/src/lib.rs` | 去掉 markdown re-export |
| `core/src/ids.rs` | `cditor-core/src/ids.rs` | |
| `core/src/version.rs` | `cditor-core/src/version.rs` | |
| `core/src/demo_fixtures.rs` | `cditor-core/src/demo_fixtures.rs` | #[cfg(feature="fixtures")] |
| `core/src/block/*` (5 文件) | `cditor-core/src/block/*` | 全部 |
| `core/src/document/*` (3 文件) | `cditor-core/src/document/*` | 全部 |
| `core/src/edit/*` (10 文件) | `cditor-core/src/edit/*` | 全部 |
| `core/src/layout/*` (8 文件) | `cditor-core/src/layout/*` | 全部 |
| `core/src/schema/*` (3 文件) | `cditor-core/src/schema/*` | 全部 |
| `core/src/telemetry/*` (5 文件) | `cditor-core/src/telemetry/*` | 全部 |
| `core/src/identity/*` (6 文件) | `cditor-core/src/identity/*` | 全部 |
| `core/src/fixtures/*` (4 文件) | `cditor-core/src/fixtures/*` | #[cfg(feature="fixtures")] |
| `core/src/rich_text/mod.rs` | `cditor-core/src/rich_text/mod.rs` | 去掉 markdown/clipboard re-export |
| `core/src/rich_text/block_kind.rs` | `cditor-core/src/rich_text/block_kind.rs` | |
| `core/src/rich_text/payload.rs` | `cditor-core/src/rich_text/payload.rs` | |
| `core/src/rich_text/document.rs` | `cditor-core/src/rich_text/document.rs` | |
| `core/src/rich_text/attrs.rs` | `cditor-core/src/rich_text/attrs.rs` | |
| `core/src/rich_text/inline.rs` | `cditor-core/src/rich_text/inline.rs` | |
| `core/src/rich_text/table.rs` | `cditor-core/src/rich_text/table.rs` | |
| `core/src/rich_text/span_splice.rs` | `cditor-core/src/rich_text/span_splice.rs` | |
| `core/src/rich_text/table/structure.rs` | `cditor-core/src/rich_text/table/structure.rs` | |
| `core/src/rich_text/table/style.rs` | `cditor-core/src/rich_text/table/style.rs` | |
| `core/src/rich_text/table/tests.rs` | `cditor-core/src/rich_text/table/tests.rs` | |
| **`core/src/rich_text/clipboard.rs`** | **`cditor-import-export/src/clipboard.rs`** | ★ 迁出 |
| **`core/src/rich_text/markdown/*` (7 文件)** | **`cditor-import-export/src/markdown/*`** | ★ 迁出 |
| **`core/src/rich_text/table/clipboard.rs`** | **`cditor-import-export/src/table_clipboard.rs`** | ★ 迁出 |

### 5.2 旧 editor/ → 新 crate

| 旧文件 | 新文件 | 备注 |
|---|---|---|
| `editor/src/lib.rs` | `cditor-editor/src/lib.rs` | |
| `editor/src/command.rs` | `cditor-editor/src/command.rs` | |
| `editor/src/command/catalog.rs` | `cditor-editor/src/command/catalog.rs` | |
| `editor/src/scroll/*` (7 文件) | `cditor-editor/src/scroll/*` | |
| `editor/src/window/*` (4 文件) | `cditor-editor/src/window/*` | |
| `editor/src/hit_test.rs` | `cditor-editor/src/hit_test.rs` | |
| `editor/src/debug_overlay.rs` | `cditor-editor/src/debug_overlay.rs` | |
| `editor/src/scroll_trace_replay.rs` | `cditor-editor/src/scroll_trace_replay.rs` | |
| `editor/src/trace_event_log.rs` | `cditor-editor/src/trace_event_log.rs` | |

### 5.3 旧 runtime/ → 新 crate

| 旧文件 | 新文件 | 备注 |
|---|---|---|
| `runtime/src/lib.rs` | `cditor-runtime/src/lib.rs` | 去掉迁出模块 |
| `runtime/src/document_runtime/*` (50+ 文件) | `cditor-runtime/src/document_runtime/*` | 全部留 |
| `runtime/src/editing/*` (3 文件) | `cditor-runtime/src/editing/*` | 全部留 |
| `runtime/src/scheduling/*` (4 文件) | `cditor-runtime/src/scheduling/*` | 全部留 |
| `runtime/src/projection/*` (2 文件) | `cditor-runtime/src/projection/*` | 全部留 |
| `runtime/src/content/media_cache.rs` | `cditor-runtime/src/content/media_cache.rs` | |
| `runtime/src/content/payload_cache.rs` | `cditor-runtime/src/content/payload_cache.rs` | |
| `runtime/src/content/payload_window.rs` | `cditor-runtime/src/content/payload_window.rs` | |
| **`runtime/src/content/paste_import.rs`** | **`cditor-import-export/src/paste_import.rs`** | ★ 迁出 |
| **`runtime/src/content/security.rs`** | **`cditor-import-export/src/security.rs`** | ★ 迁出 |
| **`runtime/src/content/query_index.rs`** | **`cditor-storage/src/query_index.rs`** | ★ 迁出 |
| **`runtime/src/acceptance/*` (6 文件)** | **`cditor-test-support/src/acceptance/*`** | ★ 迁出 |

### 5.4 旧 store/ → 新 crate

| 旧文件 | 新文件 | 备注 |
|---|---|---|
| `store/src/*` (11 文件) | `cditor-storage/src/*` | 全部迁，无拆分 |
| `store-postgres/src/*` | `cditor-storage-postgres/src/*` | 全部迁 |
| `store-sqlite/src/*` | `cditor-storage-sqlite/src/*` | 全部迁 |

### 5.5 旧 app/ → 新 crate

| 旧文件 | 新文件 | 备注 |
|---|---|---|
| `app/src/api/*` (14 文件) | `cditor-api/src/*` | ★ 迁出，独立 crate |
| `app/src/gui/block/*` (约50文件) | `cditor-editor/src/block/*` | ★ 迁出 |
| `app/src/gui/document/*` (5 文件) | `cditor-editor/src/document/*` | ★ 迁出 |
| `app/src/gui/text/*` (约12文件) | `cditor-editor/src/text/*` | ★ 迁出 |
| `app/src/gui/input/*` (9 文件) | `cditor-editor/src/input/*` | ★ 迁出 |
| `app/src/gui/overlay/*` (约14文件) | `cditor-editor/src/overlay/*` | ★ 迁出 |
| `app/src/gui/app/*` (约30文件) | `cditor-editor/src/app/*` | ★ 迁出 |
| `app/src/gui/persistence/*` (4 文件) | `cditor-editor/src/persistence/*` | ★ 迁出 |
| `app/src/gui/scroll/mod.rs` | `cditor-editor/src/scroll/mod.rs` | 与算法层 scroll/ 合并 |
| `app/src/gui/skeleton/*` (2 文件) | `cditor-editor/src/skeleton/*` | ★ 迁出 |
| `app/src/gui/diagnostics/*` (2 文件) | `cditor-editor/src/diagnostics/*` | ★ 迁出 |
| `app/src/gui/theme.rs` | `cditor-theme/src/theme.rs` | ★ 提升到 theme crate |
| `app/src/gui/platform.rs` | `cditor-editor/src/platform.rs` | ★ 迁出 |
| `app/src/gui/clipboard_assets.rs` | `cditor-editor/src/clipboard_assets.rs` | ★ 迁出 |
| `app/src/gui/image_loader.rs` | `cditor-editor/src/image_loader.rs` | ★ 迁出 |
| `app/src/gui/image_preview.rs` | `cditor-editor/src/image_preview.rs` | ★ 迁出 |
| `app/src/gui/rich_text.rs` | `cditor-editor/src/rich_text.rs` | ★ 迁出 |
| `app/src/gui/menu_metrics.rs` | `cditor-editor/src/menu_metrics.rs` | ★ 迁出 |
| `app/src/gui/mod.rs` | `cditor-editor/src/mod.rs`（合并） | ★ 迁出 |
| `app/src/main.rs` | `cditor-app/src/main.rs` | ★ 迁出 |
| `app/src/lib.rs` | **删除** | 不再需要聚合 |
| `app/tests/component_sdk.rs` | `cditor-app/tests/component_sdk.rs` | ★ 端到端测试 |

### 5.6 不变

| 旧 crate | 新 crate | 备注 |
|---|---|---|
| `crates/ai/` | `cditor-ai/` | 零依赖，完全不变 |
| `crates/text/` | `cditor-text/` | 独立布局层，不变 |
| `crates/ding-board/` | `ding-board/` | 独立白板组件，不变 |

---

## 6. 跨层依赖打破方案

新架构要求 `cditor-api` 不依赖 `cditor-editor`。当前有 2 处违反：

### 6.1 Cditor::build_view() → CditorV2View

**当前代码（app/src/api/cditor.rs:174）：**
```rust
pub fn build_view(self, cx: &mut Context<CditorV2View>) -> CditorV2View { ... }
```

`CditorV2View` 是 GPUI Entity，定义在 `cditor-editor/src/app/cditor_v2_view.rs`。

**修复方案：ViewFactory trait**

```rust
// cditor-api/src/lib.rs
use gpui::{AppContext, Entity};

pub trait CditorViewFactory: Send + Sync {
    fn build_view(
        &self,
        options: CditorOptions,
        cx: &mut AppContext,
    ) -> CditorComponent;
}

// cditor-app/src/main.rs
struct CditorAppViewFactory;
impl CditorViewFactory for CditorAppViewFactory {
    fn build_view(&self, options: CditorOptions, cx: &mut AppContext) -> CditorComponent {
        // 组装 CditorV2View 作为 CditorComponent 返回
    }
}
```

### 6.2 ThemeProvider::theme() → GuiTheme

**当前代码（app/src/api/providers.rs）：**
```rust
fn theme(&self) -> crate::gui::GuiTheme;
```

`GuiTheme` 定义在 `app/src/gui/theme.rs`（30+ 颜色字段）。

**修复方案：把 GuiTheme 提升到 cditor-theme**

```rust
// cditor-theme/src/theme.rs
#[derive(Debug, Clone, Copy)]
pub struct GuiTheme {
    pub background: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    // ... 30+ fields
}
```

`cditor-api` 和 `cditor-editor` 都依赖 `cditor-theme`，不再互相耦合。

---

## 7. crate 关键类型定义

### cditor-core（公共 API 面）

```rust
// ids
pub type DocumentId = u64;
pub type BlockId = u64;
pub type SurfaceId = u64;
pub type WorkspaceId = u64;

// block
pub enum RichBlockKind { Paragraph, Heading(u8), Quote, Callout(CalloutVariant), ... } // 30 variants
pub enum BlockPayload { RichText { spans }, Code { text, language }, Table(TablePayload), Image(ImagePayload), Collection(CollectionPayload), ... }
pub struct BlockPayloadRecord { pub block_id, pub kind, pub payload }
pub struct BlockAttrs { pub color, pub background_color, pub text_align, pub indent, pub folded, pub locked }

// document
pub struct DocumentIndex { /* SoA 平行数组 */ }
pub struct VisibleDocumentIndex { /* 折叠后的可见投影 */ }

// edit
pub struct EditTransaction { pub operations: Vec<EditOperation>, pub kind, pub origin, pub preconditions }
pub enum EditOperation { InsertText, DeleteText, SplitBlock, MergeBlocks, InsertBlock, DeleteBlock, MoveBlock, ... }
pub struct DocumentSelection { pub anchor: SelectionEndpoint, pub focus: SelectionEndpoint }
pub struct UndoStack { /* undo/redo 引擎 */ }

// layout
pub struct BlockHeightIndex { /* Fenwick Tree */ }
pub struct PageLayoutIndex { /* 分页布局 */ }
```

### cditor-runtime（公共 API 面）

```rust
pub struct DocumentRuntime { /* 56 字段的活文档状态聚合根 */ }
pub struct EditingSession { /* IME 组合 + 编辑状态 */ }
pub struct SingleCharInputHotPath { /* 单字符热路径 */ }
pub struct PayloadWindow { /* 视口 payload 窗口管理 */ }
pub struct VirtualScrollState { /* 虚拟滚动状态 */ }
pub struct EditorViewProjection { /* 视口投影 */ }
```

### cditor-api（公共 API 面）

```rust
pub struct Cditor { /* builder */ }
pub struct CditorOptions { /* backend, readonly, debug_overlay, autosave */ }
pub enum CditorBackend { Demo, LargeDemo, Memory, PostgresUrl, PostgresPool, Sqlite, Cloud }
pub enum CditorCommand { /* 所有 SDK 命令 */ }
pub enum CditorEvent { Ready, LoadFailed, Change, ... }
pub struct CditorHandle { /* WeakEntity 控制句柄 */ }
```

### cditor-storage（公共 API 面）

```rust
pub trait DocumentStorage { /* load, save, ... */ }
pub struct StorageSession { /* 存储会话 */ }
pub struct LoadedDocument { pub runtime: DocumentRuntime, pub storage_session: StorageSession }
```

---

## 8. crate 配置 schema

### CditorOptions（cditor-api）

```rust
pub struct CditorOptions {
    pub backend: CditorBackend,
    pub workspace_id: Option<WorkspaceId>,
    pub document_id: Option<DocumentId>,
    pub debug_overlay: bool,
    pub readonly: bool,
    pub payload_window_size: usize,
    pub autosave_interval: Option<Duration>,
    pub seed_large_demo_to_postgres: bool,
    pub seed_large_demo_block_count: usize,
    pub force_reseed_large_demo: bool,
}
```

### LayoutSchedulerConfig（cditor-runtime）

```rust
pub struct LayoutSchedulerConfig {
    pub max_tasks_per_frame: usize,
    pub max_composition_tasks_per_frame: usize,
    pub overscan_pages: usize,
    pub prefetch_pages: usize,
}
```

### MainThreadBudget（cditor-runtime）

```rust
pub struct MainThreadBudget {
    pub target_frame_ms: f64,
    pub max_sync_work_ms: f64,
    pub interaction_mode: InteractionMode,  // Typing / Scrolling / Idle
}
```

---

## 9. 事件目录

| 事件 | 定义位置 | 触发源 |
|---|---|---|
| `CditorEvent::Ready` | cditor-api | 文档加载完成 |
| `CditorEvent::LoadFailed` | cditor-api | 加载失败 |
| `CditorEvent::Change` | cditor-api | 文档变更 |
| `CditorEvent::SelectionChanged` | cditor-api | 选区变更 |
| `CditorEvent::SaveStatusChanged` | cditor-api | 保存状态变更 |
| `RuntimeEvent::ContentChanged(BlockId)` | cditor-runtime | 块内容变更 |
| `RuntimeEvent::StructureChanged` | cditor-runtime | 结构变更 |
| `RuntimeEvent::ScrollChanged(VirtualScrollState)` | cditor-runtime | 滚动变更 |
| `RuntimeEvent::LayoutDirty(Range<BlockId>)` | cditor-runtime | 布局脏 |
| `RuntimeEvent::WindowCommit(WindowPlanDecision)` | cditor-runtime | 窗口提交 |
| `TraceEvent::LayoutTaskDeferred` | cditor-editor | 布局任务推迟 |
| `TraceEvent::PageHeightCorrected` | cditor-editor | 页面高度修正 |

---

## 10. 测试分布矩阵

| 测试类型 | 位置 | 覆盖 |
|---|---|---|
| **单元测试** | `cditor-core/tests/` | block/document/edit/layout/selection 纯逻辑 |
| **单元测试** | `cditor-editor/tests/` | scroll/window/hit_test 算法 |
| **单元测试** | `cditor-runtime/src/document_runtime/tests/` | 编辑/事务/结构操作（30+ 文件） |
| **单元测试** | `cditor-import-export/src/markdown/tests.rs` | markdown parse/export |
| **集成测试** | `cditor-storage-postgres/tests/` | 数据库查询 |
| **端到端测试** | `cditor-app/tests/` | 打开→编辑→保存→重开 全流程 |
| **性能验收** | `cditor-test-support/src/acceptance/` | 10MB/50k rows/100k blocks 压力 |
| **回归门** | `cditor-test-support/src/trace_replay/` | 滚动 trace replay |
| **基准测试** | `cditor-text/benches/` | 文本布局性能 |

---

## 11. 线程模型与异步边界

```
┌─ Main Thread ──────────────────────────────────────────┐
│  GPUI App loop                                          │
│  ├─ Input handling (keyboard/mouse/IME)                 │
│  ├─ CditorV2View::render()                              │
│  ├─ Command router (同步命令路由)                        │
│  ├─ LayoutScheduler (帧级调度)                           │
│  └─ MainThreadBudget (帧预算仲裁)                        │
├─────────────────────────────────────────────────────────┤
│  Worker Pool (tokio)                                    │
│  ├─ Layout measurement tasks (WorkerLane::Interactive)  │
│  ├─ Background tasks (WorkerLane::Background)           │
│  └─ AsyncVersionController (结果校验)                    │
├─────────────────────────────────────────────────────────┤
│  Storage Runtime (tokio, "cditor-storage" thread)        │
│  ├─ block_on_storage()                                  │
│  ├─ SQL queries (PG/SQLite)                             │
│  └─ OptimisticPersistenceManager                        │
├─────────────────────────────────────────────────────────┤
│  Background Tasks                                       │
│  ├─ AI inference streaming                              │
│  └─ Auto-save timer                                     │
└─────────────────────────────────────────────────────────┘
```

**跨线程通信规则：**
- Main → Worker：通过 `AsyncTaskQueue` 发送 `LayoutTaskRequest`
- Worker → Main：通过 `AsyncResultQueue` 返回 `LayoutTaskResult`，经 `AsyncVersionController` 校验
- Main → Storage：通过 `block_on_storage()` 同步调用（阻塞主线程，仅用于 cold start），后续通过 `OptimisticPersistenceManager` 异步写
- 编辑热路径：禁止任何同步 I/O（`ForbiddenSyncWorkGuard` 强制执行）

---

## 12. 执行任务清单

> 每个阶段完成后执行 `cargo check --workspace`。
> 按依赖拓扑推进，不可跳过阶段。
> 完成一项在 `[ ]` 中填入 `x`。

---

### 阶段 0：环境准备

- [x] **0.1** 确认 workspace 已包含本次重构的 16 个 crate，并保留独立的 `cditor-collaboration`（合计 17 个）
- [x] **0.2** 创建 `cditor-theme-types/` 的 `Cargo.toml` + `src/lib.rs`（6 个 token 模块声明）
- [x] **0.3** 创建 `cditor-theme/` 的 `Cargo.toml` + `src/lib.rs`（7 个模块声明）
- [x] **0.4** 创建 `cditor-editor-core/` 的 `Cargo.toml` + `src/lib.rs`（模块声明）
- [x] **0.5** 创建 `cditor-import-export/` 的 `Cargo.toml` + `src/lib.rs`（模块声明）
- [x] **0.6** 创建 `cditor-api/` 的 `Cargo.toml` + `src/lib.rs`（模块声明）
- [x] **0.7** 创建 `cditor-app/` 的 `Cargo.toml` + `src/lib.rs`（模块声明）
- [x] **0.8** 创建 `cditor-test-support/` 的 `Cargo.toml` + `src/lib.rs`（模块声明）
- [x] **0.9** 确认所有空 crate 的 `cargo check` 通过（至少 `lib.rs` 不能有语法错误）

---

### 阶段 1：cditor-core — 纯领域模型

> 目标：旧 `crates/core/` → 新 `crates/cditor-core/`
> 同步迁出 markdown 到 ceditor-import-export

- [x] **1.1** 从旧 `core/src/` 搬入全部保留文件到 `cditor-core/src/`（56 个文件，不改代码）
  - block/, document/, edit/, layout/, schema/, telemetry/, identity/, fixtures/
  - rich_text/（block_kind, payload, document, attrs, inline, table, span_splice, table/）
  - ids.rs, version.rs, demo_fixtures.rs, lib.rs
- [x] **1.2** 配置 `cditor-core/Cargo.toml` 依赖：serde, serde_json, thiserror, uuid, indexmap, smallvec
- [x] **1.3** 在 `cditor-core/src/lib.rs` 去掉 markdown 和 clipboard 的 pub mod 声明
- [x] **1.4** 迁出 `rich_text/markdown/` 全部 7 个文件 → `cditor-import-export/src/markdown/`
- [x] **1.5** 迁出 `rich_text/clipboard.rs` → `cditor-import-export/src/clipboard.rs`
- [x] **1.6** 迁出 `rich_text/table/clipboard.rs` → `cditor-import-export/src/table_clipboard.rs`
- [x] **1.7** 配置 `cditor-import-export/Cargo.toml` 依赖：cditor-core + serde + serde_json
- [x] **1.8** `cargo check -p cditor-core` 通过
- [x] **1.9** `cargo check -p cditor-import-export` 通过

---

### 阶段 2：cditor-storage — 存储接口

> 目标：旧 `crates/store/` → 新 `crates/cditor-storage/`
> 旧 `crates/store-postgres/` → `crates/cditor-storage-postgres/`
> 旧 `crates/store-sqlite/` → `crates/cditor-storage-sqlite/`

- [x] **2.1** 从旧 `store/src/` 搬入全部 11 个文件到 `cditor-storage/src/`
- [x] **2.2** 配置 `cditor-storage/Cargo.toml` 依赖：cditor-core + serde + serde_json + thiserror + tokio
- [x] **2.3** `cargo check -p cditor-storage` 通过
- [x] **2.4** 从旧 `store-postgres/src/` 搬入全部文件到 `cditor-storage-postgres/src/`
- [x] **2.5** 更新 `cditor-storage-postgres/Cargo.toml` 依赖 path 指向新 crate
- [x] **2.6** `cargo check -p cditor-storage-postgres` 通过
- [x] **2.7** 从旧 `store-sqlite/src/` 搬入全部文件到 `cditor-storage-sqlite/src/`
- [x] **2.8** 更新 `cditor-storage-sqlite/Cargo.toml` 依赖 path 指向新 crate
- [x] **2.9** `cargo check -p cditor-storage-sqlite` 通过

---

### 阶段 3：cditor-editor-core — 纯算法层

> 目标：旧 `crates/editor/` → 新 `crates/cditor-editor-core/`

- [x] **3.1** 从旧 `editor/src/` 搬入全部 10 个文件到 `cditor-editor-core/src/`
  - command.rs, hit_test.rs, debug_overlay.rs, scroll_trace_replay.rs, trace_event_log.rs
  - scroll/ (7 文件), window/ (4 文件)
- [x] **3.2** 配置 `cditor-editor-core/Cargo.toml` 依赖：cditor-core + serde + serde_json + unicode-segmentation
- [x] **3.3** `cargo check -p cditor-editor-core` 通过

---

### 阶段 4：cditor-runtime — 活文档状态

> 目标：旧 `crates/runtime/` → 新 `crates/cditor-runtime/`
> 同步迁出 paste_import/security → import-export，query_index → storage，acceptance → test-support

- [x] **4.1** 从旧 `runtime/src/` 搬入保留文件到 `cditor-runtime/src/`
  - document_runtime/（50+ 文件，全部保留）
  - editing/（3 文件）
  - scheduling/（4 文件）
  - projection/（2 文件）
  - content/（media_cache, payload_cache, payload_window 三个文件保留）
  - lib.rs（去掉迁出模块的声明）
- [x] **4.2** 迁出 `content/paste_import.rs` → `cditor-import-export/src/paste_import.rs`
- [x] **4.3** 迁出 `content/security.rs` → `cditor-import-export/src/security.rs`
- [x] **4.4** 更新 `cditor-import-export/src/lib.rs` 增加 paste_import 和 security 的模块声明
- [x] **4.5** 迁出 `content/query_index.rs` → `cditor-storage/src/query_index.rs`
- [x] **4.6** 更新 `cditor-storage/src/lib.rs` 增加 query_index 的模块声明
- [x] **4.7** 迁出 `acceptance/` 全部 6 个文件 → `cditor-test-support/src/acceptance/`
- [x] **4.8** 配置 `cditor-runtime/Cargo.toml` 依赖指向新 crate path
- [x] **4.9** 配置 `cditor-test-support/Cargo.toml`：依赖 cditor-runtime + cditor-core + cditor-editor-core
- [x] **4.10** `cargo check -p cditor-runtime` 通过
- [x] **4.11** `cargo check -p cditor-import-export` 通过
- [x] **4.12** `cargo check -p cditor-test-support` 通过

---

### 阶段 5：打破跨层依赖（trait 抽象）

> 目标：cditor-api 不能依赖 cditor-editor，cditor-api 不能引用 GuiTheme

- [x] **5.1** 把 `GuiTheme` 从旧 `app/src/gui/theme.rs` 迁到 `cditor-theme/src/theme.rs`
  - 保持 30+ 颜色字段不变
  - 去掉 `app/src/gui/theme.rs` 中的原始 GuiTheme 定义
- [x] **5.2** 更新 `cditor-theme/src/lib.rs` 增加 theme 模块声明
- [x] **5.3** 配置 `cditor-theme/Cargo.toml` 依赖：cditor-theme-types + serde + serde_json
- [x] **5.4** `cargo check -p cditor-theme` 通过
- [x] **5.5** 更新旧代码中所有 `crate::gui::GuiTheme` 引用 → `cditor_theme::GuiTheme`
- [ ] **5.6** 裁决并实现跨 crate 的 typed GPUI component/handle 构造协议（不采用无法保持 `Entity<CditorV2View>` 类型的伪擦除）
  ```rust
  pub trait CditorViewFactory: Send + Sync {
      fn build_component(&self, options: CditorOptions, cx: &mut AppContext) -> CditorComponent;
  }
  ```
- [ ] **5.7** 让 `Cditor::build()` 调用 app composition API，返回可用的 typed component/handle，禁止 panic 或永久 `Unsupported`

---

### 阶段 6：cditor-api — SDK API

> 目标：旧 `app/src/api/` → 新 `crates/cditor-api/`

- [x] **6.1** 从旧 `app/src/api/` 搬入全部 14 个文件到 `cditor-api/src/`
  - builder.rs, cditor.rs, cold_start.rs, command.rs, component.rs
  - diagnostics.rs, document.rs, error.rs, event.rs, handle.rs
  - import_export.rs, mod.rs, options.rs, providers.rs
- [x] **6.2** 配置 `cditor-api/Cargo.toml` 依赖：cditor-core + cditor-runtime + cditor-storage + cditor-theme + gpui
- [x] **6.3** 更新所有 `crate::gui::*` 引用 → 对应新 crate 的路径
- [x] **6.4** `cargo check -p cditor-api` 通过

---

### 阶段 7：cditor-editor — GPUI UI 层（最重）

> 目标：旧 `app/src/gui/` → 新 `crates/cditor-editor/`
> 这是最大的改动：约 350 个 .rs 文件

- [x] **7.1** 从旧 `app/src/gui/` 搬入全部子目录和文件到 `cditor-editor/src/`
  - block/（约 50 文件，含 code/, table/, mermaid/, whiteboard/ 子目录）
  - document/（5 文件）
  - text/（约 12 文件，含 parley_adapter/ 子目录）
  - input/（9 文件）
  - overlay/（约 14 文件，含 table/ 子目录）
  - app/（约 30 文件，含 cditor_v2_view/, input/, interaction/ 子目录）
  - persistence/（4 文件）
  - scroll/（mod.rs）
  - skeleton/（2 文件）
  - diagnostics/（2 文件）
  - platform.rs, clipboard_assets.rs, image_loader.rs, image_preview.rs
  - rich_text.rs, menu_metrics.rs, mod.rs
- [x] **7.2** 配置 `cditor-editor/Cargo.toml` 依赖所有相关 crate + gpui + swash + lumis + image + mermaid_render
  ```toml
  cditor-core, cditor-editor-core, cditor-runtime, cditor-storage,
  cditor-theme, cditor-ai, cditor-text, cditor-import-export,
  gpui, gpui_platform, mermaid_render, swash, lumis, image, reqwest, ...
  ```
- [x] **7.3** 批量更新 cargo 依赖 path，把旧的 `crates/editor` → `../cditor-editor-core`
- [x] **7.4** 批量更新所有 `crate::` 内部引用 → 新 crate 名
- [x] **7.5** `cargo check -p cditor-editor` 通过（这步可能要修很多 import path）

---

### 阶段 8：cditor-app — 应用组装

> 目标：旧 `app/src/main.rs` → 新 `crates/cditor-app/`

- [x] **8.1** 从旧 `app/src/main.rs` 搬入到 `cditor-app/src/main.rs`
- [ ] **8.2** 创建 `cditor-app/src/wiring.rs`——组装 storage_wiring, runtime_wiring, editor_wiring, theme_wiring, api_wiring
- [x] **8.3** `cditor-app/Cargo.toml` 只声明 executable 实际使用的直接依赖，不强制依赖全部 crate
- [ ] **8.4** 实现 app composition API，注入 `cditor-editor::CditorV2View` 的构建逻辑
- [ ] **8.5** 恢复 `component_sdk.rs` 跨 crate typed component/handle 端到端测试
- [x] **8.6** `cargo check -p cditor-app` 通过

---

### 阶段 9：清理旧代码

- [x] **9.1** 从 workspace Cargo.toml members 移除旧的 `crates/app`, `crates/core`, `crates/editor`, `crates/runtime`, `crates/store`, `crates/store-postgres`, `crates/store-sqlite`
- [x] **9.2** 删除旧 `crates/app/` 目录
- [x] **9.3** 删除旧 `crates/core/` 目录
- [x] **9.4** 删除旧 `crates/editor/` 目录
- [x] **9.5** 删除旧 `crates/runtime/` 目录
- [x] **9.6** 删除旧 `crates/store/` 目录
- [x] **9.7** 删除旧 `crates/store-postgres/` 目录
- [x] **9.8** 删除旧 `crates/store-sqlite/` 目录
- [x] **9.9** `cargo check --workspace` 通过（全部 16 个 crate 可编译）

---

### 阶段 10：测试验证

- [x] **10.1** `cargo test -p cditor-core` 通过
- [x] **10.2** `cargo test -p cditor-editor-core` 通过
- [x] **10.3** `cargo test -p cditor-storage` 通过
- [ ] **10.4** `cargo test -p cditor-storage-postgres -- --ignored` 通过（需 PG 环境；本次未运行）
- [x] **10.5** `cargo test -p cditor-storage-sqlite` 通过
- [x] **10.6** `cargo test -p cditor-runtime` 通过
- [x] **10.7** `cargo test -p cditor-import-export` 通过
- [x] **10.8** `cargo test -p cditor-api` 通过
- [x] **10.9** `cargo test -p cditor-editor` 通过
- [x] **10.10** `cargo test -p cditor-app` 通过（当前 crate 编译与单测；component SDK 端到端覆盖见未完成的 8.5）
- [x] **10.11** `cargo test -p cditor-test-support` 通过（验收测试）
- [x] **10.12** `cargo test --workspace` 全部通过

---

### 任务统计

| 阶段 | 任务数 | 说明 |
|---|---|---|
| 0. 环境准备 | 9 | 预创建 7 个空 crate |
| 1. cditor-core | 9 | 领域模型 + markdown 迁出 |
| 2. cditor-storage | 9 | 存储三 crate |
| 3. cditor-editor-core | 3 | 纯算法层 |
| 4. cditor-runtime | 12 | 活文档状态 + 迁出到 3 个新 crate |
| 5. 打破跨层依赖 | 7 | trait + GuiTheme 提升 |
| 6. cditor-api | 4 | SDK 面 |
| 7. cditor-editor | 5 | GPUI UI 层（~350 文件，最重） |
| 8. cditor-app | 6 | 组装层 |
| 9. 清理旧代码 | 9 | 删除旧目录 |
| 10. 测试验证 | 12 | 全量测试 |
| **合计** | **85** | |

当前状态：**79/85 完成**。剩余 6 项是 typed component/handle composition（5.6、5.7、8.2、8.4、8.5）和需要外部 PostgreSQL 的 ignored 集成测试（10.4）；它们不影响 crate 目录迁移和普通 workspace 构建，但不能被目录移动本身冒充完成。


## 附录：关键决策汇总

| 决策 | 结论 |
|---|---|
| cditor-editor 身份 | **cditor-editor-core**（纯算法层，零 GPUI） + **cditor-editor**（GPUI UI 层），两个独立 crate |
| rich_text/markdown/ 归属 | **cditor-import-export**（parser+serializer 是独立的格式转换层） |
| block/chrome, drag, input_capability | **cditor-core**（纯数据结构/纯函数，不依赖 UI 框架） |
| content/paste_import + security | **cditor-import-export**（导入管道 + 安全过滤） |
| content/query_index | **cditor-storage**（FTS 查询属存储层） |
| content/payload_cache + payload_window | **cditor-runtime**（DocumentRuntime 核心字段，强制拆分会导致循环依赖） |
| acceptance/ | **cditor-test-support**（不污染生产 API） |
| store-sqlite | **保留**（与 store-postgres 对称，本地模式必需） |
| GuiTheme | **提升到 cditor-theme**（cditor-api 和 cditor-editor 都能引用） |
| Cditor::build_view() 跨层依赖 | **ViewFactory trait**（cditor-api 定义 trait，cditor-app 注入实现） |
| fixtures + demo_fixtures | **cditor-core**，通过 `#[cfg(feature = "fixtures")]` 控制编译 |
