# Cditor 模块拆分计划（历史记录）

> 本文记录 2026-07-06 的拆分过程，路径与依赖边界不代表当前实现。当前结构以 [`doc/architecture/project-structure.md`](../../architecture/project-structure.md) 为准。

> 目标：降低巨石文件复杂度，同时不破坏 V2 大文档架构：Runtime 是真相，UI 只消费 projection，Postgres 异步保存，Input/IME hot path 不同步等待存储或重型 layout。

## 当前问题

当前顶层目录 `api / core / editor / runtime / gui / storage` 基本合理，但少数文件职责过大：

| 文件 | 问题 | 优先级 |
| --- | --- | --- |
| `src/runtime/document_runtime.rs` | 运行时真相层所有逻辑混在一起：加载、编辑、结构、paste、layout、scroll、undo、projection、测试 | 高 |
| `src/gui/app/cditor_v2_view.rs` | View 同时承担 render、IME、键盘、鼠标、滚动条、块拖拽、图片 resize、Postgres 保存状态 | 高 |
| `src/core/edit/mod.rs` | selection / transaction / undo / inline mark 混在一起 | 中 |
| `src/storage/postgres/types.rs` | DB DTO、serde 映射、ID 转换、transaction 编解码混在一起 | 中 |
| `src/core/rich_text/markdown.rs` | markdown block/inline/table/list 解析混在一起 | 中 |
| `src/gui/text/element.rs` | GPUI text element、paint、hit-test、caret/selection 绘制混在一起 | 中 |

## 拆分原则

1. 先拆低风险纯 helper，再拆事件处理，再拆 runtime hot path。
2. 每次拆分不改行为，只移动代码和调整可见性。
3. 每完成一组必须运行：
   ```sh
   cargo fmt
   cargo check
   ```
4. 涉及 runtime 后必须运行：
   ```sh
   cargo test runtime::document_runtime --lib
   ```
5. 涉及 GUI input / IME 后必须运行：
   ```sh
   cargo test gui::app --lib
   cargo check
   ```
6. 拆分完成后分阶段提交，避免大范围重构难以回滚。

## 目标目录结构

### GUI App

```text
src/gui/app/
  mod.rs
  cditor_v2_view.rs          # 临时保留主文件，逐步瘦身
  input_trace.rs             # GUI input trace helper
  state.rs                   # CditorViewState，后续拆
  render.rs                  # Render impl，后续拆
  persistence.rs             # save/autosave glue，后续拆
  input/
    mod.rs
    keyboard.rs
    ime.rs
    mouse.rs
    text_drag.rs
  interaction/
    mod.rs
    geometry.rs
    gutter_drag.rs
    image_resize.rs
    scrollbar.rs
```

### Runtime

```text
src/runtime/document_runtime/
  mod.rs
  state.rs
  constructors.rs
  store_loading.rs
  focus.rs
  payload_window.rs
  projection.rs
  layout_heights.rs
  scroll.rs
  text_edit.rs
  composition.rs
  selection.rs
  markdown_paste.rs
  structure_edit.rs
  media.rs
  undo_redo.rs
  helpers.rs
```

### Postgres Types

```text
src/storage/postgres/types/
  mod.rs
  ids.rs
  attrs.rs
  payload.rs
  inline.rs
  transactions.rs
  index_snapshot.rs
  block_kind.rs
```

## 任务清单

### Phase 0：基线

- [x] 初始化/确认 Git 工作树
- [x] 忽略运行时粘贴素材目录 `minimal-editor-assets/`
- [x] 提交拆分前基线：`41f76f1 Initial Cditor baseline`

### Phase 1：拆 `src/gui/app/cditor_v2_view.rs` 低风险模块

- [x] 拆出 `src/gui/app/input_trace.rs`
- [x] 拆出 `src/gui/app/interaction/geometry.rs`
- [x] 拆出 `src/gui/app/interaction/image_resize.rs`
- [x] 跑 `cargo fmt`
- [x] 跑 `cargo check`
- [x] 提交：`Refactor gui app low-risk interaction modules`

### Phase 2：继续拆 GUI interaction

- [x] 拆出 `src/gui/app/interaction/scrollbar.rs`
- [x] 拆出 `src/gui/app/interaction/gutter_drag.rs`
- [x] 跑 `cargo fmt`
- [x] 跑 `cargo check`
- [x] 提交：`Refactor gui app drag and scrollbar modules`

### Phase 3：拆 GUI input

- [x] 拆出 `src/gui/app/input/keyboard.rs`
- [x] 拆出 `src/gui/app/input/mouse.rs`
- [x] 拆出 `src/gui/app/input/text_drag.rs`
- [x] 拆出 `src/gui/app/input/ime.rs`
- [x] 跑 `cargo test gui::app --lib`
- [x] 跑 `cargo check`
- [x] 提交：`Refactor gui app input modules`

### Phase 4：拆 Runtime 非 hot path

- [x] 把 `src/runtime/document_runtime.rs` 转为目录模块 `src/runtime/document_runtime/mod.rs`
- [x] 拆出 constructors / store_loading
- [x] 拆出 payload_window
- [x] 拆出 layout_heights
- [x] 拆出 scroll
- [x] 拆出 media
- [x] 跑 `cargo test runtime::document_runtime --lib`
- [x] 跑 `cargo check`
- [x] 提交：`Refactor runtime media and layout height modules`
- [x] 提交：`Refactor runtime scroll and payload window modules`

### Phase 5：拆 Runtime hot path

- [x] 拆出 focus
- [x] 拆出 selection
- [x] 拆出 composition
- [x] 拆出 text_edit
- [x] 拆出 markdown_paste
- [x] 拆出 structure_edit
- [x] 拆出 undo_redo
- [x] 跑 `cargo test runtime::document_runtime --lib`
- [x] 跑 `cargo check`
- [ ] 手动验证 minimal editor 输入/IME/粘贴/图片/滚动
- [x] 提交：`Refactor runtime editing modules`

### Phase 6：拆其他大文件

- [x] 拆 `src/storage/postgres/types.rs`
- [x] 拆 `src/core/edit/mod.rs`
- [ ] 拆 `src/gui/text/element.rs`
- [x] 拆 `src/core/rich_text/markdown.rs`
  - [x] 拆出 inline parser
  - [x] 拆出 block shortcut/helper
  - [x] 拆出 table parser/export helper
  - [x] 拆出 plain markdown export
- [x] 跑相关模块测试和 `cargo check`
- [x] 提交：`Refactor remaining large modules`

## 当前执行记录

- 2026-07-06：创建本计划文档，准备开始 Phase 1。
- 2026-07-06：完成 Phase 1 第一组：拆出 `input_trace`、`interaction/geometry`、`interaction/image_resize`；验证 `cargo fmt && cargo test gui::app --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：完成 Phase 2 代码拆分：拆出 `interaction/scrollbar`、`interaction/gutter_drag`；验证 `cargo fmt && cargo test gui::app --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：完成 Phase 3：拆出 `input/keyboard`、`input/mouse`、`input/text_drag`、`input/ime`；验证 `cargo fmt && cargo test gui::app --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：开始 Phase 4：将 `document_runtime.rs` 转为目录模块，拆出 `media` 和 `layout_heights`；验证 `cargo test runtime::document_runtime --lib` 和 `cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 4：拆出 `scroll` 和 `payload_window`；验证 `cargo test runtime::document_runtime --lib` 通过，115 passed / 3 ignored。
- 2026-07-06：完成 Phase 4 剩余非 hot path：拆出 `constructors` 和 `store_loading`；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：开始 Phase 5：拆出 `focus` 基础聚焦模块；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 5：拆出 `selection` 基础文本/块选区模块；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 5：拆出 `composition` IME 组合输入模块；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 5：拆出 `text_edit` 文本输入、光标移动、退格/删除与 soft tab 相关逻辑；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 5：拆出 `markdown_paste` Markdown 粘贴、Markdown shortcut 和 imported block 插入逻辑；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 5：拆出 `undo_redo` 文本/结构 undo redo、snapshot restore 与结构 move transaction queue；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：完成 Phase 5：拆出 `structure_edit` 结构编辑、块移动、enter split、todo、inline mark、跨块删除与结构 transaction glue；验证 `cargo fmt && cargo test runtime::document_runtime --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：开始 Phase 6：将 `src/storage/postgres/types.rs` 转为目录模块，拆出 `ids`、`rows`、`attrs`、`payload`、`transactions`、`block_kind`；验证 `cargo fmt && cargo test storage::postgres::types --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 6：拆出 `core/edit` 的 `text_offsets`、`selection`、`transactions`、`undo` 子模块；验证 `cargo fmt && cargo test core::edit --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续 Phase 6：拆出 `core/rich_text/markdown/inline.rs`，将 inline markdown parser 从主文件移出；验证 `cargo fmt && cargo test core::rich_text::markdown --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：继续细拆 `core/rich_text/markdown.rs`：拆出 `block.rs`、`table.rs`、`export.rs`，主文件保留 parse orchestration 和 public API；验证 `cargo fmt && cargo test core::rich_text::markdown --lib && cargo check` 通过，仅保留原有 crate 命名 warning。
- 2026-07-06：拆分后较完整自动化验证通过：`cargo fmt && cargo test runtime::document_runtime --lib && cargo test gui::app --lib && cargo test gui::text --lib && cargo test core::edit --lib && cargo test core::rich_text::markdown --lib && cargo test storage::postgres::types --lib && cargo check`。`cargo run --example minimal_postgres_editor` 可启动并收到滚轮事件；完整 IME/粘贴/图片拖拽仍需人工窗口验证。
- 2026-07-06：发现 minimal editor 光标不可见/无法编辑，回滚 `gui/text/element.rs` 拆分，恢复单文件实现；该项重新标记为未完成，后续需先补 GUI 手动验证再拆。
- 2026-07-06：定位无法编辑的直接原因：Postgres cold start 初始 payload window 小于首屏 planned render range，导致 `projection_for_window_planned` 输出 `blocks=0 placeholder=true`；修复为 cold start 至少加载 256 个 payload，并让 minimal editor 修复 metadata 已存在但 index/payload 缺失的半初始化文档。
