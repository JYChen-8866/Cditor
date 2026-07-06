# 远端 PostgreSQL 连接方式

远端服务：

```text
ssh host: edpb1492802.bohrium.tech
ssh port: 22
ssh user: root
```

> 不要把 SSH 密码写进代码、脚本、文档或 Git。首次连接时在终端交互输入密码即可。

## 推荐方案：SSH tunnel

远端 PostgreSQL 不应直接暴露公网端口。推荐用 SSH 隧道把远端数据库端口映射到本地端口。

### 1. 打开隧道

默认假设远端也使用本项目 `docker-compose.yml` 的 test 数据库端口：

```text
远端 127.0.0.1:5433 -> postgres_test / cditor_test
```

在一个单独终端运行：

```sh
./scripts/open_remote_postgres_tunnel.sh
```

等价于：

```sh
ssh -p 22 -N -L 15433:127.0.0.1:5433 root@edpb1492802.bohrium.tech
```

保持这个进程运行。

### 2. 使用隧道连接编辑器

另开一个终端：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@127.0.0.1:15433/cditor_test \
  cargo run --example minimal_postgres_editor
```

### 3. 如果远端实际使用 dev 数据库

如果远端只有 dev 数据库，隧道端口可能是远端 `5432`：

```sh
CDITOR_REMOTE_DB_PORT=5432 ./scripts/open_remote_postgres_tunnel.sh
```

然后使用：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@127.0.0.1:15433/cditor_dev \
  cargo run --example minimal_postgres_editor
```

## 远端确认命令

SSH 到服务器后可以确认 PostgreSQL / Docker 端口：

```sh
ssh root@edpb1492802.bohrium.tech
```

在远端执行：

```sh
docker ps --format 'table {{.Names}}\t{{.Ports}}'
ss -ltnp | grep -E '5432|5433'
```

如果看到类似：

```text
0.0.0.0:5433->5432/tcp
```

说明 tunnel 的默认 `REMOTE_PORT=5433` 是对的。

## 常用环境变量

`scripts/open_remote_postgres_tunnel.sh` 支持这些覆盖项：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `CDITOR_REMOTE_SSH_HOST` | `edpb1492802.bohrium.tech` | SSH 主机 |
| `CDITOR_REMOTE_SSH_PORT` | `22` | SSH 端口 |
| `CDITOR_REMOTE_SSH_USER` | `root` | SSH 用户 |
| `CDITOR_REMOTE_DB_LOCAL_PORT` | `15433` | 本地监听端口 |
| `CDITOR_REMOTE_DB_HOST` | `127.0.0.1` | 远端侧数据库地址 |
| `CDITOR_REMOTE_DB_PORT` | `5433` | 远端侧数据库端口 |

## 注意事项

- 不要提交 `.env` 或任何包含密码的文件。
- 如果本地 `15433` 被占用，可以换端口，例如：

```sh
CDITOR_REMOTE_DB_LOCAL_PORT=15434 ./scripts/open_remote_postgres_tunnel.sh
```

对应 URL：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@127.0.0.1:15434/cditor_test \
  cargo run --example minimal_postgres_editor
```

- 隧道断开后，编辑器的数据库连接会失败；重新打开 tunnel 后再启动编辑器。
