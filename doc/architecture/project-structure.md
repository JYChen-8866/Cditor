# Cditor Project Structure

本文记录当前 workspace 的实际工程结构；长期设计原则仍以 [大文档富文本架构](../large-document-rich-text-architecture.md) 为准。

## Workspace

```text
crates/
  core/            文档、富文本、事务、selection 与布局基础模型
  store/           存储接口、缓存策略和持久化调度
  store-postgres/  PostgreSQL 实现与迁移
  editor/          无 GPUI 的视口、虚拟滚动、窗口规划和命中测试算法
  runtime/         活文档状态、编辑编排、投影和异步调度
  app/             GPUI 渲染、平台输入、overlay 和应用入口
  ai/              Inline AI provider 与配置加载
  ding-board/      独立白板产品 crate
```

目录名与 Cargo 包名的对应关系：

| 目录 | Cargo 包 |
| --- | --- |
| `core` | `cditor-core` |
| `store` | `cditor-storage` |
| `store-postgres` | `cditor-storage-postgres` |
| `editor` | `cditor-editor` |
| `runtime` | `cditor-runtime` |
| `app` | `cditor-app` |
| `ai` | `cditor-ai` |

## Dependency Direction

```text
core <- store <- store-postgres
  ^        ^          ^
  |        |          |
editor ---> runtime <- ai
              ^
              |
             app ----> ding-board
```

约束：

- `core` 不依赖 GPUI、SQLx 或具体存储。
- `editor` 是无 UI 框架依赖的视口算法层；GPUI 只存在于 `app`。
- `runtime` 持有文档、selection、layout height 与 scroll 真相。
- `app` 只消费 runtime projection 并转发输入，不成为文档真相。
- `ding-board` 保持独立，只由 `app` 适配。

当前 `runtime` 仍包含 PostgreSQL 冷启动适配，因此直接依赖 `store-postgres`。这是现状边界，不应继续扩散；新增存储能力优先落在 `store` 接口与 `store-postgres` 实现中。

## Source Layout Rules

- 非白板 Rust 源文件不超过 700 行；超限时按职责拆分实现和测试。
- 单元测试优先放在同级 `*_tests.rs` 或模块的 `tests/` 目录。
- 一次性迁移脚本只进入 `scripts/archive/`，不能混入日常开发入口。
- 历史迁移文档只进入 `doc/archive/`，当前设计与计划不得引用其旧目录作为现状。
- 根目录只保留 workspace、许可证、运行环境和顶层说明文件。

以上规则由 `scripts/dev/check_structure.sh` 检查。
