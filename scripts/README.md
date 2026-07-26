# Scripts

项目脚本按用途分组：

- `dev/`：日常开发、运行和本地验证入口。
- `database/`：PostgreSQL 环境初始化与远程隧道工具。
- `packaging/`：桌面应用打包脚本；GitHub Actions 使用它生成 macOS `.app` 和 `.dmg`。
- `archive/workspace-migration/`：早期 workspace 拆分期间使用的迁移脚本，仅作历史参考，不应在当前目录结构上再次执行。

常用命令：

```bash
./scripts/dev/run_editor_postgres.sh
./scripts/dev/run_editor_sqlite.sh
./scripts/dev/check_structure.sh
./scripts/dev/check_dependency_graph.sh
./scripts/dev/check_feature_matrix.sh
./scripts/dev/check_workspace.sh
./scripts/database/bootstrap_remote_postgres.sh
./scripts/database/open_remote_postgres_tunnel.sh
```

编辑器后端启动入口：

```bash
# PostgreSQL；默认连接本地 docker-compose 的 cditor_dev。
docker compose up -d postgres
./scripts/dev/run_editor_postgres.sh

# SQLite；默认数据库为项目根目录 workspace.cditor.db。
./scripts/dev/run_editor_sqlite.sh
```

两个脚本默认使用 workspace 的 `editor-dev` profile。它会优化 GPUI、Taffy 和文本布局
热路径，同时保留增量编译、调试断言和有限调试信息，适合日常运行大文档。默认的 Cargo
`dev` profile 仍可用于纯调试；发布构建继续使用独立的 `release` profile。

显式 Cargo profile 参数优先，不会与脚本默认值叠加：

```bash
./scripts/dev/run_editor_sqlite.sh --release
./scripts/dev/run_editor_postgres.sh --profile editor-dev
```

两个后端脚本都默认打开 document `1`，并支持下列覆盖变量：

| 脚本 | 环境变量 | 默认值 |
| --- | --- | --- |
| PostgreSQL | `CDITOR_DATABASE_URL` | 本地 `cditor_dev` URL |
| PostgreSQL | `CDITOR_DOCUMENT_ID` | `1` |
| SQLite | `CDITOR_SQLITE_PATH` | `./workspace.cditor.db` |
| SQLite | `CDITOR_DOCUMENT_ID` | `1` |

脚本会显式清除另一个后端的选择变量，避免 shell 中遗留的环境变量选错后端。
`CDITOR_DRY_RUN=1` 可仅验证配置而不启动 GUI；PostgreSQL URL 的值不会输出到终端。

`check_structure.sh` 检查所有 Rust 源码的 700 行上限、废弃路径、Core/Runtime/GPUI 依赖边界，以及 Parley 只能由 `cditor-text` 直接使用；`check_workspace.sh` 会先执行结构、依赖图、feature matrix，再运行格式、严格 Clippy 和测试。
