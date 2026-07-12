# 远端 PostgreSQL 创建与连接

目标：在远端服务器上创建 PostgreSQL，然后本地编辑器直接连接：

```text
postgres://cditor:cditor@edpb1492802.bohrium.tech:5433/cditor_test
```

远端 SSH：

```text
host: edpb1492802.bohrium.tech
port: 22
user: root
```

> 不要把 SSH 密码写进代码、脚本、文档或 Git。运行脚本时在终端交互输入密码。

## 1. 先在远端创建数据库

本项目提供脚本在远端用 Docker 启动 PostgreSQL 16：

```sh
./scripts/database/bootstrap_remote_postgres.sh
```

默认会在远端创建：

| 项 | 值 |
| --- | --- |
| 容器 | `cditor-postgres-test` |
| 数据库用户 | `cditor` |
| 数据库密码 | `cditor` |
| 数据库名 | `cditor_test` |
| 远端端口 | `5433` |
| 数据目录 | Docker volume `cditor_postgres_test_data` |
| 远端 compose 目录 | `/opt/cditor-v2-postgres` |

脚本成功后，数据库地址是：

```text
postgres://cditor:cditor@edpb1492802.bohrium.tech:5433/cditor_test
```

如果想换端口或库名：

```sh
CDITOR_REMOTE_POSTGRES_PORT=5432 \
CDITOR_REMOTE_POSTGRES_DB=cditor_dev \
./scripts/database/bootstrap_remote_postgres.sh
```

## 2. 直接连接远端数据库启动编辑器

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@edpb1492802.bohrium.tech:5433/cditor_test \
  cargo run --example minimal_postgres_editor
```

说明：

- `minimal_postgres_editor` 启动后会自动运行 `migrations/0001_initial.sql` 建表。
- 如果指定文档不存在，会自动创建一个空白文档。
- PostgreSQL 服务、用户、数据库本身必须先存在，所以需要先执行第 1 步。

## 3. 验证远端端口

如果连接失败，先检查远端端口是否开放：

```sh
nc -vz edpb1492802.bohrium.tech 5433
```

如果失败，SSH 到远端看容器和监听端口：

```sh
ssh root@edpb1492802.bohrium.tech
```

远端执行：

```sh
docker ps --format 'table {{.Names}}\t{{.Ports}}'
ss -ltnp | grep -E '5432|5433'
```

需要看到类似：

```text
0.0.0.0:5433->5432/tcp
```

## 4. 备选：SSH tunnel

如果服务器防火墙不允许直接访问 `5433`，再用隧道：

```sh
./scripts/database/open_remote_postgres_tunnel.sh
```

另一个终端：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@127.0.0.1:15433/cditor_test \
  cargo run --example minimal_postgres_editor
```
