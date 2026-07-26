# 2026-07-22 结构重构基线

> 分支：`codex/parley-text-layout`
>
> 基线提交：`a0f88d32`
>
> 权威方案：`doc/architecture/重构方案 0722.md`

## 1. 用途

本文固定结构重构开始前的 workspace 依赖、公开 API、状态字段和巨型文件基线。
后续验收必须与本基线比较，不能仅以“已移动文件”判定完成。

> 以下 crate 名称和路径是 `a0f88d32` 的迁移前历史快照，不代表当前 workspace。当前结构
> 以 `doc/architecture/project-structure.md` 为准。

## 2. Workspace 内部依赖

```text
cditor-ai:
cditor-api: cditor-ai, cditor-core, cditor-editor-core, cditor-storage, cditor-theme
cditor-app: cditor-api, cditor-core, cditor-editor, cditor-runtime, cditor-storage,
            cditor-storage-postgres, cditor-storage-sqlite
cditor-collaboration: cditor-core
cditor-core:
cditor-editor-core: cditor-core
cditor-editor: cditor-ai, cditor-api, cditor-core, cditor-editor-core,
                cditor-import-export, cditor-runtime, cditor-storage, cditor-text,
                cditor-theme, ding-board
cditor-import-export: cditor-core
cditor-runtime: cditor-ai, cditor-core, cditor-editor-core, cditor-import-export
cditor-storage-postgres: cditor-core, cditor-storage
cditor-storage-sqlite: cditor-core, cditor-runtime (dev), cditor-storage
cditor-storage: cditor-core
cditor-test-support: cditor-core, cditor-editor-core, cditor-runtime
cditor-text: cditor-core
cditor-theme-types:
cditor-theme: cditor-theme-types
ding-board:
```

可复现命令：

```bash
cargo metadata --no-deps --format-version=1 \
  | jq -r '.packages[] | .name as $n | [.dependencies[] | select(.path != null) | .name] as $deps | "\($n): \($deps | join(", "))"' \
  | sort
```

## 3. 状态和 API 基线

| 指标 | 基线 | 目标 |
| --- | ---: | --- |
| `DocumentRuntime` 平铺字段 | 49 | 按 owner 收入组合子状态 |
| `CditorV2View` 平铺字段 | 66 | 视图只持有 Session handle 和 UI 短状态 |
| `document_runtime/` 公开函数 | 271 | command/query/projection/realtime port |
| Runtime 全 crate 公开函数 | 401 | 只保留稳定边界和必要 value API |
| Editor 直接 `runtime.` 出现次数 | 476 | 文档 mutation 归零，读取收敛为 Projection/Query |
| 含 `runtime.` 的 Editor 文件 | 46 | 只允许 session adapter 边界文件 |
| Editor `dispatch_command` 调用点 | 24 | 所有 mutation 全部收敛后再审计 |

统计命令：

```bash
sed -n '/pub struct DocumentRuntime {/,/^}/p' \
  crates/cditor-runtime/src/document_runtime/state.rs | rg '^\s*pub.*:' | wc -l
sed -n '/pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/app/cditor_v2_view.rs | rg '^\s*pub.*:' | wc -l
rg '^\s*pub (async )?fn ' crates/cditor-runtime/src/document_runtime | wc -l
rg '^\s*pub (async )?fn ' crates/cditor-runtime/src | wc -l
rg '\bruntime\.' crates/cditor-editor-gpui/src | wc -l
rg -l '\bruntime\.' crates/cditor-editor-gpui/src | wc -l
rg 'dispatch_command' crates/cditor-editor-gpui/src | wc -l
```

## 4. 文件规模基线

- 非白板 Rust 源码总行数：130,241。
- `ding-board` Rust 代码总行数：12,117。
- `ding-board/src/lib.rs`：11,077 行。
- 非白板最大文件已由结构门禁控制在 700 行以下。

## 5. 行为与性能基线

- 100k mixed Runtime：`doc/acceptance/2026-07-22-100k-mixed-runtime-benchmark.md`。
- SQLite migration：`doc/acceptance/2026-07-22-sqlite-migration-orchestration.md`。
- unknown plugin round-trip：`doc/acceptance/2026-07-22-unknown-plugin-roundtrip.md`。
- 成熟编辑器行为清单：`doc/architecture/cditor-mature-notion-editor-master-design.md`。

重构期间不得降低上述报告的数据量、交互覆盖、版本校验或内存预算。

## 6. 基线测试记录

2026-07-22 在基线提交后执行 `cargo test --workspace`：

- 所有非 ignored 测试通过，0 失败；
- Core 228、Editor 394、Runtime 485、Text 56、Whiteboard 47 项单元测试通过；
- 100k mixed、输入预算、Parley visual/segmented、undo memory 验收路径通过；
- 需外部 PostgreSQL 测试库的项目按声明保持 ignored，不记作通过证据。
