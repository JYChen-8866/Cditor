# cditor-editor 模块化重构方案

## 📋 文档元信息

- **创建时间**: 2026-07-22
- **状态**: 规划中
- **优先级**: P1 (重要，但依赖 DocumentRuntime 重构完成)
- **预计工期**: 3-4 周
- **风险等级**: 中（影响面广但改动相对独立）
- **前置条件**: DocumentRuntime 重构完成（Phase 1-6）

## 🎯 重构目标

### 当前问题

`cditor-editor` 是一个 **116 个文件、36,754 行代码**的巨型 crate：
- 混杂了 UI 组件、业务逻辑、输入处理、持久化桥接
- 依赖 11 个内部 crate + 5 个外部库
- 单一 crate 难以并行开发

### 目标状态

拆分为 4-5 个独立的 crate：
```
cditor-editor-blocks     # Block 渲染组件（23 个文件）
cditor-editor-overlay    # 浮层组件（6 个文件）
cditor-editor-input      # 输入处理（11 个文件）
cditor-editor-view       # 核心视图逻辑（剩余 76 个文件）
```

---

## 🗺️ 重构路线图

### Phase 1: Block 组件独立（1 周）

**目标**: 将 `block/` 目录提取为独立的 `cditor-editor-blocks` crate

#### 📦 新 Crate 结构

```
crates/cditor-editor-blocks/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── code/          # 代码块（5 个文件）
│   ├── table/         # 表格（19 个文件）
│   ├── mermaid/       # Mermaid（6 个文件）
│   ├── whiteboard/    # 白板（6 个文件）
│   ├── media.rs
│   ├── heading.rs
│   ├── paragraph.rs
│   ├── quote.rs
│   ├── list.rs
│   └── ...
```

#### ✅ Checklist

- [ ] 1.1 创建新 crate
  ```bash
  mkdir -p crates/cditor-editor-blocks/src
  ```

- [ ] 1.2 编写 `Cargo.toml`
  ```toml
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
  gpui = { ... }
  lumis = { ... }  # 语法高亮
  ```

- [ ] 1.3 复制 `block/` 目录到新 crate

- [ ] 1.4 定义公开接口
  ```rust
  // src/lib.rs
  pub mod code;
  pub mod table;
  pub mod mermaid;
  pub mod whiteboard;
  // ...

  pub trait BlockRenderer {
      fn render(&self, ctx: &BlockRenderContext) -> impl IntoElement;
  }
  ```

- [ ] 1.5 在 `cditor-editor` 中替换依赖
  ```toml
  # crates/cditor-editor/Cargo.toml
  cditor-editor-blocks = { path = "../cditor-editor-blocks" }
  ```

- [ ] 1.6 验证编译和测试
  ```bash
  cargo test -p cditor-editor-blocks
  cargo test -p cditor-editor
  ```

**交付物**:
- `cditor-editor-blocks` crate
- 所有测试通过

---

### Phase 2: Overlay 组件独立（1 周）

**目标**: 将 `overlay/` 目录提取为 `cditor-editor-overlay` crate

#### 📦 新 Crate 结构

```
crates/cditor-editor-overlay/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── floating_toolbar.rs
│   ├── slash_menu.rs
│   ├── color_menu.rs
│   ├── ai_inline.rs
│   ├── toast.rs
│   └── whiteboard_editor.rs
```

#### ✅ Checklist

- [ ] 2.1 创建新 crate
  ```bash
  mkdir -p crates/cditor-editor-overlay/src
  ```

- [ ] 2.2 编写 `Cargo.toml`
  ```toml
  [dependencies]
  cditor-core = { path = "../cditor-core" }
  cditor-runtime = { path = "../cditor-runtime" }
  cditor-theme = { path = "../cditor-theme" }
  gpui = { ... }
  ```

- [ ] 2.3 复制 `overlay/` 目录

- [ ] 2.4 定义公开接口
  ```rust
  pub fn render_floating_toolbar(...) -> impl IntoElement;
  pub fn render_slash_menu(...) -> impl IntoElement;
  pub fn render_color_menu(...) -> impl IntoElement;
  // ...
  ```

- [ ] 2.5 在 `cditor-editor` 中替换依赖

- [ ] 2.6 验证编译

**交付物**:
- `cditor-editor-overlay` crate

---

### Phase 3: Input 处理独立（1 周）

**目标**: 将 `app/input/` 目录提取为 `cditor-editor-input` crate

#### 📦 新 Crate 结构

```
crates/cditor-editor-input/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── actions.rs      # 输入动作定义
│   ├── keyboard.rs     # 键盘事件处理
│   ├── ime.rs          # IME 输入
│   ├── mouse.rs        # 鼠标事件
│   ├── clipboard.rs    # 剪贴板
│   └── ...
```

#### ✅ Checklist

- [ ] 3.1 创建新 crate

- [ ] 3.2 编写 `Cargo.toml`
  ```toml
  [dependencies]
  cditor-core = { path = "../cditor-core" }
  cditor-runtime = { path = "../cditor-runtime" }
  gpui = { ... }
  ```

- [ ] 3.3 复制 `app/input/` 目录

- [ ] 3.4 定义公开接口
  ```rust
  pub struct InputRouter {
      // ...
  }

  impl InputRouter {
      pub fn handle_keyboard_event(&mut self, event: &KeyEvent) -> InputResult;
      pub fn handle_ime_event(&mut self, event: &ImeEvent) -> InputResult;
  }
  ```

- [ ] 3.5 验证编译

**交付物**:
- `cditor-editor-input` crate

---

### Phase 4: 核心视图重组（1 周）

**目标**: 将剩余的核心视图逻辑保留在 `cditor-editor` 中，清理依赖

#### 📦 最终结构

```
crates/cditor-editor/
├── Cargo.toml          # 依赖大幅减少
├── src/
│   ├── lib.rs
│   ├── app/
│   │   ├── cditor_v2_view.rs
│   │   ├── command_router.rs
│   │   ├── lifecycle.rs
│   │   ├── render.rs
│   │   ├── sdk.rs
│   │   └── ...
│   ├── document/
│   ├── text/
│   └── persistence/
```

#### ✅ Checklist

- [ ] 4.1 更新 `cditor-editor/Cargo.toml`
  ```toml
  [dependencies]
  cditor-editor-blocks = { path = "../cditor-editor-blocks" }
  cditor-editor-overlay = { path = "../cditor-editor-overlay" }
  cditor-editor-input = { path = "../cditor-editor-input" }
  # 移除 lumis、mermaid_render 等直接依赖
  ```

- [ ] 4.2 更新 `src/app/render.rs`
  ```rust
  use cditor_editor_blocks::{render_code_block, render_table_block};
  use cditor_editor_overlay::{render_floating_toolbar, render_slash_menu};
  ```

- [ ] 4.3 验证整体编译
  ```bash
  cargo build --workspace
  cargo test --workspace
  ```

**交付物**:
- 重构后的 `cditor-editor` crate
- 所有测试通过

---

## 📐 依赖关系图

### 重构前
```
cditor-editor
├── cditor-ai
├── cditor-api
├── cditor-core
├── cditor-import-export
├── cditor-text
├── cditor-editor-core
├── cditor-runtime
├── cditor-storage
├── cditor-theme
├── ding-board
├── gpui
├── lumis
├── mermaid_render
├── reqwest
└── image
```

### 重构后
```
cditor-editor-blocks
├── cditor-core
├── cditor-runtime
├── cditor-text
├── cditor-theme
├── gpui
├── lumis (语法高亮)
└── mermaid_render

cditor-editor-overlay
├── cditor-core
├── cditor-runtime
├── cditor-theme
├── gpui
└── ding-board

cditor-editor-input
├── cditor-core
├── cditor-runtime
└── gpui

cditor-editor (瘦身后)
├── cditor-editor-blocks
├── cditor-editor-overlay
├── cditor-editor-input
├── cditor-api
├── cditor-text
├── cditor-runtime
├── cditor-storage
├── cditor-theme
├── gpui
├── reqwest (仅用于图片加载)
└── image
```

---

## 🎯 验收标准

### 功能验收
- [ ] 所有现有测试通过
- [ ] 手动测试核心场景无回退

### 代码质量验收
- [ ] 每个 crate 文件数 < 30 个
- [ ] 每个 crate 依赖数 < 8 个
- [ ] 编译时间减少 > 20%（并行编译）

### 架构验收
- [ ] 依赖关系清晰（无循环依赖）
- [ ] 每个 crate 有明确的职责边界
- [ ] 公开接口文档完整

---

## 📅 时间线

| Phase | 开始日期 | 结束日期 | 负责人 | 状态 |
|-------|---------|---------|--------|------|
| Phase 1 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 2 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 3 | TBD | TBD | TBD | ⬜ 未开始 |
| Phase 4 | TBD | TBD | TBD | ⬜ 未开始 |

---

**最后更新**: 2026-07-22
**文档版本**: v1.0
**状态**: 规划中
