# Legacy Refactor Plans

本目录保留 2026-07-22 之前的 Runtime/Editor 重构草案和进度模板，仅供追溯。

这些文档不再具有执行效力，原因包括：

- 建议将强耦合 GPUI 模块拆成 4-5 个 crate，会制造过多公开 API；
- 以“保持 100% Runtime API”为目标，与收敛 Command/Query/Projection 边界冲突；
- 包含 TBD 日期、虚假进度和已过时路径。

当前权威方案是 `doc/architecture/重构方案 0722.md`，执行证据以该文档的 R0-R9
清单和 `doc/acceptance/` 下的验收报告为准。
