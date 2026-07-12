# Documentation Index

文档以 [大文档富文本架构](large-document-rich-text-architecture.md) 为最高设计基线。新增实现应先确认文档状态，避免把历史迁移方案当成当前结构。

## 当前架构与状态

- [大文档富文本架构](large-document-rich-text-architecture.md)
- [大文档实现状态](large-document-rich-text-implementation-status.md)
- [当前工程结构](architecture/project-structure.md)
- [V2 GUI 架构](architecture/v2-rich-text-editor-gui-architecture.md)
- [数据库实现方案](architecture/database-implementation-plan.md)
- [PostgreSQL 最小编辑器](architecture/minimal-postgres-editor.md)
- [远程 PostgreSQL](architecture/remote-postgres.md)
- [白板集成架构](whiteboard-integration-architecture.md)

## 功能计划与验收

- [当前编辑器问题与任务清单](plans/current-editor-issues-deep-analysis-and-task-list.md)
- [大文档任务清单](plans/large-document-rich-text-task-list.md)
- [表格功能计划](plans/notion-table-feature-plan.md)
- [表格交互重设计](plans/notion-table-interaction-redesign.md)
- [表格手动验收](acceptance/table-manual-acceptance.md)
- [表格完成总结](acceptance/table-completion-summary.md)
- [架构原型](prototypes/)

## 重构设计

- [模块拆分计划](refactor/module-split-plan.md)
- [骨架屏加载计划](refactor/skeleton-loading-plan.md)

## 历史迁移资料

[历史迁移目录](archive/migrations/README.md) 用于记录迁移背景，不代表当前目录和命令。当前 crate 与脚本入口以项目根目录 [README](../README.md) 为准。
