# Cditor 工程结构

本文记录 workspace 的当前边界、目录职责和强制约束。长期性能与数据模型设计以[大文档富文本架构](../large-document-rich-text-architecture.md)为准。

## Workspace 分层

```text
crates/
  core/            纯文档模型：Block、富文本、selection、transaction、layout metadata
  editor/          纯编辑器算法：窗口规划、虚拟滚动、anchor、hit test
  runtime/         活文档真相：编辑编排、payload window、projection、调度
  store/           存储契约、缓存策略、持久化状态机
  store-postgres/  PostgreSQL 查询、迁移、恢复、索引与 payload 实现
  ai/              AI provider、配置与流式响应协议
  app/             GPUI、平台输入、overlay、存储装配与应用入口
  ding-board/      独立白板产品 crate
```

目录名与 Cargo 包名对应如下：

| 目录 | Cargo 包 |
| --- | --- |
| `core` | `cditor-core` |
| `editor` | `cditor-editor` |
| `runtime` | `cditor-runtime` |
| `store` | `cditor-storage` |
| `store-postgres` | `cditor-storage-postgres` |
| `ai` | `cditor-ai` |
| `app` | `cditor-app` |
| `ding-board` | `ding-board` |

## 依赖方向

```text
core ────────> editor ──────> runtime ──────┐
  │                         ▲               │
  ├──────────> store        │               │
  │              │          │               ▼
  │              └──> store-postgres ─────> app <──── ding-board
  │                                         ▲
  └─────────────────────────────────────────┤
ai ────────────────────────> runtime ───────┘
```

箭头表示“被右侧依赖”。核心约束：

- `core` 是最底层纯模型，不依赖 GPUI、SQLx、网络或具体存储。
- `editor` 只做无 UI 框架的算法，不持有 GPUI Entity。
- `runtime` 持有文档、selection、layout height 与 scroll 真相，只接受中立的冷启动数据和 payload window 结果；不得知道 PostgreSQL、SQLx 或 GPUI。
- `store` 定义存储通用能力；`store-postgres` 提供具体实现。
- `app` 是组合根：执行 PostgreSQL I/O，把结果转换为 runtime 数据，并消费 projection 绘制界面。
- `ding-board` 保持独立；Cditor 仅在 `app` 做嵌入适配。

## 功能目录

同一功能的状态、渲染、交互和测试应放在同一目录，不再以大量同级前缀文件组织。

```text
crates/app/src/gui/
  block/
    code/             代码块容器、高亮、语言与主题工具栏
    mermaid/          Mermaid 缓存、渲染和主题
    table/            表格绘制、选择、菜单、缩放和重排
    whiteboard/       白板嵌入适配
  app/cditor_v2_view/
    formatting/       选区/块格式状态、颜色和修改动作
  diagnostics/        显式环境变量开启的诊断日志

crates/store-postgres/src/stores/document/
  mod.rs              文档结构索引和公共 store 类型
  metadata.rs         文档与 workspace 元数据
  attrs.rs            Block 属性
  snapshot.rs         DocumentIndex snapshot 编解码与查询
  tests.rs            文档存储测试
```

## 源码规则

- 非白板 Rust 文件不超过 700 行；超限必须按职责拆分，不能通过放宽阈值规避。
- 测试放在模块内小型 `tests` 模块、同级 `*_tests.rs`，或功能目录的 `tests.rs`。
- `runtime` 禁止依赖或引用 `cditor-storage-postgres`、SQLx、GPUI。
- PostgreSQL 查询只进入 `store-postgres`，应用级异步装配只进入 `app`。
- 一次性脚本进入 `scripts/archive/`；日常入口按 `dev/`、`database/`、`packaging/` 分类。
- 历史计划进入 `doc/archive/`，当前文档不得把历史路径描述为现状。
- 根目录只保留 workspace 清单、许可证、配置入口和顶层说明；构建输出与本地资产必须被忽略。
- 白板产品实现位于 `crates/ding-board`，编辑器侧适配位于 `crates/app/src/gui/block/whiteboard`；编辑器目录重构不得顺手改写白板实现。

`scripts/dev/check_structure.sh` 自动检查文件规模、系统垃圾文件和 runtime 边界；`scripts/dev/check_workspace.sh` 在此基础上执行 release 配置检查、格式化、全 workspace 编译和测试。
