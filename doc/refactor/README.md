# 重构计划与追踪

本目录包含 CDitor-V2 项目的重构方案和执行追踪文档。

## 📋 活跃重构计划

### P0 - 紧急
- [DocumentRuntime 重构方案](./document-runtime-refactoring-plan.md)
  - **状态**: ⬜ 规划中
  - **优先级**: P0（紧急）
  - **预计工期**: 4-6 周
  - **目标**: 拆解 God Object，提升可维护性

### P1 - 重要
- [cditor-editor 模块化方案](./cditor-editor-modularization-plan.md)
  - **状态**: ⬜ 规划中
  - **优先级**: P1（重要）
  - **预计工期**: 3-4 周
  - **前置条件**: DocumentRuntime 重构完成
  - **目标**: 拆分巨型 crate，支持并行开发

## 📊 重构优先级

```
P0 (紧急) → DocumentRuntime 拆解
P1 (重要) → cditor-editor 模块化
P2 (建议) → cditor-editor-core 重命名
P3 (优化) → 性能优化与代码清理
```

## 🎯 整体目标

1. **消除 God Object**：将 58 个字段的 DocumentRuntime 拆分为 7-8 个子系统
2. **模块化大型 Crate**：将 116 个文件的 cditor-editor 拆分为 4-5 个 crate
3. **提升可维护性**：降低认知负担，便于新人上手
4. **支持并行开发**：独立模块可以同时开发，加快迭代速度

## 📅 执行时间线

| 阶段 | 计划开始 | 计划结束 | 实际状态 |
|------|---------|---------|---------|
| DocumentRuntime Phase 0-7 | TBD | TBD | ⬜ 未开始 |
| cditor-editor Phase 1-4 | TBD | TBD | ⬜ 未开始 |

## 🔄 进度追踪

每周更新重构进度：
- [ ] Week 1: TBD
- [ ] Week 2: TBD
- [ ] Week 3: TBD

## 📝 相关文档

- [项目结构文档](../architecture/project-structure.md)
- [大文档架构设计](../large-document-rich-text-architecture.md)
- [工程结构分析](./cditor-v2-architecture-analysis.md)

---

**最后更新**: 2026-07-22
