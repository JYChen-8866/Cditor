# 最简真实存储编辑器

这个入口用于验证最小真实场景：启动一个 GPUI 编辑器，数据真实读写 PostgreSQL。

## 入口

```sh
cargo run --example minimal_postgres_editor
```

## 默认行为

- 默认数据库：`postgres://cditor:cditor@localhost:5433/cditor_test`
- 默认文档 ID：`1`
- 默认 workspace ID：`1`
- 如果文档不存在，会自动创建一个空白真实 Postgres 文档：
  - 只有 1 个空 Paragraph block
  - 打开后就是空编辑器，可直接输入
- 后续编辑会走现有 GUI Postgres saver，停顿后自动保存到：
  - `documents`
  - `blocks`
  - `block_payloads`
  - `document_index_snapshots`
  - `edit_transactions`（有结构事务时）

## 启动 Postgres

```sh
docker compose up -d postgres
```

## 使用默认文档打开

```sh
cargo run --example minimal_postgres_editor
```

如果 Docker/Postgres 刚启动较慢，可以显式放宽初始化连接等待时间：

```sh
CDITOR_POSTGRES_TIMEOUT_SECS=30 cargo run --example minimal_postgres_editor
```

## 远端数据库

远端服务器数据库建议通过 SSH tunnel 连接，见：`doc/architecture/remote-postgres.md`。

常用启动方式：

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@127.0.0.1:15433/cditor_test \
  cargo run --example minimal_postgres_editor
```

## 指定文档打开

```sh
CDITOR_DATABASE_URL=postgres://cditor:cditor@localhost:5433/cditor_test \
CDITOR_DOCUMENT_ID=42 \
CDITOR_WORKSPACE_ID=1 \
CDITOR_DOCUMENT_TITLE="我的真实文档" \
cargo run --example minimal_postgres_editor
```

## 验证真实保存

1. 启动 example。
2. 修改正文内容。
3. 等待保存状态恢复。
4. 关闭窗口。
5. 重新运行同一个命令。
6. 修改内容应从 PostgreSQL 恢复。

## 设计边界

- 这个 example 不影响默认 `cargo run`，默认入口仍是 10w mixed demo。
- 这个 example 不 seed 10w demo，只创建一个空白 paragraph，保持空编辑器体验。
- 启动前初始化文档使用临时连接；真正打开编辑器时重新通过 `CDITOR_DATABASE_URL` 创建运行期 Postgres pool，避免复用已被临时 Tokio runtime drop 的 pool。
- 编辑器内部仍遵守 V2 架构：runtime 是文档/结构/滚动/projection 真相，UI 只消费 projection。
