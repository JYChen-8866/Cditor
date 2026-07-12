# Scripts

项目脚本按用途分组：

- `dev/`：日常开发、运行和本地验证入口。
- `database/`：PostgreSQL 环境初始化与远程隧道工具。
- `archive/workspace-migration/`：早期 workspace 拆分期间使用的迁移脚本，仅作历史参考，不应在当前目录结构上再次执行。

常用命令：

```bash
./scripts/dev/run_editor.sh
./scripts/dev/check_structure.sh
./scripts/dev/check_workspace.sh
./scripts/database/bootstrap_remote_postgres.sh
./scripts/database/open_remote_postgres_tunnel.sh
```

`check_structure.sh` 检查非白板源码的 700 行上限、废弃的 `crates/engine` 路径和系统元数据；`check_workspace.sh` 会先执行该检查，再运行格式、编译和测试。
