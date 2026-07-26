# SQLite 100k 本地持久化基准（P7-016）

日期：2026-07-25  
Harness：`sqlite-local-storage-v1` / report schema 1  
命令：`cargo bench -p cditor-test-support --bench sqlite_local_storage -- --full`

## 范围与方法

本基准通过公开 `SqliteDocumentStorage`/`DocumentStorage` API 建立真实 WAL 数据库，不使用内存 storage 或模拟延迟。语料为 100,000 个 mixed heading/paragraph/list/code Block；首次物化完成后，每个 open 样本重新创建 SQLite pool 并加载完整 index 与最多 128 个 initial payload。操作系统 page cache 不受 harness 控制，因此这里测量的是应用实际“本地已有数据库的重开”，不是清除系统缓存后的裸盘读取。

durable save 的 50 个样本均在一个事务内更新单 block payload、FTS/backlink projection、operation journal、edit transaction 与 sync outbox。结构保存样本重写完整 100k index，用于显式暴露当前 O(n) 路径；checkpoint 捕获完整 materialized state；compact 只删除已经 checkpoint 且 outbox 已 Acked 的 operation。所有分布使用 nearest-rank p50/p95。

机器与构建：macOS / aarch64，10 logical cores，Cargo `bench` optimized profile。

## 结果

| 场景 | 样本 | p50 | p95 | max | Gate |
|---|---:|---:|---:|---:|---|
| reopen + 100k index + 128 payload | 12 | 74.80ms | 76.39ms | 76.39ms | 通过，p95 < 250ms |
| durable single-block save | 50 | 4.73ms | 5.83ms | 14.04ms | 通过，p95 < 50ms |
| full 100k structure rewrite | 1 | 1770.25ms | 1770.25ms | 1770.25ms | 观察项，只允许后台执行 |
| materialized checkpoint | 3 | 552.78ms | 640.58ms | 640.58ms | 观察项，只允许后台/idle 执行 |
| compact 50 acked operations | 1 | 0.41ms | 0.41ms | 0.41ms | 50 条全部删除 |
| WAL passive flush | 1 | 0.35ms | 0.35ms | 0.35ms | 通过 |

首次 100k seed 为 3517.08ms。首窗严格保持 128 payload，完整 index 为 100,000 Block。flush 前数据库/WAL/SHM 分别为 94,883,840 / 95,748,832 / 196,608 bytes；checkpoint、compact 和 flush 后分别为 152,379,392 / 40,771,552 / 98,304 bytes。

## 修复与结论

初次 full 运行暴露了 FTS5 identity 删除的近似 O(n²) 路径：`document_id`/`block_id` 是 FTS `UNINDEXED` 列，却被用于每 block `DELETE`。运行 10 分钟仍未完成，因此中止，不把异常实现记录为可接受基线。`0008_fts_rowid_projection.sql` 现在用普通 `block_fts_state` 主键映射唯一 FTS rowid，commit/rebuild 对 rowid、state 和 backlinks 分批写入。修复后 4,096 Block seed 从 1582.73ms 降至 155.94ms，100k full 在 3.52s 完成；单 block 替换复用 rowid，不产生重复 FTS 行。

本地普通事务满足 p95 预算。Editor 的保存调用由 GPUI `cx.background_spawn` 调度，SQLite I/O 在 background task/Session I/O runtime 执行，完成结果才回到 UI entity，因此数据库工作不占 input 主线程。

仍需保持的边界：当前完整结构 snapshot 写入为 1.77s，checkpoint 为约 0.64s，均不能由输入事件同步等待。结构 mutation 长期应从“structure version 变化即重写完整 index”演进为有序结构 delta + 后台 snapshot；本基准保留 full rewrite 指标以防该成本被隐藏。

机器可读结果：`target/benchmark-reports/sqlite-local-storage-full.json`（构建产物，不提交）。
