# 重构快速参考清单

## 🚀 开始重构前

### ✅ 前置检查清单

```bash
# 1. 确保所有测试通过
cargo test --workspace
# 预期：372 tests passed

# 2. 记录性能基线
cargo bench --bench document_runtime > baseline.txt

# 3. 检查编译时间基线
time cargo build --workspace --release

# 4. 创建重构分支
git checkout -b refactor/document-runtime-modularization

# 5. 冻结当前状态
git tag refactor-baseline-$(date +%Y%m%d)
```

---

## 📦 DocumentRuntime 重构速查

### 当前结构（58 个字段）

```rust
pub struct DocumentRuntime {
    // 核心（7 个）
    document_id, document_title, revision, index,
    visible_index, height_index, page_layout,

    // 选区（4 个）
    selected_block_ids, document_selection,
    visual_caret_position, focused_text_selection,

    // 编辑（11 个）
    editing, text_models, undo_stacks, redo_stacks,
    external_undo_stack, typing_undo_group,
    pending_typing_undo, typing_mark_override,
    undo_events, redo_events, hot_path,

    // 布局（4 个）
    scroll, pending_measured_heights, layout_dirty,
    scrollbar_drag,

    // 窗口（3 个）
    window_planner, last_planned_scroll_top,
    window_plan_clock_ms,

    // Payload（4 个）
    payload_window, payload_window_generation,
    window_memory_pressure, demo_payload_count,

    // AI（2 个）
    ai_session, next_ai_request_id,

    // 内容（8 个）
    block_attrs, table_runtimes,
    table_horizontal_scroll_offsets,
    collection_records, comment_threads, assets,
    block_asset_ids, list_projection_cache,

    // 焦点（3 个）
    focused_table_cell, focused_inner_selection,

    // 事务（4 个）
    pending_structure_transactions,
    last_committed_transaction_id,
    next_transaction_id, next_input_session_id,
}
```

### 目标结构（7 个子系统）

```rust
pub struct DocumentRuntime {
    // 核心元信息
    pub document_id: DocumentId,
    pub document_title: Option<String>,
    pub revision: u64,
    pub index: DocumentIndex,
    pub visible_index: VisibleDocumentIndex,

    // 子系统
    pub selection: SelectionState,     // 4 字段
    pub editing: EditingState,         // 11 字段
    pub layout: LayoutState,           // 4 字段
    pub scroll: ScrollState,           // 5 字段
    pub ai: AiState,                   // 2 字段
    pub payload: PayloadState,         // 4 字段
    pub content: ContentState,         // 8 字段

    // 小模块（暂不独立）
    pub focused_table_cell: Option<FocusedTableCell>,
    pub focused_inner_selection: Option<FocusedInnerSelection>,
    pub pending_structure_transactions: Vec<EditTransaction>,
    pub last_committed_transaction_id: Option<u64>,
    pub next_transaction_id: u64,
    pub next_input_session_id: u64,
}
```

---

## 🛠️ 常用重构命令

### 移动文件到子目录

```bash
# Phase 1: 目录重组
cd crates/cditor-runtime/src/document_runtime

# 创建子目录
mkdir -p selection editing transaction layout scroll payload

# 移动文件
mv selection*.rs selection/
mv text_edit.rs text_navigation.rs editing/
mv transaction*.rs transaction/

# 更新 mod.rs
echo "pub mod selection;" >> selection/mod.rs
echo "pub use selection::*;" >> selection/mod.rs
```

### 提取子状态结构体

```bash
# Phase 2: 选区独立
cat > selection/state.rs << 'EOF'
#[derive(Debug)]
pub struct SelectionState {
    selected_block_ids: HashSet<BlockId>,
    document_selection: Option<DocumentSelection>,
    visual_caret_position: Option<VisualCaretPosition>,
    focused_text_selection: Option<FocusedTextSelection>,
}

impl SelectionState {
    pub fn new() -> Self { Self::default() }
}
EOF
```

### 验证每个阶段

```bash
# 编译检查
cargo check -p cditor-runtime

# 运行测试
cargo test -p cditor-runtime

# 完整测试（包括依赖方）
cargo test -p cditor-runtime -p cditor-editor -p cditor-app

# 性能回归检查
cargo bench --bench document_runtime | grep "time:"
```

---

## 🎯 cditor-editor 拆分速查

### 当前结构（116 个文件）

```
cditor-editor/src/
├── app/          20 文件（视图、输入、交互）
├── block/        23 文件（各类块渲染）
├── overlay/       6 文件（浮层组件）
├── text/          4 文件（文本引擎）
├── document/      8 文件（文档视图）
└── 其他          55 文件
```

### 拆分目标（4 个 crate）

```
cditor-editor-blocks    ← block/ (23 文件)
cditor-editor-overlay   ← overlay/ (6 文件)
cditor-editor-input     ← app/input/ (11 文件)
cditor-editor-view      ← 其余 (76 文件)
```

### 创建新 crate 模板

```bash
# 创建 cditor-editor-blocks
mkdir -p crates/cditor-editor-blocks/src
cat > crates/cditor-editor-blocks/Cargo.toml << 'EOF'
[package]
name = "cditor-editor-blocks"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
cditor-core = { path = "../cditor-core" }
cditor-runtime = { path = "../cditor-runtime" }
cditor-text = { path = "../cditor-text" }
cditor-theme = { path = "../cditor-theme" }
gpui = { git = "...", ... }
lumis = { version = "0.12.0", ... }
EOF

# 复制文件
cp -r crates/cditor-editor/src/block/* crates/cditor-editor-blocks/src/

# 更新 workspace
# 在 Cargo.toml 的 [workspace.members] 中添加
echo '    "crates/cditor-editor-blocks",' >> Cargo.toml
```

---

## 📊 验收检查清单

### 功能完整性

```bash
# ✅ 所有测试通过
cargo test --workspace --no-fail-fast

# ✅ 手动场景测试
cargo run --release
# - 打开大文档（10,000 blocks）
# - 输入中文
# - 复制粘贴
# - 撤销重做
# - AI 内联编辑
```

### 性能验收

```bash
# ✅ 对比基线
cargo bench --bench document_runtime > new_baseline.txt
diff baseline.txt new_baseline.txt

# ✅ 编译时间
time cargo build --workspace --release
# 预期：减少 20%（并行编译）
```

### 代码质量

```bash
# ✅ 无编译警告
cargo clippy --workspace -- -D warnings

# ✅ 格式化
cargo fmt --all -- --check

# ✅ 文档完整
cargo doc --workspace --no-deps
```

---

## 🔄 每日工作流

### 开始一天的工作

```bash
# 1. 同步主分支
git fetch origin main
git rebase origin/main

# 2. 确认测试基线
cargo test --workspace

# 3. 查看今天的任务
# 见重构文档中的 Checklist
```

### 提交一个改动

```bash
# 1. 运行测试
cargo test -p cditor-runtime

# 2. 格式化代码
cargo fmt

# 3. 提交
git add .
git commit -m "refactor(runtime): [Phase 2.3] extract SelectionState

- Move 4 selection fields to SelectionState
- Add delegation methods in DocumentRuntime
- All tests passing

Relates-to: #XXX"
```

### 结束一天的工作

```bash
# 1. 推送分支
git push origin refactor/document-runtime-modularization

# 2. 更新进度
# 在重构文档中勾选完成的 Checklist

# 3. 记录遇到的问题
# 添加到 doc/refactor/issues.md
```

---

## 🚨 常见问题

### Q: 测试失败怎么办？

```bash
# 1. 查看失败的测试
cargo test --workspace 2>&1 | grep FAILED

# 2. 单独运行失败的测试
cargo test -p cditor-runtime test_name -- --nocapture

# 3. 如果是路径问题
# 检查 mod.rs 的 pub use 语句

# 4. 如果是字段访问问题
# 改为通过方法访问，而非直接访问字段
```

### Q: 编译时间太长？

```bash
# 使用增量编译
export CARGO_INCREMENTAL=1

# 并行编译
cargo build -j8

# 只编译当前 crate
cargo build -p cditor-runtime
```

### Q: 性能回退怎么办？

```bash
# 1. 定位瓶颈
cargo flamegraph --bench document_runtime

# 2. 检查是否引入多余的 clone
rg "\.clone\(\)" crates/cditor-runtime/src

# 3. 检查是否引入多余的分配
# 使用 heaptrack 或 valgrind
```

---

## 📚 有用的命令别名

```bash
# 添加到 ~/.zshrc 或 ~/.bashrc
alias ct="cargo test --workspace"
alias ctr="cargo test -p cditor-runtime"
alias cte="cargo test -p cditor-editor"
alias cb="cargo build --workspace"
alias cbr="cargo build --workspace --release"
alias cc="cargo clippy --workspace -- -D warnings"
alias cf="cargo fmt --all"
```

---

**最后更新**: 2026-07-22
**使用提示**: 打印此文档，放在手边随时查阅
