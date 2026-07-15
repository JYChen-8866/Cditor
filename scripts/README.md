# Scripts

项目脚本按用途分组：

- `dev/`：日常开发、运行和本地验证入口。
- `database/`：PostgreSQL 环境初始化与远程隧道工具。
- `packaging/`：桌面应用打包脚本；GitHub Actions 使用它生成 macOS `.app` 和 `.dmg`。
- `archive/workspace-migration/`：早期 workspace 拆分期间使用的迁移脚本，仅作历史参考，不应在当前目录结构上再次执行。

常用命令：

```bash
./scripts/dev/run_editor.sh
./scripts/dev/run_editor_postgres.sh
./scripts/dev/run_editor_sqlite.sh
./scripts/dev/check_structure.sh
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

`run_editor.sh` 为兼容入口，等价于 `run_editor_postgres.sh`。两个脚本都默认打开
document `1`，并支持下列覆盖变量：

| 脚本 | 环境变量 | 默认值 |
| --- | --- | --- |
| PostgreSQL | `CDITOR_DATABASE_URL` | 本地 `cditor_dev` URL |
| PostgreSQL | `CDITOR_DOCUMENT_ID` | `1` |
| SQLite | `CDITOR_SQLITE_PATH` | `./workspace.cditor.db` |
| SQLite | `CDITOR_DOCUMENT_ID` | `1` |

脚本会显式清除另一个后端的选择变量，避免 shell 中遗留的环境变量选错后端。
`CDITOR_DRY_RUN=1` 可仅验证配置而不启动 GUI；PostgreSQL URL 的值不会输出到终端。

`check_structure.sh` 检查非白板源码的 700 行上限、废弃的 `crates/engine` 路径和系统元数据；`check_workspace.sh` 会先执行该检查，再运行格式、编译和测试。
