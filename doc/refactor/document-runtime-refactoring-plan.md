# DocumentRuntime 重构方案

## 📋 文档元信息

- **创建时间**: 2026-07-22
- **状态**: 规划中
- **优先级**: P0 (紧急)
- **预计工期**: 4-6 周
- **风险等级**: 高（涉及核心运行时）
- **责任人**: 待定

## 🎯 重构目标

### 当前问题

`DocumentRuntime` 是一个典型的 **God Object**：
- **58 个字段**混杂了 8 个不同领域的状态
- **50 个子模块**平铺在 `document_runtime/` 目录
- **16,398 行代码**集中在单一结构体
- 认知负担巨大，测试困难，并发受限

### 目标状态

- 将 `DocumentRuntime` 拆分为 **7-8 个独立子系统**
- 每个子系统有清晰的边界和独立的测试
- 目录结构按领域组织，而非平铺文件
- 保持 **100% API 兼容**（对外接口不变）

## 📊 影响范围评估

### 直接影响的文件

```
crates/cditor-runtime/src/document_runtime/
├── state.rs                          [CORE - 修改]
├── 50 个子模块 .rs                    [ALL - 重构]
└── tests/                            [ALL - 调整]
```

### 间接影响的 crate

- `cditor-editor` (重度依赖 DocumentRuntime)
- `cditor-api` (对外接口层，需保持兼容)
- `cditor-app` (集成测试需要更新)
- `cditor-test-support` (测试辅助函数需要调整)

### 风险评估

| 风险项 | 概率 | 影响 | 缓解措施 |
|--------|------|------|----------|
| API 破坏性变更 | 中 | 高 | 先做内部重构，保持公开接口不变 |
| 性能回退 | 低 | 中 | 每阶段跑 benchmark，对比基线 |
| 引入新 bug | 中 | 高 | 每阶段必须通过全部现有测试 |
| 开发周期超期 | 高 | 中 | 采用渐进式重构，可随时停止 |

---

## 🗺️ 重构路线图

### Phase 0: 准备阶段（1 周）

**目标**: 建立安全网，为重构做准备

#### ✅ Checklist

- [ ] 0.1 冻结 `DocumentRuntime` 公开 API
  - [ ] 列出所有 `pub fn` 和 `pub struct`
  - [ ] 创建 API 兼容性测试套件
  - [ ] 文档化当前行为边界

- [ ] 0.2 建立性能基线
  - [ ] 运行全部 benchmark，记录结果
  - [ ] 识别性能关键路径（hot path）
  - [ ] 设定性能回退红线（如 ±5%）

- [ ] 0.3 完善测试覆盖
  - [ ] 确保 `document_runtime` 测试通过率 100%
  - [ ] 补充缺失的边界测试
  - [ ] 添加集成测试（端到端场景）

- [ ] 0.4 创建重构分支
  ```bash
  git checkout -b refactor/document-runtime-modularization
  ```

- [ ] 0.5 设置 CI 检查
  - [ ] 所有测试必须通过
  - [ ] Benchmark 不能超过基线 ±5%
  - [ ] 代码覆盖率不能下降

**交付物**:
- `doc/refactor/runtime-api-contract.md` (API 契约文档)
- `doc/refactor/runtime-performance-baseline.md` (性能基线)

---

### Phase 1: 目录重组（1 周）

**目标**: 将 50 个平铺文件按领域分组到子目录，不改变代码逻辑

#### 🎯 新目录结构

```
document_runtime/
├── mod.rs                    # 主入口
├── state.rs                  # DocumentRuntime 结构体定义
├── core/                     # 核心状态
│   ├── mod.rs
│   ├── cold_start.rs
│   ├── domain_state.rs
│   └── capabilities.rs
├── selection/                # 选区管理
│   ├── mod.rs
│   ├── selection.rs
│   ├── selection_materialization.rs
│   ├── selection_unified.rs
│   └── selection_transaction.rs
├── editing/                  # 编辑操作
│   ├── mod.rs
│   ├── text_edit.rs
│   ├── text_navigation.rs
│   ├── text_target.rs
│   ├── structure_edit.rs
│   ├── structure_insert.rs
│   ├── structure_delete.rs
│   ├── structure_move.rs
│   └── format_transaction.rs
├── undo/                     # 撤销/重做
│   ├── mod.rs
│   └── (未来独立模块)
├── transaction/              # 事务处理
│   ├── mod.rs
│   ├── transaction_apply.rs
│   ├── transaction_apply_domain.rs
│   ├── transaction_apply_domain_validation.rs
│   ├── transaction_apply_payload.rs
│   └── transaction_apply_structure.rs
├── layout/                   # 布局计算
│   ├── mod.rs
│   ├── layout_heights.rs
│   └── queries.rs
├── scroll/                   # 滚动状态
│   ├── mod.rs
│   └── scroll.rs
├── payload/                  # Payload 窗口
│   ├── mod.rs
│   ├── payload_window.rs
│   ├── payload_hydration.rs
│   ├── payload_cache.rs
│   ├── text_payload.rs
│   └── structure_payload.rs
├── projection/               # 视图投影
│   ├── mod.rs
│   └── projection.rs
├── ai/                       # AI 集成
│   ├── mod.rs
│   └── ai.rs
├── table/                    # 表格特性
│   ├── mod.rs
│   └── table.rs
├── clipboard/                # 剪贴板
│   ├── mod.rs
│   ├── clipboard.rs
│   ├── clipboard_blocks.rs
│   └── markdown_paste.rs
├── composition/              # IME 输入
│   ├── mod.rs
│   └── composition.rs
├── focus/                    # 焦点管理
│   ├── mod.rs
│   ├── focus.rs
│   └── focus_transition.rs
├── inline/                   # 内联格式
│   ├── mod.rs
│   ├── inline_format.rs
│   └── inline_color.rs
├── text_surface/             # 文本表面
│   ├── mod.rs
│   ├── text_surface.rs
│   ├── platform_text_edit.rs
│   └── typing_marks.rs
├── media/                    # 媒体资源
│   ├── mod.rs
│   └── media.rs
└── tests/                    # 测试
    ├── mod.rs
    ├── composition_input.rs
    ├── local_text_transactions.rs
    ├── rich_text_edit.rs
    ├── runtime_shortcuts.rs
    ├── selection_scroll.rs
    └── transaction_apply.rs
```

#### ✅ Checklist

- [ ] 1.1 创建子目录结构
  ```bash
  mkdir -p crates/cditor-runtime/src/document_runtime/{core,selection,editing,undo,transaction,layout,scroll,payload,projection,ai,table,clipboard,composition,focus,inline,text_surface,media}
  ```

- [ ] 1.2 移动文件（逐个领域）
  - [ ] core/ (3 个文件)
  - [ ] selection/ (4 个文件)
  - [ ] editing/ (9 个文件)
  - [ ] transaction/ (5 个文件)
  - [ ] layout/ (2 个文件)
  - [ ] scroll/ (1 个文件)
  - [ ] payload/ (5 个文件)
  - [ ] projection/ (1 个文件)
  - [ ] ai/ (1 个文件)
  - [ ] table/ (1 个文件)
  - [ ] clipboard/ (3 个文件)
  - [ ] composition/ (1 个文件)
  - [ ] focus/ (2 个文件)
  - [ ] inline/ (2 个文件)
  - [ ] text_surface/ (3 个文件)
  - [ ] media/ (1 个文件)

- [ ] 1.3 更新每个子目录的 `mod.rs`
  ```rust
  // 示例：selection/mod.rs
  mod selection;
  mod selection_materialization;
  mod selection_unified;
  mod selection_transaction;

  pub use selection::*;
  pub use selection_materialization::*;
  // ...
  ```

- [ ] 1.4 更新 `document_runtime/mod.rs` 导入
  ```rust
  pub mod core;
  pub mod selection;
  pub mod editing;
  // ...
  ```

- [ ] 1.5 修复所有编译错误（路径调整）

- [ ] 1.6 验证测试通过
  ```bash
  cargo test -p cditor-runtime
  ```

**交付物**:
- 重组后的目录结构
- 所有测试通过的 commit

---

### Phase 2: 选区系统独立（1 周）

**目标**: 将选区相关的 4 个字段提取为独立的 `SelectionState` 结构体

#### 🎯 目标结构

```rust
// 当前（在 DocumentRuntime 中）
pub struct DocumentRuntime {
    pub selected_block_ids: HashSet<BlockId>,
    pub document_selection: Option<DocumentSelection>,
    pub visual_caret_position: Option<VisualCaretPosition>,
    pub focused_text_selection: Option<FocusedTextSelection>,
    // ... 54 个其他字段
}

// 重构后
pub struct SelectionState {
    pub selected_block_ids: HashSet<BlockId>,
    pub document_selection: Option<DocumentSelection>,
    pub visual_caret_position: Option<VisualCaretPosition>,
    pub focused_text_selection: Option<FocusedTextSelection>,
}

pub struct DocumentRuntime {
    pub selection: SelectionState,
    // ... 其他字段
}
```

#### ✅ Checklist

- [ ] 2.1 创建 `selection/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct SelectionState {
      selected_block_ids: HashSet<BlockId>,
      document_selection: Option<DocumentSelection>,
      visual_caret_position: Option<VisualCaretPosition>,
      focused_text_selection: Option<FocusedTextSelection>,
  }

  impl SelectionState {
      pub fn new() -> Self { /* ... */ }

      // Getter/Setter 方法
      pub fn selected_block_ids(&self) -> &HashSet<BlockId> { /* ... */ }
      pub fn document_selection(&self) -> Option<&DocumentSelection> { /* ... */ }

      // 业务方法
      pub fn select_block(&mut self, block_id: BlockId) { /* ... */ }
      pub fn clear_selection(&mut self) { /* ... */ }
  }
  ```

- [ ] 2.2 在 `DocumentRuntime` 中替换字段
  ```rust
  pub struct DocumentRuntime {
      pub selection: SelectionState,
      // 移除原来的 4 个字段
  }
  ```

- [ ] 2.3 迁移选区相关方法
  - [ ] 识别所有操作选区的方法（搜索 `selected_block_ids`、`document_selection`）
  - [ ] 将这些方法移动到 `SelectionState::impl`
  - [ ] 在 `DocumentRuntime` 中添加委托方法（保持兼容）
    ```rust
    impl DocumentRuntime {
        pub fn select_block(&mut self, block_id: BlockId) {
            self.selection.select_block(block_id)
        }
    }
    ```

- [ ] 2.4 更新测试
  - [ ] 所有访问选区字段的测试改为通过方法访问
  - [ ] 添加 `SelectionState` 单元测试

- [ ] 2.5 验证编译和测试
  ```bash
  cargo test -p cditor-runtime --lib
  cargo test -p cditor-editor
  ```

**交付物**:
- `selection/state.rs` 文件
- 所有测试通过的 commit

---

### Phase 3: 编辑状态独立（1 周）

**目标**: 将编辑会话、撤销栈、输入热路径提取为 `EditingState`

#### 🎯 目标结构

```rust
pub struct EditingState {
    pub editing_session: Option<EditingSession>,
    pub undo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub redo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub external_undo_stack: UndoStack,
    pub typing_undo_group: Option<TypingUndoGroup>,
    pub pending_typing_undo: Option<TypingUndoRequest>,
    pub typing_mark_override: Option<TypingMarkOverride>,
    pub undo_events: Vec<RuntimeUndoEvent>,
    pub redo_events: Vec<RuntimeUndoEvent>,
    pub hot_path: SingleCharInputHotPath,
    pub text_models: HashMap<BlockId, PieceTableTextModel>,
}

pub struct DocumentRuntime {
    pub selection: SelectionState,
    pub editing: EditingState,
    // ... 其他字段
}
```

#### ✅ Checklist

- [ ] 3.1 创建 `editing/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct EditingState {
      editing_session: Option<EditingSession>,
      undo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
      redo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
      external_undo_stack: UndoStack,
      typing_undo_group: Option<TypingUndoGroup>,
      pending_typing_undo: Option<TypingUndoRequest>,
      typing_mark_override: Option<TypingMarkOverride>,
      undo_events: Vec<RuntimeUndoEvent>,
      redo_events: Vec<RuntimeUndoEvent>,
      hot_path: SingleCharInputHotPath,
      text_models: HashMap<BlockId, PieceTableTextModel>,
  }

  impl EditingState {
      pub fn new() -> Self { /* ... */ }

      // 撤销/重做
      pub fn can_undo(&self, block_id: &BlockId) -> bool { /* ... */ }
      pub fn can_redo(&self, block_id: &BlockId) -> bool { /* ... */ }
      pub fn push_undo(&mut self, block_id: BlockId, snapshot: TextSnapshot) { /* ... */ }

      // 输入热路径
      pub fn process_single_char(&mut self, ch: char) -> InputHotPathResult { /* ... */ }
  }
  ```

- [ ] 3.2 在 `DocumentRuntime` 中替换字段
  - [ ] 移除原来的 11 个编辑相关字段
  - [ ] 添加 `pub editing: EditingState`

- [ ] 3.3 迁移编辑相关方法
  - [ ] 搜索所有访问 `undo_stacks`、`redo_stacks`、`hot_path` 的方法
  - [ ] 将方法移到 `EditingState::impl`
  - [ ] 在 `DocumentRuntime` 中添加委托方法

- [ ] 3.4 更新 `document_runtime/text_edit.rs`
  - [ ] 改为调用 `self.editing.xxx()` 而非直接访问字段

- [ ] 3.5 更新 `document_runtime/undo_redo.rs`
  - [ ] 重构为 `EditingState` 的方法

- [ ] 3.6 验证测试
  ```bash
  cargo test -p cditor-runtime -- text_edit
  cargo test -p cditor-runtime -- undo_redo
  ```

**交付物**:
- `editing/state.rs` 文件
- 所有测试通过的 commit

---

### Phase 4: 布局与滚动独立（1 周）

**目标**: 提取布局和滚动相关状态

#### 🎯 目标结构

```rust
pub struct LayoutState {
    pub height_index: BlockHeightIndex,
    pub page_layout: PageLayoutIndex,
    pub pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
    pub layout_dirty: bool,
}

pub struct ScrollState {
    pub scroll: VirtualScrollState,
    pub scrollbar_drag: Option<ScrollbarDragSession>,
    pub window_planner: WindowPlanner,
    pub last_planned_scroll_top: f64,
    pub window_plan_clock_ms: u64,
}

pub struct DocumentRuntime {
    pub selection: SelectionState,
    pub editing: EditingState,
    pub layout: LayoutState,
    pub scroll: ScrollState,
    // ... 其他字段
}
```

#### ✅ Checklist

- [ ] 4.1 创建 `layout/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct LayoutState {
      height_index: BlockHeightIndex,
      page_layout: PageLayoutIndex,
      pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
      layout_dirty: bool,
  }

  impl LayoutState {
      pub fn mark_dirty(&mut self) { /* ... */ }
      pub fn is_dirty(&self) -> bool { /* ... */ }
      pub fn update_height(&mut self, block_id: BlockId, height: f64) { /* ... */ }
  }
  ```

- [ ] 4.2 创建 `scroll/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct ScrollState {
      scroll: VirtualScrollState,
      scrollbar_drag: Option<ScrollbarDragSession>,
      window_planner: WindowPlanner,
      last_planned_scroll_top: f64,
      window_plan_clock_ms: u64,
  }

  impl ScrollState {
      pub fn scroll_to(&mut self, offset: f64) { /* ... */ }
      pub fn scroll_by(&mut self, delta: f64) { /* ... */ }
  }
  ```

- [ ] 4.3 在 `DocumentRuntime` 中替换字段
  - [ ] 移除 9 个布局/滚动字段
  - [ ] 添加 `pub layout: LayoutState`
  - [ ] 添加 `pub scroll: ScrollState`

- [ ] 4.4 迁移方法
  - [ ] `document_runtime/layout_heights.rs` → `LayoutState`
  - [ ] `document_runtime/scroll.rs` → `ScrollState`

- [ ] 4.5 验证测试
  ```bash
  cargo test -p cditor-runtime -- layout
  cargo test -p cditor-runtime -- scroll
  cargo test -p cditor-editor-core
  ```

**交付物**:
- `layout/state.rs` 和 `scroll/state.rs`
- 所有测试通过的 commit

---

### Phase 5: AI 与 Payload 独立（1 周）

**目标**: 提取 AI 会话和 Payload 窗口状态

#### 🎯 目标结构

```rust
pub struct AiState {
    pub ai_session: Option<RuntimeAiSession>,
    pub next_ai_request_id: u64,
}

pub struct PayloadState {
    pub payload_window: PayloadWindow,
    pub payload_window_generation: u64,
    pub window_memory_pressure: WindowMemoryPressure,
    pub demo_payload_count: Option<usize>,
}

pub struct DocumentRuntime {
    pub selection: SelectionState,
    pub editing: EditingState,
    pub layout: LayoutState,
    pub scroll: ScrollState,
    pub ai: AiState,
    pub payload: PayloadState,
    // ... 其他字段
}
```

#### ✅ Checklist

- [ ] 5.1 创建 `ai/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct AiState {
      ai_session: Option<RuntimeAiSession>,
      next_ai_request_id: u64,
  }

  impl AiState {
      pub fn start_session(&mut self, config: AiConfig) -> AiSessionId { /* ... */ }
      pub fn cancel_session(&mut self) { /* ... */ }
      pub fn next_request_id(&mut self) -> u64 { /* ... */ }
  }
  ```

- [ ] 5.2 创建 `payload/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct PayloadState {
      payload_window: PayloadWindow,
      payload_window_generation: u64,
      window_memory_pressure: WindowMemoryPressure,
      demo_payload_count: Option<usize>,
  }

  impl PayloadState {
      pub fn update_window(&mut self, window: PayloadWindow) { /* ... */ }
      pub fn generation(&self) -> u64 { /* ... */ }
  }
  ```

- [ ] 5.3 在 `DocumentRuntime` 中替换字段

- [ ] 5.4 迁移方法
  - [ ] `document_runtime/ai.rs` → `AiState`
  - [ ] `document_runtime/payload_window.rs` → `PayloadState`
  - [ ] `document_runtime/payload_hydration.rs` → `PayloadState`

- [ ] 5.5 验证测试
  ```bash
  cargo test -p cditor-runtime -- ai
  cargo test -p cditor-runtime -- payload
  ```

**交付物**:
- `ai/state.rs` 和 `payload/state.rs`
- 所有测试通过的 commit

---

### Phase 6: 内容状态整合（1 周）

**目标**: 提取块属性、表格、媒体、集合等内容状态

#### 🎯 目标结构

```rust
pub struct ContentState {
    pub block_attrs: HashMap<BlockId, BlockAttrs>,
    pub table_runtimes: HashMap<BlockId, TableRuntime>,
    pub table_horizontal_scroll_offsets: HashMap<BlockId, f32>,
    pub collection_records: HashMap<CollectionId, Vec<CollectionRecordSnapshot>>,
    pub comment_threads: HashMap<CommentThreadId, CommentThreadSnapshot>,
    pub assets: HashMap<AssetId, AssetSnapshot>,
    pub block_asset_ids: HashMap<BlockId, BTreeSet<AssetId>>,
    pub list_projection_cache: ListProjectionCache,
}

pub struct DocumentRuntime {
    // 核心状态
    pub document_id: DocumentId,
    pub document_title: Option<String>,
    pub revision: u64,
    pub index: DocumentIndex,
    pub visible_index: VisibleDocumentIndex,

    // 子系统
    pub selection: SelectionState,
    pub editing: EditingState,
    pub layout: LayoutState,
    pub scroll: ScrollState,
    pub ai: AiState,
    pub payload: PayloadState,
    pub content: ContentState,

    // 焦点状态（小模块，暂不独立）
    pub focused_table_cell: Option<FocusedTableCell>,
    pub focused_inner_selection: Option<FocusedInnerSelection>,

    // 事务状态（小模块，暂不独立）
    pub pending_structure_transactions: Vec<EditTransaction>,
    pub last_committed_transaction_id: Option<u64>,
    pub next_transaction_id: u64,
    pub next_input_session_id: u64,
}
```

#### ✅ Checklist

- [ ] 6.1 创建 `content/state.rs`
  ```rust
  #[derive(Debug)]
  pub struct ContentState {
      block_attrs: HashMap<BlockId, BlockAttrs>,
      table_runtimes: HashMap<BlockId, TableRuntime>,
      table_horizontal_scroll_offsets: HashMap<BlockId, f32>,
      collection_records: HashMap<CollectionId, Vec<CollectionRecordSnapshot>>,
      comment_threads: HashMap<CommentThreadId, CommentThreadSnapshot>,
      assets: HashMap<AssetId, AssetSnapshot>,
      block_asset_ids: HashMap<BlockId, BTreeSet<AssetId>>,
      list_projection_cache: ListProjectionCache,
  }

  impl ContentState {
      pub fn get_block_attrs(&self, block_id: &BlockId) -> Option<&BlockAttrs> { /* ... */ }
      pub fn set_block_attrs(&mut self, block_id: BlockId, attrs: BlockAttrs) { /* ... */ }
      pub fn get_table_runtime(&self, block_id: &BlockId) -> Option<&TableRuntime> { /* ... */ }
  }
  ```

- [ ] 6.2 在 `DocumentRuntime` 中替换字段
  - [ ] 移除 8 个内容相关字段
  - [ ] 添加 `pub content: ContentState`

- [ ] 6.3 迁移方法
  - [ ] `document_runtime/block_attrs.rs` → `ContentState`
  - [ ] `document_runtime/table.rs` → `ContentState`
  - [ ] `document_runtime/media.rs` → `ContentState`

- [ ] 6.4 验证测试
  ```bash
  cargo test -p cditor-runtime -- content
  cargo test -p cditor-runtime -- table
  ```

**交付物**:
- `content/state.rs`
- 重构后的 `DocumentRuntime` 结构
- 所有测试通过的 commit

---

### Phase 7: API 兼容性验证（1 周）

**目标**: 确保外部调用者无感知，性能无回退

#### ✅ Checklist

- [ ] 7.1 API 兼容性测试
  - [ ] 运行 `cditor-api` 的所有测试
  - [ ] 运行 `cditor-editor` 的所有测试
  - [ ] 运行 `cditor-app` 的集成测试

- [ ] 7.2 性能回归测试
  ```bash
  cargo bench --bench document_runtime
  ```
  - [ ] 对比 Phase 0 的性能基线
  - [ ] 确保关键路径性能波动 < ±5%
  - [ ] 如有回退，分析原因并优化

- [ ] 7.3 内存占用分析
  - [ ] 对比重构前后 `DocumentRuntime` 的内存大小
  - [ ] 确保没有引入额外的堆分配

- [ ] 7.4 代码审查
  - [ ] 检查所有 `pub` 接口的文档
  - [ ] 检查所有新增的 `unwrap()` 和 `expect()`
  - [ ] 检查所有 `clone()` 调用是否必要

- [ ] 7.5 更新文档
  - [ ] 更新 `doc/architecture/project-structure.md`
  - [ ] 添加 `doc/architecture/document-runtime-architecture.md`
  - [ ] 更新 README 中的架构图

**交付物**:
- 性能对比报告
- 架构文档更新
- 代码审查报告

---

## 📐 重构原则

### 1. 渐进式，可随时停止

每个 Phase 都是独立的，可以合并到主分支。如果时间不足，可以在任何 Phase 后停止。

### 2. 保持 API 兼容

所有公开方法必须保持签名不变，内部实现可以委托给子状态。

示例：
```rust
// 外部调用者无需改动
impl DocumentRuntime {
    pub fn select_block(&mut self, block_id: BlockId) {
        self.selection.select_block(block_id)  // 委托
    }
}
```

### 3. 测试驱动

每个 Phase 必须通过以下检查才能合并：
- [ ] 所有现有测试通过
- [ ] 新增子系统的单元测试
- [ ] Benchmark 性能波动 < ±5%

### 4. 小步提交

每完成一个子任务就提交，commit message 格式：
```
refactor(runtime): [Phase X.Y] 简短描述

- 详细变更 1
- 详细变更 2

Relates-to: #issue-number
```

---

## 🔍 验收标准

### 功能验收

- [ ] 所有 372 个测试通过（包括 runtime、editor、app）
- [ ] 手动测试核心场景：
  - [ ] 打开 10,000 block 的大文档
  - [ ] 输入中文（IME）
  - [ ] 复制粘贴跨 block
  - [ ] 撤销/重做 100 次
  - [ ] AI 内联编辑
  - [ ] 表格操作

### 性能验收

- [ ] 输入延迟 < 16ms (60 FPS)
- [ ] 滚动帧率 > 120 FPS
- [ ] 内存占用增长 < 5%
- [ ] 冷启动时间不变

### 代码质量验收

- [ ] `DocumentRuntime` 字段数量 < 20 个
- [ ] 单个子系统字段数量 < 12 个
- [ ] 没有新增 `unsafe` 代码
- [ ] 没有新增 TODO/FIXME（除非有 issue 跟踪）

### 文档验收

- [ ] 每个子系统有独立的文档说明
- [ ] 架构图更新
- [ ] API 文档完整（所有 pub 项有 doc comment）

---

## 🚨 回滚计划

如果重构过程中遇到不可解决的问题：

### 立即回滚条件

- 关键功能损坏且 48 小时内无法修复
- 性能回退 > 20% 且无优化方案
- 引入数据损坏 bug

### 回滚步骤

```bash
# 1. 切回主分支
git checkout main

# 2. 删除重构分支（如果已合并部分，需要 revert）
git branch -D refactor/document-runtime-modularization

# 3. 如果已合并到主分支
git revert <merge-commit-hash>
```

### 部分保留

如果某些 Phase 已经稳定，可以：
- 保留已完成的 Phase 1-3
- 放弃未完成的 Phase 4-7
- 创建技术债务 issue 跟踪

---

## 📅 里程碑与进度跟踪

### 时间线

| Phase | 开始日期 | 结束日期 | 负责人 | 状态 |
|-------|---------|---------|--------|------|
| Phase 0 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 1 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 2 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 3 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 4 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 5 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 6 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 7 | TBD | TBD | TBD | ⬜ 未开始 |

### 状态标记

- ⬜ 未开始
- 🟦 进行中
- ✅ 已完成
- ⚠️ 遇到阻塞
- ❌ 已放弃

---

## 🤝 协作与沟通

### 每日站会

- 时间：每天早上 10:00
- 内容：
  - 昨天完成了什么
  - 今天计划做什么
  - 遇到什么阻塞

### 每周回顾

- 时间：每周五下午 4:00
- 内容：
  - 本周完成的 Phase
  - 遇到的技术难点
  - 下周计划
  - 风险评估更新

### 代码审查

- 每个 Phase 完成后必须经过代码审查
- 至少 1 个 reviewer 批准才能合并
- 审查重点：
  - API 兼容性
  - 测试覆盖率
  - 性能影响
  - 代码可读性

---

## 📚 参考资料

- [当前架构文档](../architecture/project-structure.md)
- [大文档架构设计](../large-document-rich-text-architecture.md)
- [God Object 反模式](https://en.wikipedia.org/wiki/God_object)
- [渐进式重构最佳实践](https://martinfowler.com/books/refactoring.html)

---

## 📝 变更日志

| 日期 | 变更内容 | 变更人 |
|------|---------|--------|
| 2026-07-22 | 创建初始重构方案 | Claude |

---

**最后更新**: 2026-07-22
**文档版本**: v1.0
**状态**: 规划中
